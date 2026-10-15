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
  frb_api.MobileSessionLease? _activeSession;

  frb_api.MobileSessionLease get _session =>
      _activeSession ?? (throw StateError('Mobile session lease is required.'));

  @override
  Future<SessionLease> acquireSessionLease() async {
    await _ensureInitialized();
    final native = frb_api.mobileNewSessionLease();
    try {
      await frb_api.mobileAcquireSessionLease(session: native);
    } catch (_) {
      native.dispose();
      rethrow;
    }
    _activeSession = native;
    return _FrbSessionLease(native, () => _activeSession = null);
  }

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
      session: _session,
      cloudBaseUrl: cloudBaseUrl,
      username: username,
      password: password,
      totpCode: totpCode,
    );
    return LoginResult(
      username: result.username,
      accessToken: result.accessToken,
      totpContinuationToken: result.totpContinuationToken,
      deviceEnrollmentToken: result.deviceEnrollmentToken,
      totpVerified: result.totpVerified,
      accountMasterKey: result.accountMasterKey?.toList(growable: false),
    );
  }

  @override
  Future<DeviceSecrets> generateDeviceSecrets() async {
    await _ensureInitialized();
    return _fromFrbDevice(await frb_api.mobileGenerateDeviceSecrets());
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
    await _ensureInitialized();
    final result = await frb_api.mobileProvisionDeviceAndSpaces(
      session: _session,
      cloudBaseUrl: cloudBaseUrl,
      accessToken: accessToken,
      accountMasterKey: _asFixed32Bytes(
        'accountMasterKey',
        accountMasterKey,
      ),
      platform: platform,
      deviceEnrollmentToken: deviceEnrollmentToken,
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
              historyStartSeq: collection.historyStartSeq.toInt(),
              currentStateStartSeq: collection.currentStateStartSeq.toInt(),
              role: collection.role,
            ),
          )
          .toList(growable: false),
    );
  }

  /// Imports refresh token into Rust runtime memory.
  @override
  Future<void> importRefreshToken({
    required String refreshToken,
    required String rotationRequestId,
  }) async {
    await _ensureInitialized();
    await frb_api.mobileImportRefreshToken(
      session: _session,
      refreshToken: refreshToken,
      rotationRequestId: rotationRequestId,
    );
  }

  /// Exports refresh token from Rust runtime memory.
  @override
  Future<String?> exportRefreshToken() async {
    await _ensureInitialized();
    return frb_api.mobileExportRefreshToken(
      session: _session,
    );
  }

  @override
  Future<String?> exportRefreshRotationRequestId() async {
    await _ensureInitialized();
    return frb_api.mobileExportRefreshRotationRequestId(
      session: _session,
    );
  }

  /// Clears refresh token from Rust runtime memory.
  @override
  Future<void> clearRefreshToken() async {
    await _ensureInitialized();
    await frb_api.mobileClearRefreshToken(
      session: _session,
    );
  }

  @override
  Future<bool> revokeRefreshSession({
    required String cloudBaseUrl,
    required String refreshToken,
  }) async {
    await _ensureInitialized();
    return frb_api.mobileRevokeRefreshSession(
      session: _session,
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
    required String username,
    bool freshSession = false,
  }) async {
    await _ensureInitialized();
    await frb_api.mobileConfigureSync(
      session: _session,
      cloudBaseUrl: cloudBaseUrl,
      sqlitePath: sqlitePath,
      accessToken: accessToken,
      sqliteKey: _asFixed32Bytes('sqliteKey', sqliteKey),
      device: device == null ? null : _toFrbDevice(device),
      username: username,
      freshSession: freshSession,
    );
  }

  /// Triggers one sync cycle and returns number of applied events.
  @override
  Future<int> syncNow() async {
    await _ensureInitialized();
    final synced = await frb_api.mobileSyncNow(
      session: _session,
    );
    return synced.toInt();
  }

  static PimTemporal? _fromFrbTemporal(frb_types.MobilePimTemporal? value) =>
      value == null
          ? null
          : PimTemporal(
              kind: value.kind,
              date: value.date,
              utc: value.utc,
              local: value.local,
              timezone: value.timezone,
            );

  static frb_types.MobilePimTemporal? _toFrbTemporal(PimTemporal? value) =>
      value == null
          ? null
          : frb_types.MobilePimTemporal(
              kind: value.kind,
              date: value.date,
              utc: value.utc,
              local: value.local,
              timezone: value.timezone,
            );

  PimItem _fromFrbPimItem(frb_types.MobilePimItem item) => PimItem(
        spaceId: item.spaceId,
        resourceId: item.resourceId,
        projectionId: item.projectionId,
        headOperationId: item.headOperationId,
        kind: PimItemKind.fromWireName(item.resourceKind),
        title: item.title,
        completed: item.completed,
        completedAt: item.completedAt,
        notes: item.notes,
        startsAt: _fromFrbTemporal(item.startsAt),
        endsAt: _fromFrbTemporal(item.endsAt),
        dueAt: _fromFrbTemporal(item.dueAt),
        priority: item.priority.toInt(),
        location: item.location,
        recurrenceRule: item.recurrenceRule,
        recurrenceUnsupported: item.recurrenceUnsupported,
        recurrenceExdates: item.recurrenceExdates
            .map((value) => _fromFrbTemporal(value)!)
            .toList(growable: false),
        recurrenceRdates: item.recurrenceRdates
            .map((value) => _fromFrbTemporal(value)!)
            .toList(growable: false),
        recurrenceOverrides: item.recurrenceOverrides
            .map((value) => PimRecurrenceOverride(
                  recurrenceId: _fromFrbTemporal(value.recurrenceId)!,
                  startsAt: _fromFrbTemporal(value.startsAt),
                  endsAt: _fromFrbTemporal(value.endsAt),
                  title: value.title,
                  notes: value.notes,
                  location: value.location,
                  cancelled: value.cancelled,
                ))
            .toList(growable: false),
        reminderMinutes: item.reminderMinutes?.toInt(),
        categories: item.categories,
        namePrefix: item.namePrefix,
        givenName: item.givenName,
        middleName: item.middleName,
        familyName: item.familyName,
        nameSuffix: item.nameSuffix,
        emails: item.emails
            .map((value) => PimLabeledValue(
                  label: value.label,
                  value: value.value,
                  rawHead: value.rawHead,
                ))
            .toList(growable: false),
        phones: item.phones
            .map((value) => PimLabeledValue(
                  label: value.label,
                  value: value.value,
                  rawHead: value.rawHead,
                ))
            .toList(growable: false),
        addresses: item.addresses
            .map((value) => PimPostalAddress(
                  label: value.label,
                  rawHead: value.rawHead,
                  poBox: value.poBox,
                  extended: value.extended,
                  street: value.street,
                  locality: value.locality,
                  region: value.region,
                  postalCode: value.postalCode,
                  country: value.country,
                ))
            .toList(growable: false),
        organization: item.organization,
        jobTitle: item.jobTitle,
        birthday: item.birthday,
        url: item.url,
        favorite: item.favorite,
        conflict: item.conflict,
      );

  @override
  Future<List<PimItem>> listPimItems() async {
    await _ensureInitialized();
    final items = await frb_api.mobileListPimItems(
      session: _session,
    );
    return items.map(_fromFrbPimItem).toList(growable: false);
  }

  @override
  Future<PimItem> upsertPimItem({required PimDraft draft}) async {
    await _ensureInitialized();
    final item = await frb_api.mobileUpsertPimItem(
      session: _session,
      draft: frb_types.MobilePimDraft(
        spaceId: draft.spaceId,
        resourceId: draft.resourceId,
        projectionId: draft.projectionId,
        headOperationId: draft.headOperationId,
        resourceKind: draft.kind.wireName,
        title: draft.title,
        completed: draft.completed,
        completedAt: draft.completedAt,
        notes: draft.notes,
        startsAt: _toFrbTemporal(draft.startsAt),
        endsAt: _toFrbTemporal(draft.endsAt),
        dueAt: _toFrbTemporal(draft.dueAt),
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
        emails: draft.emails
            .map((value) => frb_types.MobileLabeledValue(
                  label: value.label,
                  value: value.value,
                  rawHead: value.rawHead,
                ))
            .toList(growable: false),
        phones: draft.phones
            .map((value) => frb_types.MobileLabeledValue(
                  label: value.label,
                  value: value.value,
                  rawHead: value.rawHead,
                ))
            .toList(growable: false),
        addresses: draft.addresses
            .map((value) => frb_types.MobilePostalAddress(
                  label: value.label,
                  rawHead: value.rawHead,
                  poBox: value.poBox,
                  extended: value.extended,
                  street: value.street,
                  locality: value.locality,
                  region: value.region,
                  postalCode: value.postalCode,
                  country: value.country,
                ))
            .toList(growable: false),
        organization: draft.organization,
        jobTitle: draft.jobTitle,
        birthday: draft.birthday,
        url: draft.url,
        favorite: draft.favorite,
      ),
    );
    return _fromFrbPimItem(item);
  }

  @override
  Future<void> deletePimItem({required PimItem item}) async {
    await _ensureInitialized();
    await frb_api.mobileDeletePimItem(
      session: _session,
      spaceId: item.spaceId,
      resourceId: item.resourceId,
      projectionId: item.projectionId,
      headOperationId: item.headOperationId,
      resourceKind: item.kind.wireName,
    );
  }

  @override
  Future<CollectionEntry> createCollection({required String name}) async {
    await _ensureInitialized();
    final collection =
        await frb_api.mobileCreateCollection(session: _session, name: name);
    return CollectionEntry(
      id: collection.collectionId,
      name: collection.name,
      cmk: collection.collectionKey.toList(growable: false),
      keyEpoch: collection.keyEpoch,
      historyStartSeq: collection.historyStartSeq.toInt(),
      currentStateStartSeq: collection.currentStateStartSeq.toInt(),
      role: collection.role,
    );
  }

  @override
  Future<void> moveCollectionToTrash({required String collectionId}) async {
    await _ensureInitialized();
    await frb_api.mobileMoveCollectionToTrash(
        session: _session, collectionId: collectionId);
  }

  /// Registers in-memory CMK for the local collection id.
  @override
  Future<void> registerCollectionKey({
    required String collectionId,
    required int keyEpoch,
    required int syncStartSeq,
    required List<int> cmk,
  }) async {
    await _ensureInitialized();
    await frb_api.mobileRegisterCollectionKey(
      session: _session,
      collectionId: collectionId,
      keyEpoch: keyEpoch,
      syncStartSeq: BigInt.from(syncStartSeq),
      cmk: _asFixed32Bytes('cmk', cmk),
    );
  }

  /// Removes CMK registration for a collection id.
  @override
  Future<void> unregisterCollectionKey({required String collectionId}) async {
    await _ensureInitialized();
    await frb_api.mobileUnregisterCollectionKey(
        session: _session, collectionId: collectionId);
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
      session: _session,
      collectionId: collectionId,
      collectionKey: _asFixed32Bytes('collectionKey', collectionKey),
      ttlMinutes: ttlMinutes,
    );
    return IssuedInviteCode(
      code: issued.code,
      ttlMinutes: issued.ttlMinutes.toInt(),
      keyEpoch: issued.keyEpoch.toInt(),
      currentStateStartSeq: issued.currentStateStartSeq.toInt(),
      collectionKey: issued.collectionKey.toList(growable: false),
    );
  }

  /// Redeems a short-lived invite code and returns collection key material.
  @override
  Future<RedeemedInvite> redeemInviteCode({required String inviteCode}) async {
    await _ensureInitialized();
    final redeemed = await frb_api.mobileRedeemInviteCode(
        session: _session, inviteCode: inviteCode);
    return RedeemedInvite(
      collectionId: redeemed.collectionId,
      role: redeemed.role,
      keyEpoch: redeemed.keyEpoch.toInt(),
      historyStartSeq: redeemed.historyStartSeq.toInt(),
      currentStateStartSeq: redeemed.currentStateStartSeq.toInt(),
      collectionKey: redeemed.collectionKey.toList(growable: false),
    );
  }
}

class _FrbSessionLease implements SessionLease {
  _FrbSessionLease(this._native, this._clearActive);
  final frb_api.MobileSessionLease _native;
  void Function()? _clearActive;

  @override
  void release() {
    final clear = _clearActive;
    if (clear == null) return;
    _clearActive = null;
    clear();
    // dispose decrements only Dart's opaque Arc. Every pending native call
    // owns another Arc, so engine teardown cannot unlock ongoing Rust work.
    _native.dispose();
  }
}
