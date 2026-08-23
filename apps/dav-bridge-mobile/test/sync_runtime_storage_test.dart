import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';

import 'package:dav_bridge_mobile/src/models/bridge_models.dart';
import 'package:dav_bridge_mobile/src/services/sync_runtime_storage.dart';

void main() {
  test('mobile sync snapshot round-trips collection keys', () {
    final snapshot = MobileSyncRuntimeSnapshot(
      cloudBaseUrl: 'https://api.kamori.app',
      username: 'alice',
      sqlitePath: '.kamori/mobile-cache.sqlite3',
      accessToken: 'access-token',
      backgroundSyncEnabled: false,
      collections: <CollectionEntry>[
        CollectionEntry(
          id: 'space-1',
          name: 'Personal',
          cmk: List<int>.generate(32, (index) => index),
          historyStartSeq: 3,
          currentStateStartSeq: 8,
        ),
      ],
    );

    final decoded = jsonDecode(jsonEncode(snapshot.toJson()));
    final restored = MobileSyncRuntimeSnapshot.fromJson(
      Map<String, Object?>.from(decoded as Map),
    );

    expect(restored.cloudBaseUrl, snapshot.cloudBaseUrl);
    expect(restored.username, 'alice');
    expect(restored.sqlitePath, snapshot.sqlitePath);
    expect(restored.accessToken, snapshot.accessToken);
    expect(restored.backgroundSyncEnabled, isFalse);
    expect(restored.collections.single.id, 'space-1');
    expect(restored.collections.single.cmk, snapshot.collections.single.cmk);
    expect(restored.collections.single.historyStartSeq, 3);
    expect(restored.collections.single.currentStateStartSeq, 8);
  });

  test('mobile sync snapshot rejects invalid key length', () {
    final payload = <String, Object?>{
      'version': 1,
      'cloudBaseUrl': 'https://api.kamori.app',
      'sqlitePath': '.kamori/mobile-cache.sqlite3',
      'accessToken': 'access-token',
      'collections': <Object?>[
        <String, Object?>{
          'id': 'space-1',
          'name': 'Personal',
          'cmk': base64UrlEncode(<int>[1, 2, 3]),
        },
      ],
    };

    expect(
      () => MobileSyncRuntimeSnapshot.fromJson(payload),
      throwsFormatException,
    );
  });
}
