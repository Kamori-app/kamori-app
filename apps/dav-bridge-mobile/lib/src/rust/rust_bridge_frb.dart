import 'dart:typed_data';

import 'package:dav_bridge_mobile/src/models/bridge_models.dart';
import 'package:dav_bridge_mobile/src/rust/gen/frb_api.dart' as frb_api;
import 'package:dav_bridge_mobile/src/rust/gen/frb_api/types.dart' as frb_types;
import 'package:dav_bridge_mobile/src/rust/gen/frb_generated.dart' show RustLib;
import 'package:dav_bridge_mobile/src/rust/gen/lib.dart' show U8Array32;
import 'package:dav_bridge_mobile/src/rust/rust_bridge_api.dart';

/// Production bridge implementation backed by generated FRB bindings.
///
/// This class calls `crypto-core-lib` through `flutter_rust_bridge` and exposes
/// the same interface as [RustBridgeApi].
class FrbRustBridgeApi implements RustBridgeApi {
  static Future<void>? _initFuture;

  static Future<void> _ensureInitialized() {
    return _initFuture ??= RustLib.init();
  }

  static U8Array32 _asFixed32Bytes(String fieldName, List<int> bytes) {
    if (bytes.length != U8Array32.arraySize) {
      throw ArgumentError.value(
        bytes.length,
        fieldName,
        'must be exactly ${U8Array32.arraySize} bytes',
      );
    }
    return U8Array32(Uint8List.fromList(bytes));
  }

  static frb_types.MobileDeviceSecrets _toFrbDevice(DeviceSecrets value) =>
      frb_types.MobileDeviceSecrets(
        deviceId: value.deviceId,
        signingPrivateKey: _asFixed32Bytes(
          'signingPrivateKey',
          value.signingPrivateKey,
        ),
        hpkePrivateKey: _asFixed32Bytes(
          'hpkePrivateKey',
          value.hpkePrivateKey,
        ),
        hpkePublicKey: _asFixed32Bytes('hpkePublicKey', value.hpkePublicKey),
      );

  static DeviceSecrets _fromFrbDevice(frb_types.MobileDeviceSecrets value) =>
      DeviceSecrets(
        deviceId: value.deviceId,
        signingPrivateKey: value.signingPrivateKey.toList(growable: false),
        hpkePrivateKey: value.hpkePrivateKey.toList(growable: false),
        hpkePublicKey: value.hpkePublicKey.toList(growable: false),
      );

  /// Executes cloud OPAQUE password login over MessagePack transport.
  @override
  Future<LoginResult> passwordLogin({
    required String cloudBaseUrl,
    required String username,
    required String password,
    String? totpCode,
  }) async {
    await _ensureInitialized();
    final result = await frb_api.mobilePasswordLogin(
      cloudBaseUrl: cloudBaseUrl,
      username: username,
      password: password,
      totpCode: totpCode,
    );
    return LoginResult(
      username: result.username,
      accessToken: result.accessToken,
      preauthToken: result.preauthToken,
      totpVerified: result.totpVerified,
      accountMasterKey: result.accountMasterKey?.toList(growable: false),
    );
  }

  @override
  Future<ProvisionResult> provisionDeviceAndSpaces({
    required String cloudBaseUrl,
    required String accessToken,
    required List<int> accountMasterKey,
    required String platform,
    DeviceSecrets? existingDevice,
  }) async {
    await _ensureInitialized();
    final result = await frb_api.mobileProvisionDeviceAndSpaces(
      cloudBaseUrl: cloudBaseUrl,
      accessToken: accessToken,
      accountMasterKey: _asFixed32Bytes(
        'accountMasterKey',
        accountMasterKey,
      ),
      platform: platform,
      existingDevice:
          existingDevice == null ? null : _toFrbDevice(existingDevice),
    );
    return ProvisionResult(
      accessToken: result.accessToken,
      device: _fromFrbDevice(result.device),
      collections: result.collections
          .map(
            (collection) => CollectionEntry(
              id: collection.collectionId,
              name: collection.name,
              cmk: collection.collectionKey.toList(growable: false),
              keyEpoch: collection.keyEpoch,
              role: collection.role,
            ),
          )
          .toList(growable: false),
    );
  }

  /// Imports refresh token into Rust runtime memory.
  @override
  Future<void> importRefreshToken({required String refreshToken}) async {
    await _ensureInitialized();
    await frb_api.mobileImportRefreshToken(refreshToken: refreshToken);
  }

  /// Exports refresh token from Rust runtime memory.
  @override
  Future<String?> exportRefreshToken() async {
    await _ensureInitialized();
    return frb_api.mobileExportRefreshToken();
  }

  /// Clears refresh token from Rust runtime memory.
  @override
  Future<void> clearRefreshToken() async {
    await _ensureInitialized();
    await frb_api.mobileClearRefreshToken();
  }

