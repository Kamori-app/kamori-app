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
      state = state.copyWith(
        isAuthenticated: true,
        accessToken: snapshot.accessToken,
        cloudBaseUrl: snapshot.cloudBaseUrl,
        sqlitePath: snapshot.sqlitePath,
        collections: snapshot.collections,
      );
      await _hydrateRefreshTokenIntoRuntime();
      await _configureSyncForActiveSession();
      await _reloadPimItems();
      await _loadProjectionSettings();
      await _projectSystemCopies(state.pimItems);
      if (state.backgroundSyncEnabled) {
        await _schedulePeriodicSync();
      }
    } catch (error) {
      state = state.copyWith(error: 'Failed to restore secure session: $error');
    }
  }

  /// Updates cloud backend base URL for next bridge start.
  Future<void> updateCloudBaseUrl(String value) async {
    state = state.copyWith(cloudBaseUrl: value.trim(), clearError: true);
    if (state.isAuthenticated) {
      await _configureSyncForActiveSession();
      await _persistSyncRuntime();
    }
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

    state = state.copyWith(isBusy: true, clearError: true);
    try {
      final login = await _rustBridge.passwordLogin(
        cloudBaseUrl: state.cloudBaseUrl,
        username: username.trim(),
        password: password,
        totpCode: totpCode?.trim().isEmpty == true ? null : totpCode?.trim(),
      );

      if (login.accessToken == null) {
        state = state.copyWith(
          isBusy: false,
          error: login.preauthToken != null
              ? 'TOTP is required. Enter your code and try again.'
              : 'Password login failed.',
        );
        return;
      }

      await _persistRuntimeRefreshToken();
      await _completeAuthenticatedLogin(login);
      await _configureSyncForActiveSession();
      await _loadProjectionSettings();
      await _schedulePeriodicSync();
      await _persistSyncRuntime();
      state = state.copyWith(isBusy: false);
    } catch (error) {
      state = state.copyWith(isBusy: false, error: 'Login failed: $error');
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
        collections: const <CollectionEntry>[],
        pimItems: const <PimItem>[],
        syncedItemsTotal: 0,
        clearLastSyncAt: true,
        calendarProjectionEnabled: false,
        contactsProjectionEnabled: false,
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

  Future<void> savePimItem({
    required String spaceId,
    String? resourceId,
    required PimItemKind kind,
    required String title,
    bool completed = false,
    String? email,
    String? phone,
    String? startsAt,
    String? endsAt,
  }) async {
    if (!_canWriteSpace(spaceId)) {
      state = state.copyWith(
        error: 'This space is read-only. Ask an owner for editor access.',
      );
      return;
    }
    state = state.copyWith(isBusy: true, clearError: true);
    try {
      await _hydrateRefreshTokenIntoRuntime();
      final item = await _rustBridge.upsertPimItem(
        spaceId: spaceId,
        resourceId: resourceId,
        kind: kind,
        title: title,
        completed: completed,
        email: email,
        phone: phone,
        startsAt: startsAt,
        endsAt: endsAt,
      );
      await _persistRuntimeRefreshToken();
      final items = [...state.pimItems]
        ..removeWhere(
          (existing) =>
              existing.spaceId == item.spaceId &&
              existing.resourceId == item.resourceId,
        )
        ..add(item);
      items.sort((left, right) => left.title.compareTo(right.title));
      state = state.copyWith(isBusy: false, pimItems: items);
      await _projectSystemCopies(items);
    } catch (error) {
      state =
          state.copyWith(isBusy: false, error: 'Failed to save item: $error');
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
                  existing.resourceId != item.resourceId,
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
      state = state.copyWith(isBusy: false, collections: updated);
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
      await _persistRuntimeRefreshToken();
      state = state.copyWith(isBusy: false);
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
          role: redeemed.role,
        );
      } else {
        current.add(
          CollectionEntry(
            id: redeemed.collectionId,
            name: 'Shared ${redeemed.collectionId.substring(0, 8)}',
            cmk: redeemed.collectionKey,
            keyEpoch: redeemed.keyEpoch,
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
    bool enabled, {
    bool removeProjectedData = false,
  }) async {
    state = state.copyWith(isBusy: true, clearError: true);
    try {
      if (enabled) {
        await _systemProjectionService.enableCalendar(state.pimItems);
      } else {
        await _systemProjectionService.disableCalendar(
          removeProjectedData: removeProjectedData,
        );
      }
      state = state.copyWith(
        isBusy: false,
        calendarProjectionEnabled: enabled,
      );
    } catch (error) {
      state = state.copyWith(
        isBusy: false,
        error: 'Failed to update Calendar integration: $error',
      );
    }
  }

  Future<void> setContactsProjectionEnabled(
    bool enabled, {
    bool removeProjectedData = false,
  }) async {
    state = state.copyWith(isBusy: true, clearError: true);
    try {
      if (enabled) {
        await _systemProjectionService.enableContacts(state.pimItems);
      } else {
        await _systemProjectionService.disableContacts(
          removeProjectedData: removeProjectedData,
        );
      }
      state = state.copyWith(
        isBusy: false,
        contactsProjectionEnabled: enabled,
      );
    } catch (error) {
      state = state.copyWith(
        isBusy: false,
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
      await _configureSyncForActiveSession();
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

    final vault = await _deviceVaultStorage.read();
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
        cmk: collection.cmk,
      );
    }

    await _persistRuntimeRefreshToken();
  }

  Future<void> _completeAuthenticatedLogin(LoginResult login) async {
    final accessToken = login.accessToken;
    final username = login.username;
    final accountMasterKey = login.accountMasterKey;
    if (accessToken == null ||
        username == null ||
        username.isEmpty ||
        accountMasterKey == null ||
        accountMasterKey.length != 32) {
      throw StateError('Login did not unlock the encrypted account key.');
    }
    final existingVault = await _deviceVaultStorage.read();
    final existingDevice = existingVault != null &&
            existingVault.cloudBaseUrl == state.cloudBaseUrl &&
            existingVault.username == username
        ? existingVault.device
        : null;
    final provisioned = await _rustBridge.provisionDeviceAndSpaces(
      cloudBaseUrl: state.cloudBaseUrl,
      accessToken: accessToken,
      accountMasterKey: accountMasterKey,
      platform: ref.read(mobilePlatformProvider),
      existingDevice: existingDevice,
    );
    await _deviceVaultStorage.write(
      MobileDeviceVault(
        cloudBaseUrl: state.cloudBaseUrl,
        username: username,
        accountMasterKey: accountMasterKey,
        device: provisioned.device,
      ),
    );
    _systemProjectionService.configureAccount(
      cloudBaseUrl: state.cloudBaseUrl,
      username: username,
    );
    state = state.copyWith(
      isAuthenticated: true,
      accessToken: provisioned.accessToken,
      collections: provisioned.collections,
    );
    await _persistRuntimeRefreshToken();
  }

  /// Rehydrates the account key into the Rust process before operations that
  /// create recovery-wrapped space keys. Normal offline sync does not require
  /// this network round trip.
  Future<void> _ensureProvisioningSecrets() async {
    final accessToken = state.accessToken;
    final vault = await _deviceVaultStorage.read();
    if (accessToken == null || accessToken.isEmpty || vault == null) {
      throw StateError('The secure mobile account vault is unavailable.');
    }
    final provisioned = await _rustBridge.provisionDeviceAndSpaces(
      cloudBaseUrl: state.cloudBaseUrl,
      accessToken: accessToken,
      accountMasterKey: vault.accountMasterKey,
      platform: ref.read(mobilePlatformProvider),
      existingDevice: vault.device,
    );
    final collectionsById = <String, CollectionEntry>{
      for (final collection in state.collections) collection.id: collection,
      for (final collection in provisioned.collections)
        collection.id: collection,
    };
    state = state.copyWith(
      accessToken: provisioned.accessToken,
      collections: collectionsById.values.toList(growable: false),
    );
    await _persistRuntimeRefreshToken();
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
    final vault = await _deviceVaultStorage.read();
    if (vault == null) {
      return;
    }
    _systemProjectionService.configureAccount(
      cloudBaseUrl: vault.cloudBaseUrl,
      username: vault.username,
    );
    final settings = await _systemProjectionService.readSettings();
    state = state.copyWith(
      calendarProjectionEnabled: settings.calendarEnabled,
      contactsProjectionEnabled: settings.contactsEnabled,
    );
  }

  Future<void> _projectSystemCopies(List<PimItem> items) async {
    if (!state.calendarProjectionEnabled && !state.contactsProjectionEnabled) {
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
    final refreshToken = await _refreshTokenStorage.read(
      cloudBaseUrl: state.cloudBaseUrl,
    );
    if (refreshToken == null) {
      await _rustBridge.clearRefreshToken();
      return;
    }
    await _rustBridge.importRefreshToken(refreshToken: refreshToken);
  }

  /// Persists refresh token from Rust runtime into platform secure storage.
  Future<void> _persistRuntimeRefreshToken() async {
    final refreshToken = await _rustBridge.exportRefreshToken();
    if (refreshToken == null || refreshToken.isEmpty) {
      await _refreshTokenStorage.delete(cloudBaseUrl: state.cloudBaseUrl);
      return;
    }
    await _refreshTokenStorage.write(
      cloudBaseUrl: state.cloudBaseUrl,
      refreshToken: refreshToken,
    );
  }

  /// Clears refresh token from both Rust runtime and secure storage.
  Future<void> _clearRefreshTokenState() async {
    await _rustBridge.clearRefreshToken();
    await _refreshTokenStorage.delete(cloudBaseUrl: state.cloudBaseUrl);
  }

  Future<void> _persistSyncRuntime() async {
    final accessToken = state.accessToken;
    if (!state.isAuthenticated || accessToken == null || accessToken.isEmpty) {
      return;
    }
    await _syncRuntimeStorage.write(
      MobileSyncRuntimeSnapshot(
        cloudBaseUrl: state.cloudBaseUrl,
        sqlitePath: state.sqlitePath,
        accessToken: accessToken,
        collections: state.collections,
      ),
    );
  }
}
