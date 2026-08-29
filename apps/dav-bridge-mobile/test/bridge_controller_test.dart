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
    totpContinuationToken: null,
    deviceEnrollmentToken: 'device-enrollment',
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
  IssuedInviteCode issuedInviteCode = const IssuedInviteCode(
    code: 'ABCD-EFGH-JKLM-NPQR',
    ttlMinutes: 60,
    keyEpoch: 2,
    currentStateStartSeq: 4,
    collectionKey: <int>[
      2,
      2,
      2,
      2,
      2,
      2,
      2,
      2,
      2,
      2,
      2,
      2,
      2,
      2,
      2,
      2,
      2,
      2,
      2,
      2,
      2,
      2,
      2,
      2,
      2,
      2,
      2,
      2,
      2,
      2,
      2,
      2,
    ],
  );
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

  @override
  Future<DeviceSecrets> generateDeviceSecrets() async => DeviceSecrets(
        deviceId: '00000000-0000-4000-8000-000000000001',
        signingPrivateKey: List<int>.filled(32, 2),
        hpkePrivateKey: List<int>.filled(32, 3),
        hpkePublicKey: List<int>.filled(32, 4),
      );

  int syncResult = 1;
  int registerCollectionKeyCalls = 0;
  int unregisterCollectionKeyCalls = 0;
  int moveCollectionToTrashCalls = 0;
  int configureCalls = 0;
  int provisionCalls = 0;
  int revokeCalls = 0;
  int createCollectionCalls = 0;
  bool failProvisioning = false;
  Future<void> Function()? beforeProvision;
  String lastAccessToken = '';
  String? _refreshToken = 'mock-refresh-token';
  String? _refreshRotationRequestId = '00000000-0000-4000-8000-000000000099';
  List<CollectionEntry> provisionedCollections = const <CollectionEntry>[];

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
    String? deviceEnrollmentToken,
    DeviceSecrets? existingDevice,
  }) async {
    provisionCalls += 1;
    await beforeProvision?.call();
    if (failProvisioning) {
      throw StateError('simulated provisioning failure');
    }
    return ProvisionResult(
      accessToken: accessToken,
      device: existingDevice ??
          DeviceSecrets(
            deviceId: '00000000-0000-4000-8000-000000000001',
            signingPrivateKey: List<int>.filled(32, 2),
            hpkePrivateKey: List<int>.filled(32, 3),
            hpkePublicKey: List<int>.filled(32, 4),
          ),
      collections: provisionedCollections,
    );
  }

  @override
  Future<void> importRefreshToken({
    required String refreshToken,
    required String rotationRequestId,
  }) async {
    _refreshToken = refreshToken;
    _refreshRotationRequestId = rotationRequestId;
  }

  @override
  Future<String?> exportRefreshToken() async {
    return _refreshToken;
  }

  @override
  Future<String?> exportRefreshRotationRequestId() async {
    return _refreshRotationRequestId;
  }

  @override
  Future<void> clearRefreshToken() async {
    _refreshToken = null;
    _refreshRotationRequestId = null;
  }

  @override
  Future<bool> revokeRefreshSession({
    required String cloudBaseUrl,
    required String refreshToken,
  }) async {
    revokeCalls += 1;
    return true;
  }

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
  Future<PimItem> upsertPimItem({required PimDraft draft}) async {
    final logicalId =
        draft.resourceId ?? '00000000-0000-4000-8000-000000000020';
    return PimItem(
      spaceId: draft.spaceId,
      resourceId: logicalId,
      projectionId: draft.projectionId ?? '$logicalId.ics',
      headOperationId: '00000000-0000-4000-8000-000000000021',
      kind: draft.kind,
      title: draft.title,
      completed: draft.completed,
      completedAt: draft.completedAt,
      notes: draft.notes,
      startsAt: draft.startsAt,
      endsAt: draft.endsAt,
      dueAt: draft.dueAt,
      priority: draft.priority,
      location: draft.location,
      recurrenceRule: draft.recurrenceRule,
      reminderMinutes: draft.reminderMinutes,
      categories: draft.categories,
      emails: draft.emails,
      phones: draft.phones,
      organization: draft.organization,
      jobTitle: draft.jobTitle,
      favorite: draft.favorite,
    );
  }

  @override
  Future<void> deletePimItem({required PimItem item}) async {}

  @override
  Future<CollectionEntry> createCollection({required String name}) async {
    createCollectionCalls += 1;
    final collection = CollectionEntry(
      id: '00000000-0000-4000-8000-${createCollectionCalls.toString().padLeft(12, '0')}',
      name: name,
      cmk: List<int>.filled(32, 5),
    );
    provisionedCollections = <CollectionEntry>[
      ...provisionedCollections,
      collection,
    ];
    return collection;
  }

  @override
  Future<void> moveCollectionToTrash({required String collectionId}) async {
    moveCollectionToTrashCalls += 1;
  }

  @override
  Future<void> registerCollectionKey({
    required String collectionId,
    required int keyEpoch,
    required int syncStartSeq,
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
  final Map<String, MobileDeviceVault> _vaults = <String, MobileDeviceVault>{};
  MobileDeviceVault? legacyVault;

  MobileDeviceVault? get vault =>
      _vaults.isEmpty ? legacyVault : _vaults.values.first;

  set vault(MobileDeviceVault? value) {
    _vaults.clear();
    legacyVault = null;
    if (value != null) {
      _vaults['${value.cloudBaseUrl}\u0000${value.username}'] = value;
    }
  }

  @override
  Future<MobileDeviceVault?> read({
    String? cloudBaseUrl,
    String? username,
  }) async {
    if (cloudBaseUrl == null || username == null) return legacyVault;
    return _vaults['$cloudBaseUrl\u0000$username'];
  }

  @override
  Future<void> write(MobileDeviceVault value) async {
    _vaults['${value.cloudBaseUrl}\u0000${value.username}'] = value;
  }
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
  final Map<String, RefreshCredential> _tokens = <String, RefreshCredential>{};
  PendingRefreshRevocation? _pending;

  @override
  Future<void> write({
    required String cloudBaseUrl,
    required String refreshToken,
    required String rotationRequestId,
  }) async {
    _tokens[cloudBaseUrl] = RefreshCredential(
      refreshToken: refreshToken,
      rotationRequestId: rotationRequestId,
    );
  }

  @override
  Future<String?> read({required String cloudBaseUrl}) async {
    return _tokens[cloudBaseUrl]?.refreshToken;
  }

  @override
  Future<RefreshCredential?> readCredential(
      {required String cloudBaseUrl}) async {
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
  final Set<String> calendarCollectionIds = <String>{};
  final Set<String> contactsCollectionIds = <String>{};
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
      calendarCollectionIds: Set.unmodifiable(calendarCollectionIds),
      contactsCollectionIds: Set.unmodifiable(contactsCollectionIds),
    );
  }

  @override
  Future<void> enableCalendar(String collectionId, List<PimItem> items) async {
    calendarCollectionIds.add(collectionId);
    projectCalls += 1;
  }

  @override
  Future<void> enableContacts(String collectionId, List<PimItem> items) async {
    contactsCollectionIds.add(collectionId);
    projectCalls += 1;
  }

  @override
  Future<void> disableCalendar(
    String collectionId, {
    required bool removeProjectedData,
  }) async {
    calendarCollectionIds.remove(collectionId);
    lastCalendarRemoveChoice = removeProjectedData;
  }

  @override
  Future<void> disableContacts(
    String collectionId, {
    required bool removeProjectedData,
  }) async {
    contactsCollectionIds.remove(collectionId);
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
    _FakeSyncRuntimeStorage? syncRuntimeStorage,
    _FakeDeviceVaultStorage? deviceVaultStorage,
  }) {
    return ProviderContainer(
      overrides: [
        rustBridgeProvider.overrideWith((ref) => bridge),
        refreshTokenStorageProvider.overrideWith(
          (ref) => refreshTokenStorage ?? _FakeRefreshTokenStorage(),
        ),
        syncRuntimeStorageProvider.overrideWith(
          (ref) => syncRuntimeStorage ?? _FakeSyncRuntimeStorage(),
        ),
        localCacheKeyStorageProvider.overrideWith(
          (ref) => _FakeLocalCacheKeyStorage(),
        ),
        deviceVaultStorageProvider.overrideWith(
          (ref) => deviceVaultStorage ?? _FakeDeviceVaultStorage(),
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

    test('settings reject insecure remote origins', () async {
      final bridge = _FakeRustBridgeApi();
      final container = createContainer(bridge: bridge);
      addTearDown(container.dispose);
      final controller = container.read(bridgeControllerProvider.notifier);

      await controller.updateCloudBaseUrl('http://example.com');

      final state = container.read(bridgeControllerProvider);
      expect(state.cloudBaseUrl, 'https://api.kamori.app');
      expect(state.error, contains('HTTPS'));
    });

    test('authenticated sessions cannot silently switch server origin',
        () async {
      final bridge = _FakeRustBridgeApi();
      final container = createContainer(bridge: bridge);
      addTearDown(container.dispose);
      final controller = container.read(bridgeControllerProvider.notifier);

      await controller.loginWithPassword(username: 'alice', password: 'secret');
      await controller.updateCloudBaseUrl('https://other.example');

      final state = container.read(bridgeControllerProvider);
      expect(state.cloudBaseUrl, 'https://api.kamori.app');
      expect(state.error, contains('Sign out'));
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

    test('persists a new device identity before server provisioning', () async {
      final bridge = _FakeRustBridgeApi();
      final vault = _FakeDeviceVaultStorage();
      bridge.beforeProvision = () async {
        expect(vault.vault, isNotNull);
        expect(
          vault.vault!.device.deviceId,
          '00000000-0000-4000-8000-000000000001',
        );
      };
      final container = createContainer(
        bridge: bridge,
        deviceVaultStorage: vault,
      );
      addTearDown(container.dispose);

      await container
          .read(bridgeControllerProvider.notifier)
          .loginWithPassword(username: 'alice', password: 'secret');

      expect(container.read(bridgeControllerProvider).isAuthenticated, isTrue);
      expect(bridge.provisionCalls, 1);
    });

    test('failed provisioning revokes the session but keeps device keys',
        () async {
      final bridge = _FakeRustBridgeApi()..failProvisioning = true;
      final vault = _FakeDeviceVaultStorage();
      final tokens = _FakeRefreshTokenStorage();
      final container = createContainer(
        bridge: bridge,
        deviceVaultStorage: vault,
        refreshTokenStorage: tokens,
      );
      addTearDown(container.dispose);

      await container
          .read(bridgeControllerProvider.notifier)
          .loginWithPassword(username: 'alice', password: 'secret');

      final state = container.read(bridgeControllerProvider);
      expect(state.isAuthenticated, isFalse);
      expect(state.accessToken, isNull);
      expect(vault.vault, isNotNull);
      expect(bridge.revokeCalls, 1);
      expect(
        await tokens.read(cloudBaseUrl: 'https://api.kamori.app'),
        isNull,
      );
    });

    test('failed provisioning for another account preserves existing keys',
        () async {
      final bridge = _FakeRustBridgeApi()
        ..failProvisioning = true
        ..passwordLoginResult = LoginResult(
          username: 'bob',
          accessToken: 'session-bob',
          totpContinuationToken: null,
          deviceEnrollmentToken: 'device-enrollment-bob',
          totpVerified: true,
          accountMasterKey: List<int>.filled(32, 21),
        );
      final vault = _FakeDeviceVaultStorage();
      final aliceDevice = DeviceSecrets(
        deviceId: '00000000-0000-4000-8000-0000000000aa',
        signingPrivateKey: List<int>.filled(32, 12),
        hpkePrivateKey: List<int>.filled(32, 13),
        hpkePublicKey: List<int>.filled(32, 14),
      );
      await vault.write(MobileDeviceVault(
        cloudBaseUrl: 'https://api.kamori.app',
        username: 'alice',
        accountMasterKey: List<int>.filled(32, 11),
        device: aliceDevice,
      ));
      final container = createContainer(
        bridge: bridge,
        deviceVaultStorage: vault,
      );
      addTearDown(container.dispose);

      await container
          .read(bridgeControllerProvider.notifier)
          .loginWithPassword(username: 'bob', password: 'secret');

      expect(
        (await vault.read(
          cloudBaseUrl: 'https://api.kamori.app',
          username: 'alice',
        ))
            ?.device
            .deviceId,
        aliceDevice.deviceId,
      );
      expect(
        await vault.read(
          cloudBaseUrl: 'https://api.kamori.app',
          username: 'bob',
        ),
        isNotNull,
      );
    });

    test('restored background preference remains disabled', () async {
      final bridge = _FakeRustBridgeApi();
      final runtime = _FakeSyncRuntimeStorage()
        ..snapshot = const MobileSyncRuntimeSnapshot(
          cloudBaseUrl: 'https://api.kamori.app',
          username: 'alice',
          sqlitePath: '.kamori/mobile-cache.sqlite3',
          accessToken: 'persisted-access',
          collections: <CollectionEntry>[],
          backgroundSyncEnabled: false,
        );
      final vault = _FakeDeviceVaultStorage()
        ..vault = MobileDeviceVault(
          cloudBaseUrl: 'https://api.kamori.app',
          username: 'alice',
          accountMasterKey: List<int>.filled(32, 1),
          device: DeviceSecrets(
            deviceId: '00000000-0000-4000-8000-000000000001',
            signingPrivateKey: List<int>.filled(32, 2),
            hpkePrivateKey: List<int>.filled(32, 3),
            hpkePublicKey: List<int>.filled(32, 4),
          ),
        );
      var scheduleCalls = 0;
      final container = createContainer(
        bridge: bridge,
        syncRuntimeStorage: runtime,
        deviceVaultStorage: vault,
        schedulePeriodicSync: () async => scheduleCalls += 1,
      );
      addTearDown(container.dispose);

      await container
          .read(bridgeControllerProvider.notifier)
          .restorePersistedSession();

      final state = container.read(bridgeControllerProvider);
      expect(state.isAuthenticated, isTrue);
      expect(state.backgroundSyncEnabled, isFalse);
      expect(scheduleCalls, 0);
      expect(bridge.provisionCalls, 1);
    });

    test('password login surfaces totp required message', () async {
      final bridge = _FakeRustBridgeApi()
        ..passwordLoginResult = const LoginResult(
          accessToken: null,
          totpContinuationToken: 'continuation',
          deviceEnrollmentToken: null,
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

      await controller.loginWithPassword(username: 'alice', password: 'secret');
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
      await controller.savePimItem(PimDraft(
        spaceId: spaceId,
        kind: PimItemKind.task,
        title: 'Ship MVP',
      ));

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
      await controller.createCollection('Personal');
      final collectionId =
          container.read(bridgeControllerProvider).collections.single.id;
      expect(
        container
            .read(bridgeControllerProvider)
            .calendarProjectionCollectionIds,
        isEmpty,
      );

      await controller.setCalendarProjectionEnabled(collectionId, true);
      expect(
        container
            .read(bridgeControllerProvider)
            .calendarProjectionCollectionIds,
        {collectionId},
      );

      await controller.setCalendarProjectionEnabled(
        collectionId,
        false,
        removeProjectedData: true,
      );
      expect(
        container
            .read(bridgeControllerProvider)
            .calendarProjectionCollectionIds,
        isEmpty,
      );
      expect(projection.lastCalendarRemoveChoice, isTrue);
    });

    test('system projection remains isolated to the selected collection',
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
      await controller.createCollection('Personal');
      await controller.createCollection('Work');
      final collections = container.read(bridgeControllerProvider).collections;

      await controller.setContactsProjectionEnabled(collections.first.id, true);

      expect(
        container
            .read(bridgeControllerProvider)
            .contactsProjectionCollectionIds,
        {collections.first.id},
      );
      expect(projection.contactsCollectionIds, {collections.first.id});
      expect(
        projection.contactsCollectionIds.contains(collections.last.id),
        isFalse,
      );
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
      await controller.savePimItem(PimDraft(
        spaceId: state.collections.single.id,
        kind: PimItemKind.task,
        title: 'Should not save',
      ));

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
      final rotated =
          container.read(bridgeControllerProvider).collections.single;
      expect(rotated.keyEpoch, 2);
      expect(rotated.historyStartSeq, 0);
      expect(rotated.currentStateStartSeq, 4);
      expect(rotated.cmk, everyElement(2));
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
