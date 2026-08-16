import 'package:flutter/foundation.dart';

import 'package:dav_bridge_mobile/src/models/bridge_models.dart';
import 'package:dav_bridge_mobile/src/rust/rust_bridge_frb.dart';
import 'package:dav_bridge_mobile/src/rust/rust_bridge_mock.dart';

/// Platform bridge contract used by Flutter state layer.
///
/// Implementations can be backed by either generated FRB bindings or a local
/// mock used during early product development.
abstract class RustBridgeApi {
  /// Performs password-based sign-in.
  ///
  /// Returns either an access token or a pre-auth token if additional TOTP
  /// verification is required.
  Future<LoginResult> passwordLogin({
    required String cloudBaseUrl,
    required String username,
    required String password,
    String? totpCode,
  });

  Future<ProvisionResult> provisionDeviceAndSpaces({
    required String cloudBaseUrl,
    required String accessToken,
    required List<int> accountMasterKey,
    required String platform,
    DeviceSecrets? existingDevice,
  });

  /// Imports refresh token into Rust runtime memory for authenticated refresh flow.
  Future<void> importRefreshToken({required String refreshToken});

  /// Exports current refresh token from Rust runtime memory.
  Future<String?> exportRefreshToken();

  /// Clears refresh token from Rust runtime memory.
  Future<void> clearRefreshToken();

  /// Configures authenticated offline synchronization without a local server.
  Future<void> configureSync({
    required String cloudBaseUrl,
    required String sqlitePath,
    required String accessToken,
    required List<int> sqliteKey,
    DeviceSecrets? device,
  });

  /// Triggers one explicit sync cycle and returns number of synced items.
  Future<int> syncNow();

  Future<List<PimItem>> listPimItems();

  Future<PimItem> upsertPimItem({
    required String spaceId,
    String? resourceId,
    required PimItemKind kind,
    required String title,
    bool completed,
    String? email,
    String? phone,
    String? startsAt,
    String? endsAt,
  });

  Future<void> deletePimItem({required PimItem item});

  Future<CollectionEntry> createCollection({required String name});

  /// Moves an owned security space into the server-side 30-day trash.
  Future<void> moveCollectionToTrash({required String collectionId});

  /// Registers collection master key in local runtime state.
  Future<void> registerCollectionKey({
    required String collectionId,
    required int keyEpoch,
    required List<int> cmk,
  });

  /// Removes collection master key from local runtime state.
  Future<void> unregisterCollectionKey({required String collectionId});

  /// Creates a short-lived invite code for a collection.
  Future<IssuedInviteCode> createInviteCode({
    required String collectionId,
    required List<int> collectionKey,
    required int ttlMinutes,
  });

  /// Redeems an invite code and returns collection id + collection key.
  Future<RedeemedInvite> redeemInviteCode({required String inviteCode});
}

class RustBridgeFactory {
  /// Creates bridge implementation for current run mode.
  ///
  /// In debug mode, `KAMORI_USE_MOCK_RUST` defaults to `true`, so mock bridge
  /// is used unless explicitly disabled.
  static RustBridgeApi create() {
    const useMock = bool.fromEnvironment(
      'KAMORI_USE_MOCK_RUST',
      defaultValue: true,
    );
    if (kDebugMode && useMock) {
      return MockRustBridgeApi();
    }
    return FrbRustBridgeApi();
  }
}
