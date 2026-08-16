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
}

/// Android Keystore / iOS Keychain-backed refresh token storage.
class SecureRefreshTokenStorage implements RefreshTokenStorage {
  SecureRefreshTokenStorage({FlutterSecureStorage? storage})
      : _storage = storage ?? const FlutterSecureStorage();

  static const String _refreshTokenKeyPrefix = 'kamori.refresh_token.';
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

  String _storageKey(String cloudBaseUrl) {
    final normalized = cloudBaseUrl.trim().toLowerCase();
    final digest = sha256.convert(utf8.encode(normalized)).toString();
    return '$_refreshTokenKeyPrefix$digest';
  }
}
