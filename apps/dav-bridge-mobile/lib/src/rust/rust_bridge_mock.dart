import 'dart:math';

import 'package:uuid/uuid.dart';

import 'package:dav_bridge_mobile/src/models/bridge_models.dart';
import 'package:dav_bridge_mobile/src/rust/rust_bridge_api.dart';

class _MockInviteCodeEntry {
  const _MockInviteCodeEntry({
    required this.collectionId,
    required this.collectionKey,
    required this.expiresAt,
  });

  final String collectionId;
  final List<int> collectionKey;
  final DateTime expiresAt;
}

/// In-memory bridge implementation for local UI development and tests.
class MockRustBridgeApi implements RustBridgeApi {
  String? _accessToken;
  String? _refreshToken;
  String? _refreshRotationRequestId;
  String? _cloudBaseUrl;
  String? _sqlitePath;
  final Map<String, List<int>> _collectionKeys = <String, List<int>>{};
  final List<PimItem> _pimItems = <PimItem>[];
  final Map<String, _MockInviteCodeEntry> _inviteCodes =
      <String, _MockInviteCodeEntry>{};

  @override
  Future<DeviceSecrets> generateDeviceSecrets() async => DeviceSecrets(
        deviceId: const Uuid().v4(),
        signingPrivateKey:
            List<int>.generate(32, (_) => Random.secure().nextInt(256)),
        hpkePrivateKey:
            List<int>.generate(32, (_) => Random.secure().nextInt(256)),
        hpkePublicKey:
            List<int>.generate(32, (_) => Random.secure().nextInt(256)),
      );

  @override
  Future<LoginResult> passwordLogin({
    required String cloudBaseUrl,
    required String username,
    required String password,
    String? totpCode,
  }) async {
    if (username.trim().isEmpty || password.isEmpty) {
      return const LoginResult(
        accessToken: null,
        totpContinuationToken: null,
        deviceEnrollmentToken: null,
        totpVerified: false,
      );
    }

    if ((totpCode ?? '').trim().isEmpty) {
      return const LoginResult(
        accessToken: null,
        totpContinuationToken: 'mock-totp-continuation',
        deviceEnrollmentToken: null,
        totpVerified: false,
      );
    }

    _accessToken = 'mock-access-token';
    _refreshToken = 'mock-refresh-token';
    _refreshRotationRequestId = const Uuid().v4();
    return const LoginResult(
      username: 'mock-user',
      accessToken: 'mock-access-token',
      totpContinuationToken: null,
      deviceEnrollmentToken: 'mock-device-enrollment',
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
    if (accountMasterKey.length != 32) {
      throw ArgumentError('Account master key must contain exactly 32 bytes');
    }
    final device = existingDevice ??
        DeviceSecrets(
          deviceId: '00000000-0000-4000-8000-000000000001',
          signingPrivateKey: List<int>.filled(32, 2),
          hpkePrivateKey: List<int>.filled(32, 3),
          hpkePublicKey: List<int>.filled(32, 4),
        );
    return ProvisionResult(
      accessToken: accessToken,
      device: device,
      collections: const <CollectionEntry>[],
    );
  }

  @override
  Future<void> importRefreshToken({
    required String refreshToken,
    required String rotationRequestId,
  }) async {
    final value = refreshToken.trim();
    if (value.isEmpty) {
      throw ArgumentError('Refresh token is required');
    }
    _refreshToken = value;
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
    return refreshToken.trim().isNotEmpty;
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
      throw ArgumentError('SQLite key must contain exactly 32 bytes');
    }
    _cloudBaseUrl = cloudBaseUrl;
    _sqlitePath = sqlitePath;
    _accessToken = accessToken;
  }

  @override
  Future<int> syncNow() async {
    return _cloudBaseUrl != null && _sqlitePath != null && _accessToken != null
        ? 1
        : 0;
  }

  @override
  Future<List<PimItem>> listPimItems() async =>
      List<PimItem>.unmodifiable(_pimItems);

  @override
  Future<PimItem> upsertPimItem({required PimDraft draft}) async {
    if (!_collectionKeys.containsKey(draft.spaceId) ||
        draft.title.trim().isEmpty) {
      throw ArgumentError('Invalid PIM item');
    }
    final logicalId = draft.resourceId ?? const Uuid().v4();
    final item = PimItem(
      spaceId: draft.spaceId,
      resourceId: logicalId,
      projectionId: draft.projectionId ??
          '$logicalId.${draft.kind == PimItemKind.contact ? 'vcf' : 'ics'}',
      headOperationId: const Uuid().v4(),
      kind: draft.kind,
      title: draft.title.trim(),
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
      namePrefix: draft.namePrefix,
      givenName: draft.givenName,
      middleName: draft.middleName,
      familyName: draft.familyName,
      nameSuffix: draft.nameSuffix,
      emails: draft.emails,
      phones: draft.phones,
      addresses: draft.addresses,
      organization: draft.organization,
      jobTitle: draft.jobTitle,
      birthday: draft.birthday,
      url: draft.url,
      favorite: draft.favorite,
    );
    _pimItems.removeWhere(
      (existing) =>
          existing.spaceId == item.spaceId &&
          existing.projectionId == item.projectionId,
    );
    _pimItems.add(item);
    return item;
  }

