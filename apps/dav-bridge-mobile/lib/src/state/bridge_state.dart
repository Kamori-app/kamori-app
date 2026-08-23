import 'package:dav_bridge_mobile/src/models/bridge_models.dart';

const String kDefaultMobileSqlitePath = '.kamori/mobile-cache.sqlite3';
const String kDefaultMobileCloudBaseUrl = String.fromEnvironment(
  'KAMORI_CLOUD_BASE_URL',
  defaultValue: 'https://api.kamori.app',
);

class BridgeState {
  const BridgeState({
    required this.isBusy,
    required this.isAuthenticated,
    required this.cloudBaseUrl,
    required this.sqlitePath,
    required this.collections,
    required this.pimItems,
    this.accessToken,
    this.username,
    this.error,
    this.lastSyncAt,
    this.syncedItemsTotal = 0,
    this.backgroundSyncEnabled = true,
    this.calendarProjectionCollectionIds = const <String>{},
    this.contactsProjectionCollectionIds = const <String>{},
  });

  factory BridgeState.initial() {
    return const BridgeState(
      isBusy: false,
      isAuthenticated: false,
      cloudBaseUrl: kDefaultMobileCloudBaseUrl,
      sqlitePath: kDefaultMobileSqlitePath,
      collections: <CollectionEntry>[],
      pimItems: <PimItem>[],
    );
  }

  final bool isBusy;
  final bool isAuthenticated;
  final String? accessToken;
  final String? username;
  final String cloudBaseUrl;
  final String sqlitePath;
  final List<CollectionEntry> collections;
  final List<PimItem> pimItems;
  final String? error;
  final DateTime? lastSyncAt;
  final int syncedItemsTotal;
  final bool backgroundSyncEnabled;
  final Set<String> calendarProjectionCollectionIds;
  final Set<String> contactsProjectionCollectionIds;

  BridgeState copyWith({
    bool? isBusy,
    bool? isAuthenticated,
    String? accessToken,
    bool clearAccessToken = false,
    String? username,
    bool clearUsername = false,
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
    Set<String>? calendarProjectionCollectionIds,
    Set<String>? contactsProjectionCollectionIds,
  }) {
    return BridgeState(
      isBusy: isBusy ?? this.isBusy,
      isAuthenticated: isAuthenticated ?? this.isAuthenticated,
      accessToken: clearAccessToken ? null : (accessToken ?? this.accessToken),
      username: clearUsername ? null : (username ?? this.username),
      cloudBaseUrl: cloudBaseUrl ?? this.cloudBaseUrl,
      sqlitePath: sqlitePath ?? this.sqlitePath,
      collections: collections ?? this.collections,
      pimItems: pimItems ?? this.pimItems,
      error: clearError ? null : (error ?? this.error),
      lastSyncAt: clearLastSyncAt ? null : (lastSyncAt ?? this.lastSyncAt),
      syncedItemsTotal: syncedItemsTotal ?? this.syncedItemsTotal,
      backgroundSyncEnabled:
          backgroundSyncEnabled ?? this.backgroundSyncEnabled,
      calendarProjectionCollectionIds: Set.unmodifiable(
        calendarProjectionCollectionIds ?? this.calendarProjectionCollectionIds,
      ),
      contactsProjectionCollectionIds: Set.unmodifiable(
        contactsProjectionCollectionIds ?? this.contactsProjectionCollectionIds,
      ),
    );
  }
}
