import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:dav_bridge_mobile/src/models/bridge_models.dart';
import 'package:dav_bridge_mobile/src/rust/rust_bridge_api.dart';
import 'package:dav_bridge_mobile/src/services/background_sync_service.dart';
import 'package:dav_bridge_mobile/src/services/sync_runtime_storage.dart';
import 'package:dav_bridge_mobile/src/services/system_projection_service.dart';
import 'package:dav_bridge_mobile/src/services/token_storage_service.dart';
import 'package:dav_bridge_mobile/src/state/bridge_state.dart';

/// Bridge implementation provider (`MockRustBridgeApi` or FRB-backed bridge).
final rustBridgeProvider = Provider<RustBridgeApi>(
  (ref) => RustBridgeFactory.create(),
);

/// Provider for secure refresh-token persistence on mobile.
final refreshTokenStorageProvider = Provider<RefreshTokenStorage>((ref) {
  return SecureRefreshTokenStorage();
});

final syncRuntimeStorageProvider = Provider<SyncRuntimeStorage>((ref) {
  return SecureSyncRuntimeStorage();
});

final localCacheKeyStorageProvider = Provider<LocalCacheKeyStorage>((ref) {
  return SecureLocalCacheKeyStorage();
});

final deviceVaultStorageProvider = Provider<DeviceVaultStorage>((ref) {
  return SecureDeviceVaultStorage();
});

final systemProjectionServiceProvider = Provider<SystemProjectionService>(
  (ref) => NativeSystemProjectionService(),
);

final mobilePlatformProvider = Provider<String>((ref) {
  return defaultTargetPlatform == TargetPlatform.iOS ? 'ios' : 'android';
});

/// Injectable callback for scheduling periodic background sync.
final schedulePeriodicSyncProvider = Provider<Future<void> Function()>((ref) {
  return () => BackgroundSyncService.schedulePeriodicSync();
});

/// Injectable callback for cancelling periodic background sync.
final cancelPeriodicSyncProvider = Provider<Future<void> Function()>((ref) {
  return () => BackgroundSyncService.cancelPeriodicSync();
});

/// Injectable clock provider to make time-dependent code deterministic in tests.
final nowProvider = Provider<DateTime Function()>((ref) => DateTime.now);

/// Root state provider for bridge UI/state actions.
final bridgeControllerProvider =
    NotifierProvider<BridgeController, BridgeState>(BridgeController.new);

class BridgeController extends Notifier<BridgeState> {
  RustBridgeApi get _rustBridge => ref.read(rustBridgeProvider);
  RefreshTokenStorage get _refreshTokenStorage =>
      ref.read(refreshTokenStorageProvider);
  SyncRuntimeStorage get _syncRuntimeStorage =>
      ref.read(syncRuntimeStorageProvider);
  LocalCacheKeyStorage get _localCacheKeyStorage =>
      ref.read(localCacheKeyStorageProvider);
  DeviceVaultStorage get _deviceVaultStorage =>
      ref.read(deviceVaultStorageProvider);
  SystemProjectionService get _systemProjectionService =>
      ref.read(systemProjectionServiceProvider);
  Future<void> Function() get _schedulePeriodicSync =>
      ref.read(schedulePeriodicSyncProvider);
  Future<void> Function() get _cancelPeriodicSync =>
      ref.read(cancelPeriodicSyncProvider);
  DateTime Function() get _now => ref.read(nowProvider);

  @override
  BridgeState build() => BridgeState.initial();