  @override
  Future<void> deletePimItem({required PimItem item}) async {
    _pimItems.removeWhere(
      (existing) =>
          existing.spaceId == item.spaceId &&
          existing.projectionId == item.projectionId,
    );
  }

  @override
  Future<CollectionEntry> createCollection({required String name}) async {
    final normalized = name.trim();
    if (normalized.isEmpty) {
      throw ArgumentError('Collection name is required');
    }
    final random = Random.secure();
    final collection = CollectionEntry(
      id: const Uuid().v4(),
      name: normalized,
      cmk: List<int>.generate(32, (_) => random.nextInt(256)),
    );
    _collectionKeys[collection.id] = collection.cmk;
    return collection;
  }

  @override
  Future<void> moveCollectionToTrash({required String collectionId}) async {
    if (collectionId.isEmpty || !_collectionKeys.containsKey(collectionId)) {
      throw ArgumentError('Collection not found');
    }
    _collectionKeys.remove(collectionId);
  }

  @override
  Future<void> registerCollectionKey({
    required String collectionId,
    required int keyEpoch,
    required int syncStartSeq,
    required List<int> cmk,
  }) async {
    if (collectionId.isEmpty || keyEpoch < 1 || cmk.length != 32) {
      throw ArgumentError('Invalid collection key payload');
    }
    _collectionKeys[collectionId] = List<int>.from(cmk);
  }

  @override
  Future<void> unregisterCollectionKey({required String collectionId}) async {
    if (collectionId.isEmpty) {
      throw ArgumentError('Collection id is required');
    }
    _collectionKeys.remove(collectionId);
  }

  @override
  Future<IssuedInviteCode> createInviteCode({
    required String collectionId,
    required List<int> collectionKey,
    required int ttlMinutes,
  }) async {
    if (collectionId.isEmpty || collectionKey.length != 32) {
      throw ArgumentError('Invalid invite payload');
    }
    if (ttlMinutes < 15 || ttlMinutes > 7 * 24 * 60) {
      throw ArgumentError('ttlMinutes must be between 15 and 10080');
    }

    final rotatedKey = List<int>.generate(
      32,
      (index) => collectionKey[index] ^ 0x5a,
      growable: false,
    );
    final code = _generateInviteCode();
    _inviteCodes[code] = _MockInviteCodeEntry(
      collectionId: collectionId,
      collectionKey: rotatedKey,
      expiresAt: DateTime.now().add(Duration(minutes: ttlMinutes)),
    );
    return IssuedInviteCode(
      code: code,
      ttlMinutes: ttlMinutes,
      keyEpoch: 2,
      currentStateStartSeq: 0,
      collectionKey: rotatedKey,
    );
  }

  @override
  Future<RedeemedInvite> redeemInviteCode({required String inviteCode}) async {
    final normalized = _normalizeInviteCode(inviteCode);
    if (normalized.isEmpty) {
      throw ArgumentError('Invite code is required');
    }

    final entry = _inviteCodes[normalized];
    if (entry == null) {
      throw StateError('Invite code not found');
    }
    if (DateTime.now().isAfter(entry.expiresAt)) {
      _inviteCodes.remove(normalized);
      throw StateError('Invite code has expired');
    }

    _inviteCodes.remove(normalized);
    _collectionKeys[entry.collectionId] = List<int>.from(entry.collectionKey);

    return RedeemedInvite(
      collectionId: entry.collectionId,
      role: 'editor',
      keyEpoch: 2,
      collectionKey: List<int>.from(entry.collectionKey),
    );
  }

  String _generateInviteCode() {
    const alphabet = 'ABCDEFGHJKLMNPQRSTUVWXYZ23456789';
    final rng = Random.secure();
    final chars = List<String>.generate(
      16,
      (_) => alphabet[rng.nextInt(alphabet.length)],
      growable: false,
    );
    final code = StringBuffer()
      ..writeAll(chars.sublist(0, 4))
      ..write('-')
      ..writeAll(chars.sublist(4, 8))
      ..write('-')
      ..writeAll(chars.sublist(8, 12))
      ..write('-')
      ..writeAll(chars.sublist(12, 16));
    return code.toString();
  }

  String _normalizeInviteCode(String value) {
    final normalized = value.toUpperCase().replaceAll(RegExp(r'[^A-Z0-9]'), '');
    if (normalized.length != 16) {
      return '';
    }
    return '${normalized.substring(0, 4)}-${normalized.substring(4, 8)}-${normalized.substring(8, 12)}-${normalized.substring(12, 16)}';
  }
}
