import 'package:flutter/widgets.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:workmanager/workmanager.dart';

import 'package:dav_bridge_mobile/src/rust/rust_bridge_api.dart';
import 'package:dav_bridge_mobile/src/services/sync_runtime_storage.dart';
import 'package:dav_bridge_mobile/src/services/token_storage_service.dart';
import 'package:dav_bridge_mobile/src/services/system_projection_service.dart';

const String kKamoriBackgroundSyncTask = 'kamori_background_sync_task';

@pragma('vm:entry-point')
void backgroundSyncCallbackDispatcher() {
  Workmanager().executeTask((task, inputData) async {
    WidgetsFlutterBinding.ensureInitialized();
    return runBackgroundSync(
      bridge: RustBridgeFactory.create(),
      runtimeStorage: SecureSyncRuntimeStorage(),
      vaultStorage: SecureDeviceVaultStorage(),
      cacheKeyStorage: SecureLocalCacheKeyStorage(),
      tokenStorage: SecureRefreshTokenStorage(),
      projection: NativeSystemProjectionService(),
    );
  });
}

/// Shared production orchestration; platform and persistence boundaries are
/// injected so background work can be exercised without touching OS stores.
Future<bool> runBackgroundSync({
  required RustBridgeApi bridge,
  required SyncRuntimeStorage runtimeStorage,
  required DeviceVaultStorage vaultStorage,
  required LocalCacheKeyStorage cacheKeyStorage,
  required RefreshTokenStorage tokenStorage,
  required SystemProjectionService projection,
}) async {
  SessionLease? lease;
  try {
    // The Rust runtime is process-global. Retain the lease through native
    // network work, secure-store commit and OS projection, not just the check.
    lease = await bridge.acquireSessionLease();
    final snapshot = await runtimeStorage.read();
    if (snapshot == null || !snapshot.backgroundSyncEnabled) {
      return true;
    }
    final vault = snapshot.username == null
        ? await vaultStorage.read()
        : await vaultStorage.read(
            cloudBaseUrl: snapshot.cloudBaseUrl,
            username: snapshot.username,
          );
    if (vault == null ||
        vault.cloudBaseUrl != snapshot.cloudBaseUrl ||
        (snapshot.username != null && vault.username != snapshot.username)) {
      return false;
    }
    final refreshCredential = await tokenStorage.readCredential(
      cloudBaseUrl: snapshot.cloudBaseUrl,
    );
    if (refreshCredential == null) {
      return false;
    }
    final refreshToken = refreshCredential.refreshToken;
    await bridge.importRefreshToken(
      refreshToken: refreshCredential.refreshToken,
      rotationRequestId: refreshCredential.rotationRequestId,
    );
    await bridge.configureSync(
      cloudBaseUrl: snapshot.cloudBaseUrl,
      sqlitePath: snapshot.sqlitePath,
      accessToken: snapshot.accessToken,
      sqliteKey: await cacheKeyStorage.readOrCreate(),
      device: vault.device,
      username: vault.username,
    );
    for (final collection in snapshot.collections) {
      await bridge.registerCollectionKey(
        collectionId: collection.id,
        keyEpoch: collection.keyEpoch,
        syncStartSeq:
            collection.historyStartSeq > collection.currentStateStartSeq
                ? collection.historyStartSeq
                : collection.currentStateStartSeq,
        cmk: collection.cmk,
      );
    }
    await bridge.syncNow();
    final currentSnapshot = await runtimeStorage.read();
    if (currentSnapshot == null ||
        currentSnapshot.cloudBaseUrl != snapshot.cloudBaseUrl ||
        currentSnapshot.username != snapshot.username ||
        currentSnapshot.accessToken != snapshot.accessToken) {
      // Sign-out/account switch while the network operation was in flight.
      return true;
    }
    final rotatedRefreshToken = await bridge.exportRefreshToken();
    if (rotatedRefreshToken != null && rotatedRefreshToken.isNotEmpty) {
      final rotatedRequestId = await bridge.exportRefreshRotationRequestId();
      if (rotatedRequestId == null || rotatedRequestId.isEmpty) {
        return false;
      }
      final currentCredential = await tokenStorage.readCredential(
        cloudBaseUrl: snapshot.cloudBaseUrl,
      );
      // Never let an older background isolate overwrite a token already
      // advanced by the foreground client. Server refresh retries are exact,
      // so equal replacements remain safe.
      if (currentCredential?.refreshToken == refreshToken ||
          currentCredential?.refreshToken == rotatedRefreshToken) {
        await tokenStorage.write(
          cloudBaseUrl: snapshot.cloudBaseUrl,
          refreshToken: rotatedRefreshToken,
          rotationRequestId: rotatedRequestId,
        );
      }
    }
    try {
      if (currentSnapshot.backgroundSyncEnabled) {
        final items = await bridge.listPimItems();
        projection.configureAccount(
          cloudBaseUrl: snapshot.cloudBaseUrl,
          username: vault.username,
        );
        // projectEnabled reads persisted per-collection opt-ins now and never
        // requests permissions. Provider failures do not undo encrypted sync
        // or prevent the rotated refresh credential from being persisted.
        await projection.projectEnabled(items);
      }
    } catch (error, stackTrace) {
      debugPrint('background projection will retry: $error');
      debugPrintStack(stackTrace: stackTrace);
    }
    return true;
  } catch (error, stackTrace) {
    debugPrint('background sync failed: $error');
    debugPrintStack(stackTrace: stackTrace);
    return false;
  } finally {
    lease?.release();
  }
}

class BackgroundSyncService {
  /// Initializes Workmanager and registers background callback entrypoint.
  static Future<void> initialize() async {
    await Workmanager().initialize(backgroundSyncCallbackDispatcher);
  }

  /// Schedules periodic background sync with conservative battery constraints.
  static Future<void> schedulePeriodicSync({
    Duration frequency = const Duration(hours: 4),
  }) async {
    await Workmanager().registerPeriodicTask(
      kKamoriBackgroundSyncTask,
      kKamoriBackgroundSyncTask,
      frequency: frequency,
      constraints: Constraints(
        networkType: NetworkType.connected,
        requiresBatteryNotLow: true,
      ),
      existingWorkPolicy: ExistingPeriodicWorkPolicy.update,
      initialDelay: const Duration(minutes: 10),
      backoffPolicy: BackoffPolicy.exponential,
      backoffPolicyDelay: const Duration(minutes: 5),
    );
    if (defaultTargetPlatform == TargetPlatform.iOS) {
      // The pinned Apple plugin logs submission failures instead of throwing.
      // A pending request confirms admission, not eventual delivery by iOS.
      final admitted = await const MethodChannel('kamori/background-sync')
          .invokeMethod<bool>('isScheduled');
      if (admitted != true) {
        throw StateError('iOS did not accept the background sync request.');
      }
    }
  }

  /// Cancels periodic background sync task by its unique name.
  static Future<void> cancelPeriodicSync() async {
    await Workmanager().cancelByUniqueName(kKamoriBackgroundSyncTask);
  }
}