  Future<void> restorePersistedSession() async {
    try {
      await _retryPendingRevocation();
      final snapshot = await _syncRuntimeStorage.read();
      if (snapshot == null) {
        return;
      }
      final vault = snapshot.username == null
          ? await _deviceVaultStorage.read()
          : await _deviceVaultStorage.read(
              cloudBaseUrl: snapshot.cloudBaseUrl,
              username: snapshot.username,
            );
      if (vault == null ||
          vault.cloudBaseUrl != snapshot.cloudBaseUrl ||
          (snapshot.username != null && vault.username != snapshot.username)) {
        throw StateError(
            'The persisted session does not match a device vault.');
      }
      state = state.copyWith(
        isAuthenticated: true,
        accessToken: snapshot.accessToken,
        username: vault.username,
        cloudBaseUrl: snapshot.cloudBaseUrl,
        sqlitePath: snapshot.sqlitePath,
        collections: snapshot.collections,
        backgroundSyncEnabled: snapshot.backgroundSyncEnabled,
      );
      await _hydrateRefreshTokenIntoRuntime();
      await _configureSyncForActiveSession();
      try {
        await _reconcileProvisioning();
      } catch (error) {
        // Keep the decrypted offline cache available when the server cannot be
        // reached. The next manual/background sync retries reconciliation.
        state = state.copyWith(
          error:
              'Offline data is available; account reconciliation will retry: $error',
        );
      }
      await _reloadPimItems();
      await _loadProjectionSettings();
      await _projectSystemCopies(state.pimItems);
      if (state.backgroundSyncEnabled) {
        await _schedulePeriodicSync();
      }
    } catch (error) {
      state = state.copyWith(
        isAuthenticated: false,
        clearAccessToken: true,
        clearUsername: true,
        collections: const <CollectionEntry>[],
        pimItems: const <PimItem>[],
        error: 'Failed to restore secure session: $error',
      );
    }
  }

  /// Updates cloud backend base URL for next bridge start.
  Future<void> updateCloudBaseUrl(String value) async {
    final normalized = _validatedCloudBaseUrl(value);
    if (normalized == null) {
      state = state.copyWith(
        error: kDebugMode
            ? 'Use HTTPS, or HTTP only for a localhost debug server.'
            : 'The server address must be an HTTPS origin.',
      );
      return;
    }
    if (state.isAuthenticated && normalized != state.cloudBaseUrl) {
      state = state.copyWith(
        error: 'Sign out before changing the server address.',
      );
      return;
    }
    state = state.copyWith(cloudBaseUrl: normalized, clearError: true);
  }

  /// Performs password login and configures offline sync on success.
  Future<void> loginWithPassword({
    required String username,
    required String password,
    String? totpCode,
  }) async {
    if (username.trim().isEmpty || password.isEmpty) {
      state = state.copyWith(error: 'Username and password are required.');
      return;
    }
    if (state.isAuthenticated) {
      state = state.copyWith(error: 'Sign out before switching accounts.');
      return;
    }

    state = state.copyWith(isBusy: true, clearError: true);
    LoginResult? login;
    try {
      login = await _rustBridge.passwordLogin(
        cloudBaseUrl: state.cloudBaseUrl,
        username: username.trim(),
        password: password,
        totpCode: totpCode?.trim().isEmpty == true ? null : totpCode?.trim(),
      );

      if (login.accessToken == null) {
        state = state.copyWith(
          isBusy: false,
          error: login.totpContinuationToken != null
              ? 'TOTP is required. Enter your code and try again.'
              : 'Password login failed.',
        );
        return;
      }

      await _persistRuntimeRefreshToken();
      await _completeAuthenticatedLogin(login);
      await _configureSyncForActiveSession();
      await _loadProjectionSettings();
      if (state.backgroundSyncEnabled) {
        await _schedulePeriodicSync();
      }
      await _persistSyncRuntime();
      state = state.copyWith(isBusy: false);
    } catch (error) {
      final rollbackWarning =
          login?.accessToken == null ? null : await _rollbackIncompleteLogin();
      state = state.copyWith(
        isBusy: false,
        isAuthenticated: false,
        clearAccessToken: true,
        clearUsername: true,
        collections: const <CollectionEntry>[],
        error: rollbackWarning == null
            ? 'Login failed: $error'
            : 'Login failed: $error. $rollbackWarning',
      );
    }
  }

