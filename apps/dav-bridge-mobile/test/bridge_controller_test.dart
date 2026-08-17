import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:dav_bridge_mobile/src/models/bridge_models.dart';
import 'package:dav_bridge_mobile/src/rust/rust_bridge_api.dart';
import 'package:dav_bridge_mobile/src/services/sync_runtime_storage.dart';
import 'package:dav_bridge_mobile/src/services/system_projection_service.dart';
import 'package:dav_bridge_mobile/src/services/token_storage_service.dart';
import 'package:dav_bridge_mobile/src/state/bridge_controller.dart';

class _FakeRustBridgeApi implements RustBridgeApi {
  LoginResult passwordLoginResult = const LoginResult(
    username: 'alice',
    accessToken: 'session-1',
    preauthToken: null,
    totpVerified: true,
    accountMasterKey: <int>[
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
    ],
  );
  IssuedInviteCode issuedInviteCode =
      const IssuedInviteCode(code: 'ABCD-EFGH-JKLM-NPQR', ttlMinutes: 60);
  RedeemedInvite redeemedInvite = const RedeemedInvite(
    collectionId: 'collection-shared-1',
    role: 'editor',
    keyEpoch: 1,
    collectionKey: <int>[
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
    ],
  );

  int syncResult = 1;
  int registerCollectionKeyCalls = 0;
  int unregisterCollectionKeyCalls = 0;
  int moveCollectionToTrashCalls = 0;
  int configureCalls = 0;
  String lastAccessToken = '';
  String? _refreshToken = 'mock-refresh-token';

  @override
  Future<LoginResult> passwordLogin({
    required String cloudBaseUrl,
    required String username,
    required String password,
    String? totpCode,
  }) async {
    return passwordLoginResult;
  }

  @override
  Future<ProvisionResult> provisionDeviceAndSpaces({
    required String cloudBaseUrl,
    required String accessToken,
    required List<int> accountMasterKey,
    required String platform,
    DeviceSecrets? existingDevice,
  }) async {
    return ProvisionResult(
      accessToken: accessToken,
      device: existingDevice ??
          DeviceSecrets(
            deviceId: '00000000-0000-4000-8000-000000000001',
            signingPrivateKey: List<int>.filled(32, 2),
            hpkePrivateKey: List<int>.filled(32, 3),
            hpkePublicKey: List<int>.filled(32, 4),
          ),
      collections: const <CollectionEntry>[],
    );
  }

  @override
  Future<void> importRefreshToken({required String refreshToken}) async {
    _refreshToken = refreshToken;
  }

  @override
  Future<String?> exportRefreshToken() async {
    return _refreshToken;
  }

  @override
  Future<void> clearRefreshToken() async {
    _refreshToken = null;
  }

  @override
  Future<bool> revokeRefreshSession({
    required String cloudBaseUrl,
    required String refreshToken,
  }) async =>
      true;

  @override
  Future<void> configureSync({
    required String cloudBaseUrl,
    required String sqlitePath,
    required String accessToken,
    required List<int> sqliteKey,
    DeviceSecrets? device,
  }) async {
    if (sqliteKey.length != 32) {
      throw ArgumentError('invalid SQLite key');
    }
    configureCalls += 1;
    lastAccessToken = accessToken;
  }

  @override
  Future<int> syncNow() async => syncResult;

  @override
  Future<List<PimItem>> listPimItems() async => const <PimItem>[];

