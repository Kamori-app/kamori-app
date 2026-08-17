import 'dart:convert';
import 'dart:math';

import 'package:flutter_secure_storage/flutter_secure_storage.dart';

import 'package:dav_bridge_mobile/src/models/bridge_models.dart';

class MobileDeviceVault {
  const MobileDeviceVault({
    required this.cloudBaseUrl,
    required this.username,
    required this.accountMasterKey,
    required this.device,
  });

  final String cloudBaseUrl;
  final String username;
  final List<int> accountMasterKey;
  final DeviceSecrets device;

  Map<String, Object> toJson() => <String, Object>{
        'version': 1,
        'cloudBaseUrl': cloudBaseUrl,
        'username': username,
        'accountMasterKey': base64UrlEncode(accountMasterKey),
        'device': <String, Object>{
          'deviceId': device.deviceId,
          'signingPrivateKey': base64UrlEncode(device.signingPrivateKey),
          'hpkePrivateKey': base64UrlEncode(device.hpkePrivateKey),
          'hpkePublicKey': base64UrlEncode(device.hpkePublicKey),
        },
      };

  static MobileDeviceVault fromJson(Map<String, Object?> json) {
    if (json['version'] != 1 || json['device'] is! Map<String, Object?>) {
      throw const FormatException('invalid mobile device vault');
    }
    final rawDevice = json['device']! as Map<String, Object?>;
    List<int> decodeKey(Object? value, String field) {
      if (value is! String) {
        throw FormatException('invalid $field');
      }
      final bytes = base64Url.decode(value);
      if (bytes.length != 32) {
        throw FormatException('$field must be 32 bytes');
      }
      return bytes;
    }

    return MobileDeviceVault(
      cloudBaseUrl: json['cloudBaseUrl']! as String,
      username: json['username']! as String,
      accountMasterKey: decodeKey(
        json['accountMasterKey'],
        'account master key',
      ),
      device: DeviceSecrets(
        deviceId: rawDevice['deviceId']! as String,
        signingPrivateKey: decodeKey(
          rawDevice['signingPrivateKey'],
          'signing private key',
        ),
        hpkePrivateKey: decodeKey(
          rawDevice['hpkePrivateKey'],
          'HPKE private key',
        ),
        hpkePublicKey: decodeKey(
          rawDevice['hpkePublicKey'],
          'HPKE public key',
        ),
      ),
    );
  }
}

abstract class DeviceVaultStorage {
  Future<void> write(MobileDeviceVault vault);
  Future<MobileDeviceVault?> read();
}

class SecureDeviceVaultStorage implements DeviceVaultStorage {
  SecureDeviceVaultStorage({FlutterSecureStorage? storage})
      : _storage = storage ?? const FlutterSecureStorage();

  static const _storageKey = 'kamori.mobile.device_vault.v1';
  static const _androidOptions = AndroidOptions();
  static const _iosOptions = IOSOptions(
    accessibility: KeychainAccessibility.first_unlock_this_device,
  );
  final FlutterSecureStorage _storage;

  @override
  Future<void> write(MobileDeviceVault vault) => _storage.write(
        key: _storageKey,
        value: jsonEncode(vault.toJson()),
        aOptions: _androidOptions,
        iOptions: _iosOptions,
      );

  @override
  Future<MobileDeviceVault?> read() async {
    final value = await _storage.read(
      key: _storageKey,
      aOptions: _androidOptions,
      iOptions: _iosOptions,
    );
    if (value == null || value.isEmpty) {
      return null;
    }
    final decoded = jsonDecode(value);
    if (decoded is! Map<String, Object?>) {
      throw const FormatException('invalid mobile device vault');
    }
    return MobileDeviceVault.fromJson(decoded);
  }
}

class MobileSyncRuntimeSnapshot {
  const MobileSyncRuntimeSnapshot({
    required this.cloudBaseUrl,
    required this.sqlitePath,
    required this.accessToken,
    required this.collections,
  });

  final String cloudBaseUrl;
  final String sqlitePath;
  final String accessToken;
  final List<CollectionEntry> collections;

  Map<String, Object> toJson() => <String, Object>{
        'version': 1,
        'cloudBaseUrl': cloudBaseUrl,
        'sqlitePath': sqlitePath,
        'accessToken': accessToken,
        'collections': collections
            .map(
              (entry) => <String, Object>{
                'id': entry.id,
                'name': entry.name,
                'cmk': base64UrlEncode(entry.cmk),
                'keyEpoch': entry.keyEpoch,
                'role': entry.role,
              },
            )
            .toList(growable: false),
      };