  /// Clears the authenticated session and local token state.
  Future<void> logout() async {
    state = state.copyWith(isBusy: true, clearError: true);
    try {
      String? serverWarning;
      final refreshToken = await _refreshTokenStorage.read(
        cloudBaseUrl: state.cloudBaseUrl,
      );
      if (refreshToken != null) {
        try {
          await _rustBridge.revokeRefreshSession(
            cloudBaseUrl: state.cloudBaseUrl,
            refreshToken: refreshToken,
          );
          await _refreshTokenStorage.deleteQueuedRevocation();
        } catch (error) {
          await _refreshTokenStorage.queueRevocation(
            cloudBaseUrl: state.cloudBaseUrl,
            refreshToken: refreshToken,
          );
          serverWarning =
              'Signed out locally. Server session revocation will retry when online.';
        }
      }
      await _clearRefreshTokenState();
      await _syncRuntimeStorage.delete();
      await _cancelPeriodicSync();
      state = state.copyWith(
        isBusy: false,
        isAuthenticated: false,
        clearAccessToken: true,
        clearUsername: true,
        collections: const <CollectionEntry>[],
        pimItems: const <PimItem>[],
        syncedItemsTotal: 0,
        clearLastSyncAt: true,
        calendarProjectionCollectionIds: const <String>{},
        contactsProjectionCollectionIds: const <String>{},
        error: serverWarning,
      );
    } catch (error) {
      state = state.copyWith(isBusy: false, error: 'Logout failed: $error');
    }
  }

  Future<void> _retryPendingRevocation() async {
    final pending = await _refreshTokenStorage.readQueuedRevocation();
    if (pending == null || pending.refreshToken.trim().isEmpty) return;
    await _rustBridge.revokeRefreshSession(
      cloudBaseUrl: pending.cloudBaseUrl,
      refreshToken: pending.refreshToken,
    );
    await _refreshTokenStorage.deleteQueuedRevocation();
  }

  /// Triggers immediate manual sync.
  Future<void> syncNow() async {
    state = state.copyWith(isBusy: true, clearError: true);
    try {
      await _hydrateRefreshTokenIntoRuntime();
      await _reconcileProvisioning();
      final synced = await _rustBridge.syncNow();
      final pimItems = await _rustBridge.listPimItems();
      await _persistRuntimeRefreshToken();
      state = state.copyWith(
        isBusy: false,
        syncedItemsTotal: state.syncedItemsTotal + synced,
        lastSyncAt: _now(),
        pimItems: pimItems,
      );
      await _projectSystemCopies(pimItems);
    } catch (error) {
      state = state.copyWith(isBusy: false, error: 'Sync failed: $error');
    }
  }

  Future<void> loadPimItems() async {
    try {
      await _reloadPimItems();
    } catch (error) {
      state = state.copyWith(error: 'Failed to load organizer items: $error');
    }
  }

  void clearError() {
    state = state.copyWith(clearError: true);
  }

  Future<bool> savePimItem(PimDraft draft) async {
    if (!_canWriteSpace(draft.spaceId)) {
      state = state.copyWith(
        error: 'This space is read-only. Ask an owner for editor access.',
      );
      return false;
    }
    state = state.copyWith(isBusy: true, clearError: true);
    try {
      await _hydrateRefreshTokenIntoRuntime();
      final item = await _rustBridge.upsertPimItem(draft: draft);
      await _persistRuntimeRefreshToken();
      final items = [...state.pimItems]
        ..removeWhere(
          (existing) =>
              existing.spaceId == item.spaceId &&
              existing.projectionId == item.projectionId,
        )
        ..add(item);
      items.sort((left, right) => left.title.compareTo(right.title));
      state = state.copyWith(isBusy: false, pimItems: items);
      await _projectSystemCopies(items);
      return true;
    } catch (error) {
      state =
          state.copyWith(isBusy: false, error: 'Failed to save item: $error');
      return false;
    }
  }

  Future<void> deletePimItem(PimItem item) async {
    if (!_canWriteSpace(item.spaceId)) {
      state = state.copyWith(
        error: 'This space is read-only. Ask an owner for editor access.',
      );
      return;
    }
    state = state.copyWith(isBusy: true, clearError: true);
    try {
      await _hydrateRefreshTokenIntoRuntime();
      await _rustBridge.deletePimItem(item: item);
      await _persistRuntimeRefreshToken();
      state = state.copyWith(
        isBusy: false,
        pimItems: state.pimItems
            .where(
              (existing) =>
                  existing.spaceId != item.spaceId ||
                  existing.projectionId != item.projectionId,
            )
            .toList(growable: false),
      );
      await _projectSystemCopies(state.pimItems);
    } catch (error) {
      state =
          state.copyWith(isBusy: false, error: 'Failed to delete item: $error');
    }
  }