  @override
  Future<bool> revokeRefreshSession({
    required String cloudBaseUrl,
    required String refreshToken,
  }) async {
    await _ensureInitialized();
    return frb_api.mobileRevokeRefreshSession(
      cloudBaseUrl: cloudBaseUrl,
      refreshToken: refreshToken,
    );
  }

  /// Configures the authenticated offline sync runtime.
  @override
  Future<void> configureSync({
    required String cloudBaseUrl,
    required String sqlitePath,
    required String accessToken,
    required List<int> sqliteKey,
    DeviceSecrets? device,
  }) async {
    await _ensureInitialized();
    await frb_api.mobileConfigureSync(
      cloudBaseUrl: cloudBaseUrl,
      sqlitePath: sqlitePath,
      accessToken: accessToken,
      sqliteKey: _asFixed32Bytes('sqliteKey', sqliteKey),
      device: device == null ? null : _toFrbDevice(device),
    );
  }

  /// Triggers one sync cycle and returns number of applied events.
  @override
  Future<int> syncNow() async {
    await _ensureInitialized();
    final synced = await frb_api.mobileSyncNow();
    return synced.toInt();
  }

  PimItem _fromFrbPimItem(frb_types.MobilePimItem item) => PimItem(
        spaceId: item.spaceId,
        resourceId: item.resourceId,
        kind: PimItemKind.fromWireName(item.resourceKind),
        title: item.title,
        completed: item.completed,
        email: item.email,
        phone: item.phone,
        startsAt: item.startsAt,
        endsAt: item.endsAt,
        conflict: item.conflict,
      );

  @override
  Future<List<PimItem>> listPimItems() async {
    await _ensureInitialized();
    final items = await frb_api.mobileListPimItems();
    return items.map(_fromFrbPimItem).toList(growable: false);
  }

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
    await _ensureInitialized();
    final item = await frb_api.mobileUpsertPimItem(
      spaceId: spaceId,
      resourceId: resourceId,
      resourceKind: kind.wireName,
      title: title,
      completed: completed,
      email: email,
      phone: phone,
      startsAt: startsAt,
      endsAt: endsAt,
    );
    return _fromFrbPimItem(item);
  }

  @override
  Future<void> deletePimItem({required PimItem item}) async {
    await _ensureInitialized();
    await frb_api.mobileDeletePimItem(
      spaceId: item.spaceId,
      resourceId: item.resourceId,
      resourceKind: item.kind.wireName,
    );
  }

  @override
  Future<CollectionEntry> createCollection({required String name}) async {
    await _ensureInitialized();
    final collection = await frb_api.mobileCreateCollection(name: name);
    return CollectionEntry(
      id: collection.collectionId,
      name: collection.name,
      cmk: collection.collectionKey.toList(growable: false),
      keyEpoch: collection.keyEpoch,
      role: collection.role,
    );
  }

  @override
  Future<void> moveCollectionToTrash({required String collectionId}) async {
    await _ensureInitialized();
    await frb_api.mobileMoveCollectionToTrash(collectionId: collectionId);
  }

  /// Registers in-memory CMK for the local collection id.
  @override
  Future<void> registerCollectionKey({
    required String collectionId,
    required int keyEpoch,
    required List<int> cmk,
  }) async {
    await _ensureInitialized();
    await frb_api.mobileRegisterCollectionKey(
      collectionId: collectionId,
      keyEpoch: keyEpoch,
      cmk: _asFixed32Bytes('cmk', cmk),
    );
  }

  /// Removes CMK registration for a collection id.
  @override
  Future<void> unregisterCollectionKey({required String collectionId}) async {
    await _ensureInitialized();
    await frb_api.mobileUnregisterCollectionKey(collectionId: collectionId);
  }

  /// Issues a short-lived invite code for a collection.
  @override
  Future<IssuedInviteCode> createInviteCode({
    required String collectionId,
    required List<int> collectionKey,
    required int ttlMinutes,
  }) async {
    await _ensureInitialized();
    final issued = await frb_api.mobileCreateInviteCode(
      collectionId: collectionId,
      collectionKey: _asFixed32Bytes('collectionKey', collectionKey),
      ttlMinutes: ttlMinutes,
    );
    return IssuedInviteCode(
      code: issued.code,
      ttlMinutes: issued.ttlMinutes.toInt(),
    );
  }

  /// Redeems a short-lived invite code and returns collection key material.
  @override
  Future<RedeemedInvite> redeemInviteCode({required String inviteCode}) async {
    await _ensureInitialized();
    final redeemed =
        await frb_api.mobileRedeemInviteCode(inviteCode: inviteCode);
    return RedeemedInvite(
      collectionId: redeemed.collectionId,
      role: redeemed.role,
      keyEpoch: redeemed.keyEpoch.toInt(),
      collectionKey: redeemed.collectionKey.toList(growable: false),
    );
  }
}
