import 'dart:convert';

import 'package:crypto/crypto.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';

/// Secure persistence contract for refresh tokens on mobile.
abstract class RefreshTokenStorage {
  Future<void> write({
    required String cloudBaseUrl,
    required String refreshToken,
  });

  Future<String?> read({required String cloudBaseUrl});

  Future<void> delete({required String cloudBaseUrl});

  Future<void> queueRevocation({
    required String cloudBaseUrl,
    required String refreshToken,
  });

  Future<PendingRefreshRevocation?> readQueuedRevocation();

  Future<void> deleteQueuedRevocation();
}

class PendingRefreshRevocation {
  const PendingRefreshRevocation(this.cloudBaseUrl, this.refreshToken);

  final String cloudBaseUrl;
  final String refreshToken;
}

/// Android Keystore / iOS Keychain-backed refresh token storage.
class SecureRefreshTokenStorage implements RefreshTokenStorage {
  SecureRefreshTokenStorage({FlutterSecureStorage? storage})
      : _storage = storage ?? const FlutterSecureStorage();

  static const String _refreshTokenKeyPrefix = 'kamori.refresh_token.';
  static const String _revocationKey = 'kamori.pending_revocation.v1';
  static const AndroidOptions _androidOptions = AndroidOptions();
  static const IOSOptions _iosOptions = IOSOptions(
    accessibility: KeychainAccessibility.first_unlock_this_device,
  );

  final FlutterSecureStorage _storage;

  @override
  Future<void> write({
    required String cloudBaseUrl,
    required String refreshToken,
  }) async {
    final normalizedToken = refreshToken.trim();
    if (normalizedToken.isEmpty) {
      throw ArgumentError('Refresh token is required');
    }
    await _storage.write(
      key: _storageKey(cloudBaseUrl),
      value: normalizedToken,
      aOptions: _androidOptions,
      iOptions: _iosOptions,
    );
  }

  @override
  Future<String?> read({required String cloudBaseUrl}) async {
    final token = await _storage.read(
      key: _storageKey(cloudBaseUrl),
      aOptions: _androidOptions,
      iOptions: _iosOptions,
    );
    final normalized = token?.trim();
    if (normalized == null || normalized.isEmpty) {
      return null;
    }
    return normalized;
  }

  @override
  Future<void> delete({required String cloudBaseUrl}) async {
    await _storage.delete(
      key: _storageKey(cloudBaseUrl),
      aOptions: _androidOptions,
      iOptions: _iosOptions,
    );
  }

  @override
  Future<void> queueRevocation({
    required String cloudBaseUrl,
    required String refreshToken,
  }) async {
    await _storage.write(
      key: _revocationKey,
      value: jsonEncode(<String, String>{
        'cloudBaseUrl': cloudBaseUrl,
        'refreshToken': refreshToken,
      }),
      aOptions: _androidOptions,
      iOptions: _iosOptions,
    );
  }

  @override
  Future<PendingRefreshRevocation?> readQueuedRevocation() async {
    final encoded = await _storage.read(
      key: _revocationKey,
      aOptions: _androidOptions,
      iOptions: _iosOptions,
    );
    if (encoded == null) return null;
    final value = jsonDecode(encoded) as Map<String, dynamic>;
    final baseUrl = value['cloudBaseUrl'] as String?;
    final token = value['refreshToken'] as String?;
    if (baseUrl == null || token == null || token.isEmpty) return null;
    return PendingRefreshRevocation(baseUrl, token);
  }

  @override
  Future<void> deleteQueuedRevocation() => _storage.delete(
        key: _revocationKey,
        aOptions: _androidOptions,
        iOptions: _iosOptions,
      );

  String _storageKey(
    String cloudBaseUrl, [
    String prefix = _refreshTokenKeyPrefix,
  ]) {
    final normalized = cloudBaseUrl.trim().toLowerCase();
    final digest = sha256.convert(utf8.encode(normalized)).toString();
    return '$prefix$digest';
  }
}