  Future<void> _reloadPimItems() async {
    if (state.collections.isEmpty) {
      state = state.copyWith(pimItems: const <PimItem>[]);
      return;
    }
    state = state.copyWith(pimItems: await _rustBridge.listPimItems());
  }

  /// Creates a server-backed encrypted security space.
  Future<void> createCollection(String name) async {
    final collectionName = name.trim();
    if (collectionName.isEmpty) {
      state = state.copyWith(error: 'Collection name is required.');
      return;
    }

    state = state.copyWith(isBusy: true, clearError: true);
    try {
      await _hydrateRefreshTokenIntoRuntime();
      await _ensureProvisioningSecrets();
      final collection = await _rustBridge.createCollection(
        name: collectionName,
      );
      await _persistRuntimeRefreshToken();
      state = state.copyWith(
        isBusy: false,
        collections: <CollectionEntry>[...state.collections, collection],
      );
      await _persistSyncRuntime();
    } catch (error) {
      state = state.copyWith(
        isBusy: false,
        error: 'Failed to create encrypted space: $error',
      );
    }
  }

  /// Moves an owned collection to the server-side 30-day trash.
  Future<void> deleteCollection(String collectionId) async {
    final updated = state.collections
        .where((collection) => collection.id != collectionId)
        .toList(growable: false);

    if (updated.length == state.collections.length) {
      state = state.copyWith(error: 'Collection not found.');
      return;
    }

    state = state.copyWith(isBusy: true, clearError: true);
    try {
      await _hydrateRefreshTokenIntoRuntime();
      await _rustBridge.moveCollectionToTrash(collectionId: collectionId);
      await _persistRuntimeRefreshToken();
      String? projectionWarning;
      if (state.calendarProjectionCollectionIds.contains(collectionId)) {
        try {
          await _systemProjectionService.disableCalendar(
            collectionId,
            removeProjectedData: true,
          );
        } catch (error) {
          projectionWarning = 'Calendar projection cleanup failed: $error';
        }
      }
      if (state.contactsProjectionCollectionIds.contains(collectionId)) {
        try {
          await _systemProjectionService.disableContacts(
            collectionId,
            removeProjectedData: true,
          );
        } catch (error) {
          projectionWarning = [
            if (projectionWarning != null) projectionWarning,
            'Contacts projection cleanup failed: $error',
          ].join(' ');
        }
      }
      state = state.copyWith(
        isBusy: false,
        collections: updated,
        pimItems: state.pimItems
            .where((item) => item.spaceId != collectionId)
            .toList(growable: false),
        calendarProjectionCollectionIds: {
          ...state.calendarProjectionCollectionIds,
        }..remove(collectionId),
        contactsProjectionCollectionIds: {
          ...state.contactsProjectionCollectionIds,
        }..remove(collectionId),
        error: projectionWarning,
        clearError: projectionWarning == null,
      );
      await _persistSyncRuntime();
    } catch (error) {
      state = state.copyWith(
        isBusy: false,
        error: 'Failed to move encrypted space to trash: $error',
      );
    }
  }

  /// Issues a short-lived invite code for the selected collection.
  Future<IssuedInviteCode?> createInviteCode({
    required String collectionId,
    required int ttlMinutes,
  }) async {
    CollectionEntry? collection;
    for (final item in state.collections) {
      if (item.id == collectionId) {
        collection = item;
        break;
      }
    }

    if (collection == null) {
      state = state.copyWith(error: 'Collection not found.');
      return null;
    }

    state = state.copyWith(isBusy: true, clearError: true);
    try {
      await _hydrateRefreshTokenIntoRuntime();
      final issued = await _rustBridge.createInviteCode(
        collectionId: collection.id,
        collectionKey: collection.cmk,
        ttlMinutes: ttlMinutes,
      );
      if (issued.collectionKey.length != 32 ||
          issued.keyEpoch <= collection.keyEpoch) {
        throw StateError(
            'Invite creation returned invalid rotated key material.');
      }
      final updated = state.collections
          .map(
            (item) => item.id == collection!.id
                ? CollectionEntry(
                    id: item.id,
                    name: item.name,
                    cmk: issued.collectionKey,
                    keyEpoch: issued.keyEpoch,
                    historyStartSeq: item.historyStartSeq,
                    currentStateStartSeq: issued.currentStateStartSeq,
                    role: item.role,
                  )
                : item,
          )
          .toList(growable: false);
      await _persistRuntimeRefreshToken();
      state = state.copyWith(isBusy: false, collections: updated);
      await _persistSyncRuntime();
      return issued;
    } catch (error) {
      state = state.copyWith(
        isBusy: false,
        error: 'Failed to create invite code: $error',
      );
      return null;
    }
  }

