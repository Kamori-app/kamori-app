import 'package:flutter_test/flutter_test.dart';

import 'package:dav_bridge_mobile/src/state/bridge_state.dart';

void main() {
  test('initial state has expected defaults', () {
    final state = BridgeState.initial();

    expect(state.isBusy, isFalse);
    expect(state.isAuthenticated, isFalse);
    expect(state.cloudBaseUrl, 'http://127.0.0.1:3000');
    expect(state.sqlitePath, kDefaultMobileSqlitePath);
    expect(state.collections, isEmpty);
  });

  test('copyWith clears optional fields when clear flags are set', () {
    const state = BridgeState(
      isBusy: false,
      isAuthenticated: true,
      accessToken: 'session',
      cloudBaseUrl: 'http://localhost:3000',
      sqlitePath: 'cache.sqlite3',
      collections: [],
      pimItems: [],
      error: 'oops',
      lastSyncAt: null,
      syncedItemsTotal: 10,
      backgroundSyncEnabled: true,
    );

    final updated = state.copyWith(
      clearAccessToken: true,
      clearError: true,
    );

    expect(updated.accessToken, isNull);
    expect(updated.error, isNull);
  });
}