  @override
  Future<PimItem> upsertPimItem({
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
    return PimItem(
      spaceId: spaceId,
      resourceId: resourceId ?? '00000000-0000-4000-8000-000000000020',
      kind: kind,
      title: title,
      completed: completed,
      email: email,
      phone: phone,
      startsAt: startsAt,
      endsAt: endsAt,
    );
  }

  @override
  Future<void> deletePimItem({required PimItem item}) async {}

  @override
  Future<CollectionEntry> createCollection({required String name}) async {
    return CollectionEntry(
      id: '00000000-0000-4000-8000-000000000010',
      name: name,
      cmk: List<int>.filled(32, 5),
    );
  }

  @override
  Future<void> moveCollectionToTrash({required String collectionId}) async {
    moveCollectionToTrashCalls += 1;
  }

  @override
  Future<void> registerCollectionKey({
    required String collectionId,
    required int keyEpoch,
    required List<int> cmk,
  }) async {
    registerCollectionKeyCalls += 1;
  }

  @override
  Future<void> unregisterCollectionKey({required String collectionId}) async {
    unregisterCollectionKeyCalls += 1;
  }

  @override
  Future<IssuedInviteCode> createInviteCode({
    required String collectionId,
    required List<int> collectionKey,
    required int ttlMinutes,
  }) async {
    return issuedInviteCode;
  }

  @override
  Future<RedeemedInvite> redeemInviteCode({required String inviteCode}) async {
    return redeemedInvite;
  }
}

class _FakeDeviceVaultStorage implements DeviceVaultStorage {
  MobileDeviceVault? vault;

  @override
  Future<MobileDeviceVault?> read() async => vault;

  @override
  Future<void> write(MobileDeviceVault value) async => vault = value;
}

class _FakeSyncRuntimeStorage implements SyncRuntimeStorage {
  MobileSyncRuntimeSnapshot? snapshot;

  @override
  Future<void> delete() async => snapshot = null;

  @override
  Future<MobileSyncRuntimeSnapshot?> read() async => snapshot;

  @override
  Future<void> write(MobileSyncRuntimeSnapshot value) async {
    snapshot = value;
  }
}

class _FakeLocalCacheKeyStorage implements LocalCacheKeyStorage {
  @override
  Future<List<int>> readOrCreate() async => List<int>.filled(32, 7);
}

class _FakeRefreshTokenStorage implements RefreshTokenStorage {
  final Map<String, String> _tokens = <String, String>{};
  PendingRefreshRevocation? _pending;

  @override
  Future<void> write({
    required String cloudBaseUrl,
    required String refreshToken,
  }) async {
    _tokens[cloudBaseUrl] = refreshToken;
  }

  @override
  Future<String?> read({required String cloudBaseUrl}) async {
    return _tokens[cloudBaseUrl];
  }

  @override
  Future<void> delete({required String cloudBaseUrl}) async {
    _tokens.remove(cloudBaseUrl);
  }

  @override
  Future<void> queueRevocation({
    required String cloudBaseUrl,
    required String refreshToken,
  }) async {
    _pending = PendingRefreshRevocation(cloudBaseUrl, refreshToken);
  }

  @override
  Future<PendingRefreshRevocation?> readQueuedRevocation() async => _pending;

  @override
  Future<void> deleteQueuedRevocation() async {
    _pending = null;
  }
}

class _FakeSystemProjectionService implements SystemProjectionService {
  bool calendarEnabled = false;
  bool contactsEnabled = false;
  int projectCalls = 0;
  bool? lastCalendarRemoveChoice;
  bool? lastContactsRemoveChoice;

  @override
  void configureAccount({
    required String cloudBaseUrl,
    required String username,
  }) {}

  @override
  Future<SystemProjectionSettings> readSettings() async {
    return SystemProjectionSettings(
      calendarEnabled: calendarEnabled,
      contactsEnabled: contactsEnabled,
    );
  }

  @override
  Future<void> enableCalendar(List<PimItem> items) async {
    calendarEnabled = true;
    projectCalls += 1;
  }

  @override
  Future<void> enableContacts(List<PimItem> items) async {
    contactsEnabled = true;
    projectCalls += 1;
  }

  @override
  Future<void> disableCalendar({required bool removeProjectedData}) async {
    calendarEnabled = false;
    lastCalendarRemoveChoice = removeProjectedData;
  }

  @override
  Future<void> disableContacts({required bool removeProjectedData}) async {
    contactsEnabled = false;
    lastContactsRemoveChoice = removeProjectedData;
  }