  /// Redeems an invite code and stores the collection key locally.
  Future<RedeemedInvite?> redeemInviteCode(String inviteCode) async {
    final trimmed = inviteCode.trim();
    if (trimmed.isEmpty) {
      state = state.copyWith(error: 'Invite code is required.');
      return null;
    }

    state = state.copyWith(isBusy: true, clearError: true);
    try {
      await _hydrateRefreshTokenIntoRuntime();
      await _ensureProvisioningSecrets();
      final redeemed = await _rustBridge.redeemInviteCode(inviteCode: trimmed);
      await _persistRuntimeRefreshToken();

      final current = [...state.collections];
      final existingIndex = current
          .indexWhere((collection) => collection.id == redeemed.collectionId);
      if (existingIndex >= 0) {
        current[existingIndex] = CollectionEntry(
          id: current[existingIndex].id,
          name: current[existingIndex].name,
          cmk: redeemed.collectionKey,
          keyEpoch: redeemed.keyEpoch,
          historyStartSeq: redeemed.historyStartSeq,
          currentStateStartSeq: redeemed.currentStateStartSeq,
          role: redeemed.role,
        );
      } else {
        current.add(
          CollectionEntry(
            id: redeemed.collectionId,
            name: 'Shared ${redeemed.collectionId.substring(0, 8)}',
            cmk: redeemed.collectionKey,
            keyEpoch: redeemed.keyEpoch,
            historyStartSeq: redeemed.historyStartSeq,
            currentStateStartSeq: redeemed.currentStateStartSeq,
            role: redeemed.role,
          ),
        );
      }

      state = state.copyWith(isBusy: false, collections: current);
      await _persistSyncRuntime();
      return redeemed;
    } catch (error) {
      state = state.copyWith(
        isBusy: false,
        error: 'Failed to redeem invite code: $error',
      );
      return null;
    }
  }

  /// Enables/disables periodic background sync scheduling.
  Future<void> setBackgroundSyncEnabled(bool enabled) async {
    state = state.copyWith(backgroundSyncEnabled: enabled, clearError: true);
    if (!state.isAuthenticated) {
      return;
    }

    try {
      await _persistSyncRuntime();
      if (enabled) {
        await _schedulePeriodicSync();
      } else {
        await _cancelPeriodicSync();
      }
    } catch (error) {
      state = state.copyWith(error: 'Failed to update background sync: $error');
    }
  }

  Future<void> setCalendarProjectionEnabled(
    String collectionId,
    bool enabled, {
    bool removeProjectedData = false,
  }) async {
    if (!state.collections.any((collection) => collection.id == collectionId)) {
      state = state.copyWith(error: 'Collection not found.');
      return;
    }
    state = state.copyWith(isBusy: true, clearError: true);
    try {
      if (enabled) {
        await _systemProjectionService.enableCalendar(
          collectionId,
          state.pimItems,
        );
      } else {
        await _systemProjectionService.disableCalendar(
          collectionId,
          removeProjectedData: removeProjectedData,
        );
      }
      final enabledCollections = {
        ...state.calendarProjectionCollectionIds,
      };
      if (enabled) {
        enabledCollections.add(collectionId);
      } else {
        enabledCollections.remove(collectionId);
      }
      state = state.copyWith(
        isBusy: false,
        calendarProjectionCollectionIds: enabledCollections,
      );
    } catch (error) {
      final settings = await _systemProjectionService.readSettings();
      state = state.copyWith(
        isBusy: false,
        calendarProjectionCollectionIds: settings.calendarCollectionIds,
        error: 'Failed to update Calendar integration: $error',
      );
    }
  }

