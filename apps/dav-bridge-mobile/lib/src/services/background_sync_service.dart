import 'package:flutter/widgets.dart';
import 'package:workmanager/workmanager.dart';

import 'package:dav_bridge_mobile/src/rust/rust_bridge_api.dart';
import 'package:dav_bridge_mobile/src/services/sync_runtime_storage.dart';
import 'package:dav_bridge_mobile/src/services/token_storage_service.dart';

const String kKamoriBackgroundSyncTask = 'kamori_background_sync_task';

@pragma('vm:entry-point')
void backgroundSyncCallbackDispatcher() {
  Workmanager().executeTask((task, inputData) async {
    WidgetsFlutterBinding.ensureInitialized();
    final bridge = RustBridgeFactory.create();
    try {
      final snapshot = await SecureSyncRuntimeStorage().read();
      if (snapshot == null) {
        return true;
      }
      final vault = snapshot.username == null
          ? await SecureDeviceVaultStorage().read()
          : await SecureDeviceVaultStorage().read(
              cloudBaseUrl: snapshot.cloudBaseUrl,
              username: snapshot.username,
            );
      if (vault == null ||
          vault.cloudBaseUrl != snapshot.cloudBaseUrl ||
          (snapshot.username != null && vault.username != snapshot.username)) {
        return false;
      }
      await bridge.configureSync(
        cloudBaseUrl: snapshot.cloudBaseUrl,
        sqlitePath: snapshot.sqlitePath,
        accessToken: snapshot.accessToken,
        sqliteKey: await SecureLocalCacheKeyStorage().readOrCreate(),
        device: vault.device,
      );
      final tokenStorage = SecureRefreshTokenStorage();
      final refreshCredential = await tokenStorage.readCredential(
        cloudBaseUrl: snapshot.cloudBaseUrl,
      );
      final refreshToken = refreshCredential?.refreshToken;
      if (refreshCredential != null) {
        await bridge.importRefreshToken(
          refreshToken: refreshCredential.refreshToken,
          rotationRequestId: refreshCredential.rotationRequestId,
        );
      }
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
      final rotatedRefreshToken = await bridge.exportRefreshToken();
      if (rotatedRefreshToken != null && rotatedRefreshToken.isNotEmpty) {
        final rotatedRequestId =
            await bridge.exportRefreshRotationRequestId();
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
      return true;
    } catch (error, stackTrace) {
      debugPrint('background sync failed: $error');
      debugPrintStack(stackTrace: stackTrace);
      return false;
    }
  });
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
  }

  /// Cancels periodic background sync task by its unique name.
  static Future<void> cancelPeriodicSync() async {
    await Workmanager().cancelByUniqueName(kKamoriBackgroundSyncTask);
  }
}
