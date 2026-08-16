import 'package:dav_bridge_mobile/src/models/bridge_models.dart';

const String kDefaultMobileSqlitePath = '.kamori/mobile-cache.sqlite3';

class BridgeState {
  const BridgeState({
    required this.isBusy,
    required this.isAuthenticated,
    required this.cloudBaseUrl,
    required this.sqlitePath,
    required this.collections,
    required this.pimItems,
    this.accessToken,
    this.error,
    this.lastSyncAt,
    this.syncedItemsTotal = 0,
    this.backgroundSyncEnabled = true,
    this.calendarProjectionEnabled = false,
    this.contactsProjectionEnabled = false,
  });

  factory BridgeState.initial() {
    return const BridgeState(
      isBusy: false,
      isAuthenticated: false,
      cloudBaseUrl: 'http://127.0.0.1:3000',
      sqlitePath: kDefaultMobileSqlitePath,
      collections: <CollectionEntry>[],
      pimItems: <PimItem>[],
    );
  }

  final bool isBusy;
  final bool isAuthenticated;
  final String? accessToken;
  final String cloudBaseUrl;
  final String sqlitePath;
  final List<CollectionEntry> collections;
  final List<PimItem> pimItems;
  final String? error;
  final DateTime? lastSyncAt;
  final int syncedItemsTotal;
  final bool backgroundSyncEnabled;
  final bool calendarProjectionEnabled;
  final bool contactsProjectionEnabled;

  BridgeState copyWith({
    bool? isBusy,
    bool? isAuthenticated,
    String? accessToken,
    bool clearAccessToken = false,
    String? cloudBaseUrl,
    String? sqlitePath,
    List<CollectionEntry>? collections,
    List<PimItem>? pimItems,
    String? error,
    bool clearError = false,
    DateTime? lastSyncAt,
    bool clearLastSyncAt = false,
    int? syncedItemsTotal,
    bool? backgroundSyncEnabled,
    bool? calendarProjectionEnabled,
    bool? contactsProjectionEnabled,
  }) {
    return BridgeState(
      isBusy: isBusy ?? this.isBusy,
      isAuthenticated: isAuthenticated ?? this.isAuthenticated,
      accessToken: clearAccessToken ? null : (accessToken ?? this.accessToken),
      cloudBaseUrl: cloudBaseUrl ?? this.cloudBaseUrl,
      sqlitePath: sqlitePath ?? this.sqlitePath,
      collections: collections ?? this.collections,
      pimItems: pimItems ?? this.pimItems,
      error: clearError ? null : (error ?? this.error),
      lastSyncAt: clearLastSyncAt ? null : (lastSyncAt ?? this.lastSyncAt),
      syncedItemsTotal: syncedItemsTotal ?? this.syncedItemsTotal,
      backgroundSyncEnabled:
          backgroundSyncEnabled ?? this.backgroundSyncEnabled,
      calendarProjectionEnabled:
          calendarProjectionEnabled ?? this.calendarProjectionEnabled,
      contactsProjectionEnabled:
          contactsProjectionEnabled ?? this.contactsProjectionEnabled,
    );
  }
}