  static MobileSyncRuntimeSnapshot fromJson(Map<String, Object?> json) {
    if (json['version'] != 1) {
      throw const FormatException('unsupported mobile runtime version');
    }
    final rawCollections = json['collections'];
    if (rawCollections is! List<Object?>) {
      throw const FormatException('invalid mobile runtime collections');
    }
    final collections = rawCollections.map((raw) {
      if (raw is! Map<String, Object?>) {
        throw const FormatException('invalid collection entry');
      }
      final key = base64Url.decode(raw['cmk']! as String);
      if (key.length != 32) {
        throw const FormatException('invalid collection key length');
      }
      return CollectionEntry(
        id: raw['id']! as String,
        name: raw['name']! as String,
        cmk: key,
        keyEpoch: (raw['keyEpoch'] as int?) ?? 1,
        role: (raw['role'] as String?) ?? 'owner',
      );
    }).toList(growable: false);
    return MobileSyncRuntimeSnapshot(
      cloudBaseUrl: json['cloudBaseUrl']! as String,
      sqlitePath: json['sqlitePath']! as String,
      accessToken: json['accessToken']! as String,
      collections: collections,
    );
  }
}

abstract class SyncRuntimeStorage {
  Future<void> write(MobileSyncRuntimeSnapshot snapshot);
  Future<MobileSyncRuntimeSnapshot?> read();
  Future<void> delete();
}

abstract class LocalCacheKeyStorage {
  Future<List<int>> readOrCreate();
}

class SecureLocalCacheKeyStorage implements LocalCacheKeyStorage {
  SecureLocalCacheKeyStorage({FlutterSecureStorage? storage})
      : _storage = storage ?? const FlutterSecureStorage();

  static const _storageKey = 'kamori.mobile.local_cache_key.v1';
  static const _androidOptions = AndroidOptions();
  static const _iosOptions = IOSOptions(
    accessibility: KeychainAccessibility.first_unlock_this_device,
  );

  final FlutterSecureStorage _storage;

  @override
  Future<List<int>> readOrCreate() async {
    final encoded = await _storage.read(
      key: _storageKey,
      aOptions: _androidOptions,
      iOptions: _iosOptions,
    );
    if (encoded != null && encoded.isNotEmpty) {
      final key = base64Url.decode(encoded);
      if (key.length != 32) {
        throw const FormatException('invalid local cache key length');
      }
      return key;
    }

    final random = Random.secure();
    final key = List<int>.generate(32, (_) => random.nextInt(256));
    await _storage.write(
      key: _storageKey,
      value: base64UrlEncode(key),
      aOptions: _androidOptions,
      iOptions: _iosOptions,
    );
    return key;
  }
}

class SecureSyncRuntimeStorage implements SyncRuntimeStorage {
  SecureSyncRuntimeStorage({FlutterSecureStorage? storage})
      : _storage = storage ?? const FlutterSecureStorage();

  static const _storageKey = 'kamori.mobile.sync_runtime.v1';
  static const _androidOptions = AndroidOptions();
  static const _iosOptions = IOSOptions(
    accessibility: KeychainAccessibility.first_unlock_this_device,
  );

  final FlutterSecureStorage _storage;

  @override
  Future<void> write(MobileSyncRuntimeSnapshot snapshot) => _storage.write(
        key: _storageKey,
        value: jsonEncode(snapshot.toJson()),
        aOptions: _androidOptions,
        iOptions: _iosOptions,
      );

  @override
  Future<MobileSyncRuntimeSnapshot?> read() async {
    final encoded = await _storage.read(
      key: _storageKey,
      aOptions: _androidOptions,
      iOptions: _iosOptions,
    );
    if (encoded == null || encoded.isEmpty) {
      return null;
    }
    final decoded = jsonDecode(encoded);
    if (decoded is! Map<String, Object?>) {
      throw const FormatException('invalid mobile runtime snapshot');
    }
    return MobileSyncRuntimeSnapshot.fromJson(decoded);
  }

  @override
  Future<void> delete() => _storage.delete(
        key: _storageKey,
        aOptions: _androidOptions,
        iOptions: _iosOptions,
      );
}