  Future<void> setContactsProjectionEnabled(
    String collectionId,
    bool enabled, {
    bool removeProjectedData = false,
  }) async {
    if (!state.collections.any((collection) => collection.id == collectionId)) {
      state = state.copyWith(error: 'Collection not found.');
      return;
    }
    state = state.copyWith(isBusy: true, clearError: true);
    try {
      if (enabled) {
        await _systemProjectionService.enableContacts(
          collectionId,
          state.pimItems,
        );
      } else {
        await _systemProjectionService.disableContacts(
          collectionId,
          removeProjectedData: removeProjectedData,
        );
      }
      final enabledCollections = {
        ...state.contactsProjectionCollectionIds,
      };
      if (enabled) {
        enabledCollections.add(collectionId);
      } else {
        enabledCollections.remove(collectionId);
      }
      state = state.copyWith(
        isBusy: false,
        contactsProjectionCollectionIds: enabledCollections,
      );
    } catch (error) {
      final settings = await _systemProjectionService.readSettings();
      state = state.copyWith(
        isBusy: false,
        contactsProjectionCollectionIds: settings.contactsCollectionIds,
        error: 'Failed to update Contacts integration: $error',
      );
    }
  }

  /// Persists rotated credentials across app lifecycle transitions.
  Future<void> handleLifecycleState(AppLifecycleState lifecycleState) async {
    if (!state.isAuthenticated) {
      return;
    }

    if (lifecycleState != AppLifecycleState.resumed) {
      try {
        await _persistRuntimeRefreshToken();
      } catch (_) {
        // best-effort credential persistence
      }
      return;
    }

    try {
      await _hydrateRefreshTokenIntoRuntime();
      await _reconcileProvisioning();
    } catch (_) {
      // best-effort lifecycle resume
    }
  }

  /// Configures sync and re-registers all in-memory collection keys.
  Future<void> _configureSyncForActiveSession() async {
    final token = state.accessToken;
    if (token == null || token.isEmpty) {
      state = state.copyWith(error: 'Access token is missing.');
      return;
    }

    final username = state.username;
    if (username == null || username.isEmpty) {
      throw StateError('The authenticated account identity is unavailable.');
    }
    final vault = await _deviceVaultStorage.read(
      cloudBaseUrl: state.cloudBaseUrl,
      username: username,
    );
    if (vault == null || vault.cloudBaseUrl != state.cloudBaseUrl) {
      throw StateError('This mobile device has not been provisioned.');
    }
    await _hydrateRefreshTokenIntoRuntime();
    await _rustBridge.configureSync(
      cloudBaseUrl: state.cloudBaseUrl,
      sqlitePath: state.sqlitePath,
      accessToken: token,
      sqliteKey: await _localCacheKeyStorage.readOrCreate(),
      device: vault.device,
    );

    for (final collection in state.collections) {
      await _rustBridge.registerCollectionKey(
        collectionId: collection.id,
        keyEpoch: collection.keyEpoch,
        syncStartSeq:
            collection.historyStartSeq > collection.currentStateStartSeq
                ? collection.historyStartSeq
                : collection.currentStateStartSeq,
        cmk: collection.cmk,
      );
    }

    await _persistRuntimeRefreshToken();
  }

  Future<void> _completeAuthenticatedLogin(LoginResult login) async {
    final accessToken = login.accessToken;
    final username = login.username;
    final accountMasterKey = login.accountMasterKey;
    final deviceEnrollmentToken = login.deviceEnrollmentToken;
    if (accessToken == null ||
        username == null ||
        username.isEmpty ||
        accountMasterKey == null ||
        accountMasterKey.length != 32 ||
        deviceEnrollmentToken == null ||
        deviceEnrollmentToken.isEmpty) {
      throw StateError('Login did not unlock the encrypted account key.');
    }
    final existingVault = await _deviceVaultStorage.read(
      cloudBaseUrl: state.cloudBaseUrl,
      username: username,
    );
    final existingDevice = existingVault != null &&
            existingVault.cloudBaseUrl == state.cloudBaseUrl &&
            existingVault.username == username
        ? existingVault.device
        : null;
    final device = existingDevice ?? await _rustBridge.generateDeviceSecrets();
    // Persist the private identity before the first server mutation. If any
    // later request fails, the next login reuses this exact device instead of
    // orphaning a registered public key whose private half was lost.
    await _deviceVaultStorage.write(
      MobileDeviceVault(
        cloudBaseUrl: state.cloudBaseUrl,
        username: username,
        accountMasterKey: List<int>.from(accountMasterKey),
        device: device,
      ),
    );
    final workingMasterKey = List<int>.from(accountMasterKey);
    late final ProvisionResult provisioned;
    try {
      provisioned = await _rustBridge.provisionDeviceAndSpaces(
        cloudBaseUrl: state.cloudBaseUrl,
        accessToken: accessToken,
        accountMasterKey: workingMasterKey,
        platform: ref.read(mobilePlatformProvider),
        deviceEnrollmentToken: deviceEnrollmentToken,
        existingDevice: device,
      );
    } finally {
      workingMasterKey.fillRange(0, workingMasterKey.length, 0);
    }
    if (provisioned.device.deviceId != device.deviceId) {
      throw StateError('Provisioning returned a different device identity.');
    }
    _systemProjectionService.configureAccount(
      cloudBaseUrl: state.cloudBaseUrl,
      username: username,
    );
    state = state.copyWith(
      isAuthenticated: true,
      accessToken: provisioned.accessToken,
      username: username,
      collections: provisioned.collections,
    );
    await _persistRuntimeRefreshToken();
  }