  @override
  Future<void> projectEnabled(List<PimItem> items) async {
    projectCalls += 1;
  }
}

void main() {
  ProviderContainer createContainer({
    required _FakeRustBridgeApi bridge,
    _FakeRefreshTokenStorage? refreshTokenStorage,
    Future<void> Function()? schedulePeriodicSync,
    Future<void> Function()? cancelPeriodicSync,
    DateTime Function()? now,
    _FakeSystemProjectionService? systemProjectionService,
  }) {
    return ProviderContainer(
      overrides: [
        rustBridgeProvider.overrideWith((ref) => bridge),
        refreshTokenStorageProvider.overrideWith(
          (ref) => refreshTokenStorage ?? _FakeRefreshTokenStorage(),
        ),
        syncRuntimeStorageProvider.overrideWith(
          (ref) => _FakeSyncRuntimeStorage(),
        ),
        localCacheKeyStorageProvider.overrideWith(
          (ref) => _FakeLocalCacheKeyStorage(),
        ),
        deviceVaultStorageProvider.overrideWith(
          (ref) => _FakeDeviceVaultStorage(),
        ),
        systemProjectionServiceProvider.overrideWith(
          (ref) => systemProjectionService ?? _FakeSystemProjectionService(),
        ),
        mobilePlatformProvider.overrideWith((ref) => 'android'),
        schedulePeriodicSyncProvider.overrideWith(
          (ref) => schedulePeriodicSync ?? (() async {}),
        ),
        cancelPeriodicSyncProvider.overrideWith(
          (ref) => cancelPeriodicSync ?? (() async {}),
        ),
        nowProvider.overrideWith((ref) => now ?? DateTime.now),
      ],
    );
  }

  group('BridgeController', () {
    test('settings updates trim values', () async {
      final bridge = _FakeRustBridgeApi();
      final container = createContainer(bridge: bridge);
      addTearDown(container.dispose);
      final controller = container.read(bridgeControllerProvider.notifier);

      await controller.updateCloudBaseUrl('  http://localhost:4000  ');

      final state = container.read(bridgeControllerProvider);
      expect(state.cloudBaseUrl, 'http://localhost:4000');
    });

    test('password login authenticates and configures sync', () async {
      final bridge = _FakeRustBridgeApi();
      var scheduleCalls = 0;
      final container = createContainer(
        bridge: bridge,
        schedulePeriodicSync: () async {
          scheduleCalls += 1;
        },
      );
      addTearDown(container.dispose);
      final controller = container.read(bridgeControllerProvider.notifier);

      await controller.loginWithPassword(username: 'alice', password: 'secret');

      final state = container.read(bridgeControllerProvider);
      expect(state.isAuthenticated, isTrue);
      expect(bridge.configureCalls, 1);
      expect(bridge.lastAccessToken, 'session-1');
      expect(scheduleCalls, 1);
    });

    test('password login surfaces totp required message', () async {
      final bridge = _FakeRustBridgeApi()
        ..passwordLoginResult = const LoginResult(
          accessToken: null,
          preauthToken: 'preauth',
          totpVerified: false,
        );
      final container = createContainer(bridge: bridge);
      addTearDown(container.dispose);
      final controller = container.read(bridgeControllerProvider.notifier);

      await controller.loginWithPassword(username: 'alice', password: 'secret');

      final state = container.read(bridgeControllerProvider);
      expect(state.isAuthenticated, isFalse);
      expect(state.error, contains('TOTP is required'));
    });

    test('syncNow updates synced total and timestamp', () async {
      final bridge = _FakeRustBridgeApi()..syncResult = 3;
      final now = DateTime.utc(2026, 3, 3, 12, 0, 0);
      final container = createContainer(bridge: bridge, now: () => now);
      addTearDown(container.dispose);
      final controller = container.read(bridgeControllerProvider.notifier);

      await controller.syncNow();

      final state = container.read(bridgeControllerProvider);
      expect(state.syncedItemsTotal, 3);
      expect(state.lastSyncAt, now);
    });

    test('createCollection uses server-backed bridge result', () async {
      final bridge = _FakeRustBridgeApi();
      final container = createContainer(bridge: bridge);
      addTearDown(container.dispose);
      final controller = container.read(bridgeControllerProvider.notifier);

      await controller.loginWithPassword(username: 'alice', password: 'secret');
      await controller.createCollection('Personal');

      final state = container.read(bridgeControllerProvider);
      expect(state.collections, hasLength(1));
      expect(state.collections.first.cmk, hasLength(32));
      expect(bridge.registerCollectionKeyCalls, 0);
    });

    test('deleteCollection moves server space to trash before local removal',
        () async {
      final bridge = _FakeRustBridgeApi();
      final container = createContainer(bridge: bridge);
      addTearDown(container.dispose);
      final controller = container.read(bridgeControllerProvider.notifier);

      await controller.loginWithPassword(username: 'alice', password: 'secret');
      await controller.createCollection('Personal');
      final collectionId =
          container.read(bridgeControllerProvider).collections.first.id;

      await controller.deleteCollection(collectionId);

      final state = container.read(bridgeControllerProvider);
      expect(state.collections, isEmpty);
      expect(bridge.moveCollectionToTrashCalls, 1);
      expect(bridge.unregisterCollectionKeyCalls, 0);
    });

    test('organizer items can be created and deleted through signed bridge API',
        () async {
      final bridge = _FakeRustBridgeApi();
      final container = createContainer(bridge: bridge);
      addTearDown(container.dispose);
      final controller = container.read(bridgeControllerProvider.notifier);

      await controller.loginWithPassword(username: 'alice', password: 'secret');
      await controller.createCollection('Personal');
      final spaceId =
          container.read(bridgeControllerProvider).collections.first.id;
      await controller.savePimItem(
        spaceId: spaceId,
        kind: PimItemKind.task,
        title: 'Ship MVP',
      );

      var state = container.read(bridgeControllerProvider);
      expect(state.pimItems, hasLength(1));
      expect(state.pimItems.single.title, 'Ship MVP');

      await controller.deletePimItem(state.pimItems.single);
      state = container.read(bridgeControllerProvider);
      expect(state.pimItems, isEmpty);
    });

    test('system projection is explicit and preserves the removal choice',
        () async {
      final bridge = _FakeRustBridgeApi();
      final projection = _FakeSystemProjectionService();
      final container = createContainer(
        bridge: bridge,
        systemProjectionService: projection,
      );
      addTearDown(container.dispose);
      final controller = container.read(bridgeControllerProvider.notifier);

      await controller.loginWithPassword(username: 'alice', password: 'secret');
      expect(container.read(bridgeControllerProvider).calendarProjectionEnabled,
          isFalse);

      await controller.setCalendarProjectionEnabled(true);
      expect(container.read(bridgeControllerProvider).calendarProjectionEnabled,
          isTrue);

      await controller.setCalendarProjectionEnabled(
        false,
        removeProjectedData: true,
      );
      expect(container.read(bridgeControllerProvider).calendarProjectionEnabled,
          isFalse);
      expect(projection.lastCalendarRemoveChoice, isTrue);
    });

    test('reader spaces reject organizer writes before calling the bridge',
        () async {
      final bridge = _FakeRustBridgeApi()
        ..redeemedInvite = RedeemedInvite(
          collectionId: 'collection-shared-1',
          role: 'reader',
          keyEpoch: 1,
          collectionKey: List<int>.filled(32, 1),
        );
      final container = createContainer(bridge: bridge);
      addTearDown(container.dispose);
      final controller = container.read(bridgeControllerProvider.notifier);

      await controller.loginWithPassword(username: 'alice', password: 'secret');
      await controller.redeemInviteCode('ABCD-EFGH-JKLM-NPQR');
      final state = container.read(bridgeControllerProvider);
      await controller.savePimItem(
        spaceId: state.collections.single.id,
        kind: PimItemKind.task,
        title: 'Should not save',
      );

      final updated = container.read(bridgeControllerProvider);
      expect(updated.pimItems, isEmpty);
      expect(updated.error, contains('read-only'));
    });

    test('lifecycle resume reconfigures sync without starting a server',
        () async {
      final bridge = _FakeRustBridgeApi();
      final container = createContainer(bridge: bridge);
      addTearDown(container.dispose);
      final controller = container.read(bridgeControllerProvider.notifier);

      await controller.loginWithPassword(username: 'alice', password: 'secret');
      await controller.handleLifecycleState(AppLifecycleState.paused);
      await controller.handleLifecycleState(AppLifecycleState.resumed);

      expect(bridge.configureCalls, 2);
    });

    test('background sync toggle uses provided callbacks', () async {
      final bridge = _FakeRustBridgeApi();
      var scheduleCalls = 0;
      var cancelCalls = 0;
      final container = createContainer(
        bridge: bridge,
        schedulePeriodicSync: () async {
          scheduleCalls += 1;
        },
        cancelPeriodicSync: () async {
          cancelCalls += 1;
        },
      );
      addTearDown(container.dispose);
      final controller = container.read(bridgeControllerProvider.notifier);

      await controller.loginWithPassword(username: 'alice', password: 'secret');
      await controller.setBackgroundSyncEnabled(false);
      await controller.setBackgroundSyncEnabled(true);

      expect(scheduleCalls, 2);
      expect(cancelCalls, 1);
      final state = container.read(bridgeControllerProvider);
      expect(state.backgroundSyncEnabled, isTrue);
    });

    test('logout clears session and cancels periodic sync', () async {
      final bridge = _FakeRustBridgeApi();
      var cancelCalls = 0;
      final container = createContainer(
        bridge: bridge,
        cancelPeriodicSync: () async {
          cancelCalls += 1;
        },
      );
      addTearDown(container.dispose);
      final controller = container.read(bridgeControllerProvider.notifier);

      await controller.loginWithPassword(username: 'alice', password: 'secret');
      await controller.createCollection('Personal');
      await controller.logout();

      final state = container.read(bridgeControllerProvider);
      expect(cancelCalls, 1);
      expect(state.isAuthenticated, isFalse);
      expect(state.accessToken, isNull);
      expect(state.collections, isEmpty);
    });

    test('createInviteCode returns code', () async {
      final bridge = _FakeRustBridgeApi();
      final container = createContainer(bridge: bridge);
      addTearDown(container.dispose);
      final controller = container.read(bridgeControllerProvider.notifier);

      await controller.loginWithPassword(username: 'alice', password: 'secret');
      await controller.createCollection('Personal');
      final collectionId =
          container.read(bridgeControllerProvider).collections.first.id;

      final issued = await controller.createInviteCode(
        collectionId: collectionId,
        ttlMinutes: 30,
      );

      expect(issued, isNotNull);
      expect(issued!.code, 'ABCD-EFGH-JKLM-NPQR');
      expect(issued.ttlMinutes, 60);
    });

    test('redeemInviteCode adds shared collection', () async {
      final bridge = _FakeRustBridgeApi();
      final container = createContainer(bridge: bridge);
      addTearDown(container.dispose);
      final controller = container.read(bridgeControllerProvider.notifier);

      await controller.loginWithPassword(username: 'alice', password: 'secret');
      final redeemed = await controller.redeemInviteCode('ABCD-EFGH-JKLM-NPQR');

      expect(redeemed, isNotNull);
      final state = container.read(bridgeControllerProvider);
      expect(state.collections, hasLength(1));
      expect(state.collections.first.id, 'collection-shared-1');
      expect(state.collections.first.cmk, hasLength(32));
    });
  });
}
