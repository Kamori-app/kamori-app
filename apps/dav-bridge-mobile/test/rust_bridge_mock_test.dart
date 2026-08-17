import 'package:flutter_test/flutter_test.dart';

import 'package:dav_bridge_mobile/src/rust/rust_bridge_mock.dart';

void main() {
  group('MockRustBridgeApi', () {
    test('returns failure when username or password is empty', () async {
      final bridge = MockRustBridgeApi();

      final result = await bridge.passwordLogin(
        cloudBaseUrl: 'http://127.0.0.1:3000',
        username: '',
        password: '',
      );

      expect(result.accessToken, isNull);
      expect(result.preauthToken, isNull);
      expect(result.totpVerified, isFalse);
    });

    test('returns preauth first and session after totp', () async {
      final bridge = MockRustBridgeApi();

      final preauth = await bridge.passwordLogin(
        cloudBaseUrl: 'http://127.0.0.1:3000',
        username: 'alice',
        password: 'secret',
      );
      final authed = await bridge.passwordLogin(
        cloudBaseUrl: 'http://127.0.0.1:3000',
        username: 'alice',
        password: 'secret',
        totpCode: '123456',
      );

      expect(preauth.accessToken, isNull);
      expect(preauth.preauthToken, isNotNull);
      expect(authed.accessToken, isNotNull);
      expect(authed.totpVerified, isTrue);
    });

    test('configures offline synchronization without a local server', () async {
      final bridge = MockRustBridgeApi();

      await bridge.configureSync(
        cloudBaseUrl: 'http://127.0.0.1:3000',
        sqlitePath: '.kamori/mobile-cache.sqlite3',
        accessToken: 'session',
        sqliteKey: List<int>.filled(32, 7),
      );
      final synced = await bridge.syncNow();
      expect(synced, 1);
    });

    test('rejects invalid collection key registration payload', () async {
      final bridge = MockRustBridgeApi();

      expect(
        () => bridge.registerCollectionKey(
            collectionId: '', keyEpoch: 1, cmk: List.filled(32, 1)),
        throwsArgumentError,
      );
      expect(
        () => bridge.registerCollectionKey(
          collectionId: 'collection-1',
          keyEpoch: 1,
          cmk: List.filled(31, 1),
        ),
        throwsArgumentError,
      );
    });

    test('creates and redeems invite code', () async {
      final bridge = MockRustBridgeApi();

      final issued = await bridge.createInviteCode(
        collectionId: 'collection-1',
        collectionKey: List.filled(32, 2),
        ttlMinutes: 30,
      );

      final redeemed = await bridge.redeemInviteCode(inviteCode: issued.code);

      expect(issued.code, matches(RegExp(r'^[A-Z0-9]{4}(-[A-Z0-9]{4}){3}$')));
      expect(redeemed.collectionId, 'collection-1');
      expect(redeemed.collectionKey, hasLength(32));
    });
  });
}