  /// Rehydrates the account key into the Rust process before operations that
  /// create recovery-wrapped space keys. Normal offline sync does not require
  /// this network round trip.
  Future<void> _ensureProvisioningSecrets() async {
    await _reconcileProvisioning();
  }

  /// Reconciles server membership and current key epochs before network work.
  ///
  /// Provisioning is idempotent for an existing device. It refreshes device
  /// packages, removes spaces that are no longer accessible, and updates the
  /// in-memory key registry before the next sync cycle.
  Future<void> _reconcileProvisioning() async {
    final accessToken = state.accessToken;
    final username = state.username;
    final vault = username == null
        ? null
        : await _deviceVaultStorage.read(
            cloudBaseUrl: state.cloudBaseUrl,
            username: username,
          );
    if (accessToken == null || accessToken.isEmpty || vault == null) {
      throw StateError('The secure mobile account vault is unavailable.');
    }
    final provisioned = await _rustBridge.provisionDeviceAndSpaces(
      cloudBaseUrl: state.cloudBaseUrl,
      accessToken: accessToken,
      accountMasterKey: vault.accountMasterKey,
      platform: ref.read(mobilePlatformProvider),
      deviceEnrollmentToken: null,
      existingDevice: vault.device,
    );
    final collectionsById = <String, CollectionEntry>{
      for (final collection in state.collections) collection.id: collection,
      for (final collection in provisioned.collections)
        collection.id: collection,
    };
    state = state.copyWith(
      accessToken: provisioned.accessToken,
      collections: provisioned.collections.isEmpty
          ? const <CollectionEntry>[]
          : collectionsById.values
              .where(
                (collection) => provisioned.collections.any(
                  (available) => available.id == collection.id,
                ),
              )
              .toList(growable: false),
    );
    await _configureSyncForActiveSession();
    await _persistRuntimeRefreshToken();
    await _persistSyncRuntime();
  }

  bool _canWriteSpace(String spaceId) {
    for (final collection in state.collections) {
      if (collection.id == spaceId) {
        return collection.role != 'reader';
      }
    }
    return false;
  }

  Future<void> _loadProjectionSettings() async {
    final username = state.username;
    final vault = username == null
        ? null
        : await _deviceVaultStorage.read(
            cloudBaseUrl: state.cloudBaseUrl,
            username: username,
          );
    if (vault == null) {
      return;
    }
    _systemProjectionService.configureAccount(
      cloudBaseUrl: vault.cloudBaseUrl,
      username: vault.username,
    );
    final settings = await _systemProjectionService.readSettings();
    state = state.copyWith(
      calendarProjectionCollectionIds: settings.calendarCollectionIds,
      contactsProjectionCollectionIds: settings.contactsCollectionIds,
    );
  }

  Future<void> _projectSystemCopies(List<PimItem> items) async {
    if (state.calendarProjectionCollectionIds.isEmpty &&
        state.contactsProjectionCollectionIds.isEmpty) {
      return;
    }
    try {
      await _systemProjectionService.projectEnabled(items);
    } catch (error) {
      state = state.copyWith(
        error: 'Encrypted sync succeeded, but system projection failed: $error',
      );
    }
  }

  /// Imports refresh token from secure storage into Rust runtime state.
  Future<void> _hydrateRefreshTokenIntoRuntime() async {
    final credential = await _refreshTokenStorage.readCredential(
      cloudBaseUrl: state.cloudBaseUrl,
    );
    if (credential == null) {
      await _rustBridge.clearRefreshToken();
      return;
    }
    await _rustBridge.importRefreshToken(
      refreshToken: credential.refreshToken,
      rotationRequestId: credential.rotationRequestId,
    );
  }

  /// Persists refresh token from Rust runtime into platform secure storage.
  Future<void> _persistRuntimeRefreshToken() async {
    final refreshToken = await _rustBridge.exportRefreshToken();
    if (refreshToken == null || refreshToken.isEmpty) {
      await _refreshTokenStorage.delete(cloudBaseUrl: state.cloudBaseUrl);
      return;
    }
    final rotationRequestId =
        await _rustBridge.exportRefreshRotationRequestId();
    if (rotationRequestId == null || rotationRequestId.isEmpty) {
      throw StateError(
          'Rust runtime did not expose a refresh rotation identity');
    }
    await _refreshTokenStorage.write(
      cloudBaseUrl: state.cloudBaseUrl,
      refreshToken: refreshToken,
      rotationRequestId: rotationRequestId,
    );
  }

  /// Revokes or durably queues the refresh session created by a login that
  /// failed before local provisioning completed.
  Future<String?> _rollbackIncompleteLogin() async {
    final refreshToken = await _rustBridge.exportRefreshToken();
    String? warning;
    if (refreshToken != null && refreshToken.trim().isNotEmpty) {
      try {
        await _rustBridge.revokeRefreshSession(
          cloudBaseUrl: state.cloudBaseUrl,
          refreshToken: refreshToken,
        );
        await _refreshTokenStorage.deleteQueuedRevocation();
      } catch (_) {
        try {
          await _refreshTokenStorage.queueRevocation(
            cloudBaseUrl: state.cloudBaseUrl,
            refreshToken: refreshToken,
          );
          warning = 'Server-session revocation was queued for the next launch.';
        } catch (_) {
          warning =
              'The incomplete server session could not be revoked or queued; revoke it from another signed-in device.';
        }
      }
    }
    try {
      await _rustBridge.clearRefreshToken();
      await _refreshTokenStorage.delete(cloudBaseUrl: state.cloudBaseUrl);
    } catch (_) {
      warning ??= 'Local refresh-token cleanup did not complete.';
    }
    return warning;
  }

  /// Clears refresh token from both Rust runtime and secure storage.
  Future<void> _clearRefreshTokenState() async {
    await _rustBridge.clearRefreshToken();
    await _refreshTokenStorage.delete(cloudBaseUrl: state.cloudBaseUrl);
  }

  Future<void> _persistSyncRuntime() async {
    final accessToken = state.accessToken;
    final username = state.username;
    if (!state.isAuthenticated ||
        accessToken == null ||
        accessToken.isEmpty ||
        username == null ||
        username.isEmpty) {
      return;
    }
    await _syncRuntimeStorage.write(
      MobileSyncRuntimeSnapshot(
        cloudBaseUrl: state.cloudBaseUrl,
        username: username,
        sqlitePath: state.sqlitePath,
        accessToken: accessToken,
        collections: state.collections,
        backgroundSyncEnabled: state.backgroundSyncEnabled,
      ),
    );
  }

  String? _validatedCloudBaseUrl(String value) {
    final raw = value.trim();
    final uri = Uri.tryParse(raw);
    if (uri == null ||
        !uri.hasScheme ||
        uri.host.isEmpty ||
        uri.userInfo.isNotEmpty ||
        uri.hasQuery ||
        uri.hasFragment ||
        (uri.path.isNotEmpty && uri.path != '/')) {
      return null;
    }
    final isHttps = uri.scheme == 'https';
    final isLoopback =
        uri.host == 'localhost' || uri.host == '127.0.0.1' || uri.host == '::1';
    if (!isHttps && !(kDebugMode && uri.scheme == 'http' && isLoopback)) {
      return null;
    }
    return uri.replace(path: '').toString().replaceAll(RegExp(r'/$'), '');
  }
}
