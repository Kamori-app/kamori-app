import 'dart:convert';

import 'package:crypto/crypto.dart';
import 'package:device_calendar_plus/device_calendar_plus.dart';
import 'package:flutter_contacts/flutter_contacts.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';

import 'package:dav_bridge_mobile/src/models/bridge_models.dart';

class SystemProjectionSettings {
  const SystemProjectionSettings({
    required this.calendarCollectionIds,
    required this.contactsCollectionIds,
  });

  final Set<String> calendarCollectionIds;
  final Set<String> contactsCollectionIds;
}

abstract class SystemProjectionService {
  void configureAccount({
    required String cloudBaseUrl,
    required String username,
  });

  Future<SystemProjectionSettings> readSettings();

  Future<void> enableCalendar(String collectionId, List<PimItem> items);

  Future<void> enableContacts(String collectionId, List<PimItem> items);

  Future<void> disableCalendar(
    String collectionId, {
    required bool removeProjectedData,
  });

  Future<void> disableContacts(
    String collectionId, {
    required bool removeProjectedData,
  });

  Future<void> projectEnabled(List<PimItem> items);
}

/// Opt-in projection from Kamori's encrypted local state into OS data stores.
///
/// The system stores contain plaintext by design. They are never treated as the
/// sync source of truth and no permission prompt is shown until an explicit
/// enable action reaches this service.
class NativeSystemProjectionService implements SystemProjectionService {
  NativeSystemProjectionService({FlutterSecureStorage? storage})
      : _storage = storage ?? const FlutterSecureStorage();

  static const _calendarCollectionsKey =
      'kamori.projection.calendar.collections.v2';
  static const _contactsCollectionsKey =
      'kamori.projection.contacts.collections.v2';
  static const _calendarIdKey = 'kamori.projection.calendar.id';
  static const _eventMapKey = 'kamori.projection.calendar.events';
  static const _contactMapKey = 'kamori.projection.contacts.items';
  static const _descriptionPrefix = 'Kamori encrypted projection';
  static const _androidOptions = AndroidOptions();
  static const _iosOptions = IOSOptions(
    accessibility: KeychainAccessibility.first_unlock_this_device,
  );

  final FlutterSecureStorage _storage;
  final DeviceCalendar _calendar = DeviceCalendar.instance;
  String? _accountScope;

  @override
  void configureAccount({
    required String cloudBaseUrl,
    required String username,
  }) {
    _accountScope = sha256
        .convert(utf8.encode('${cloudBaseUrl.trim()}\n${username.trim()}'))
        .toString();
  }

  @override
  Future<SystemProjectionSettings> readSettings() async {
    return SystemProjectionSettings(
      calendarCollectionIds: await _readSet(_calendarCollectionsKey),
      contactsCollectionIds: await _readSet(_contactsCollectionsKey),
    );
  }

  @override
  Future<void> enableCalendar(
    String collectionId,
    List<PimItem> items,
  ) async {
    final status = await _calendar.requestPermissions(
      level: CalendarAccessLevel.full,
    );
    if (status != CalendarPermissionStatus.granted) {
      throw StateError('Calendar permission was not granted.');
    }
    final enabled = await _readSet(_calendarCollectionsKey);
    enabled.add(collectionId);
    await _projectCalendar(items, enabled);
    await _writeSet(_calendarCollectionsKey, enabled);
  }

  @override
  Future<void> enableContacts(
    String collectionId,
    List<PimItem> items,
  ) async {
    final status = await FlutterContacts.permissions.request(
      PermissionType.readWrite,
    );
    if (status != PermissionStatus.granted &&
        status != PermissionStatus.limited) {
      throw StateError('Contacts permission was not granted.');
    }
    final enabled = await _readSet(_contactsCollectionsKey);
    enabled.add(collectionId);
    await _projectContacts(items, enabled);
    await _writeSet(_contactsCollectionsKey, enabled);
  }

  @override
  Future<void> disableCalendar(
    String collectionId, {
    required bool removeProjectedData,
  }) async {
    final mapping = await _readMap(_eventMapKey);
    if (removeProjectedData) {
      final keys = mapping.keys
          .where((key) => key.startsWith('$collectionId/'))
          .toList(growable: false);
      for (final key in keys) {
        final id = mapping[key];
        if (id != null && await _calendar.getEvent(id) != null) {
          await _calendar.deleteEvent(eventId: id);
        }
        mapping.remove(key);
        await _writeMap(_eventMapKey, mapping);
      }
    }
    final enabled = await _readSet(_calendarCollectionsKey);
    enabled.remove(collectionId);
    await _writeSet(_calendarCollectionsKey, enabled);
  }

  @override
  Future<void> disableContacts(
    String collectionId, {
    required bool removeProjectedData,
  }) async {
    final mapping = await _readMap(_contactMapKey);
    if (removeProjectedData) {
      final keys = mapping.keys
          .where((key) => key.startsWith('$collectionId/'))
          .toList(growable: false);
      for (final key in keys) {
        final id = mapping[key];
        if (id != null && await FlutterContacts.get(id) != null) {
          await FlutterContacts.delete(id);
        }
        mapping.remove(key);
        await _writeMap(_contactMapKey, mapping);
      }
    }
    final enabled = await _readSet(_contactsCollectionsKey);
    enabled.remove(collectionId);
    await _writeSet(_contactsCollectionsKey, enabled);
  }

  @override
  Future<void> projectEnabled(List<PimItem> items) async {
    final settings = await readSettings();
    if (settings.calendarCollectionIds.isNotEmpty) {
      await _projectCalendar(items, settings.calendarCollectionIds);
    }
    if (settings.contactsCollectionIds.isNotEmpty) {
      await _projectContacts(items, settings.contactsCollectionIds);
    }
  }

  Future<void> _projectCalendar(
    List<PimItem> items,
    Set<String> enabledCollectionIds,
  ) async {
    final calendarId = await _getOrCreateCalendarId();
    final mapping = await _readMap(_eventMapKey);
    final events = items
        .where(
          (item) =>
              item.kind == PimItemKind.calendarEvent &&
              !item.conflict &&
              enabledCollectionIds.contains(item.spaceId),
        )
        .toList(growable: false);
    final activeKeys = events.map(_itemKey).toSet();

    final managedKeys = mapping.keys
        .where(
          (key) => enabledCollectionIds.any(
            (collectionId) => key.startsWith('$collectionId/'),
          ),
        )
        .toSet();
    for (final staleKey in managedKeys.difference(activeKeys)) {
      final eventId = mapping[staleKey];
      if (eventId != null && await _calendar.getEvent(eventId) != null) {
        await _calendar.deleteEvent(eventId: eventId);
      }
      mapping.remove(staleKey);
      await _writeMap(_eventMapKey, mapping);
    }

    for (final item in events) {
      final start = _parseCompactUtc(item.startsAt, field: 'start');
      final end = _parseCompactUtc(item.endsAt, field: 'end');
      final key = _itemKey(item);
      final existingId = mapping[key];
      if (existingId == null) {
        mapping[key] = await _calendar.createEvent(
          calendarId: calendarId,
          title: item.title,
          startDate: start,
          endDate: end,
          description: _descriptionPrefix,
          timeZone: 'UTC',
        );
        await _writeMap(_eventMapKey, mapping);
      } else {
        final existing = await _calendar.getEvent(existingId);
        if (existing != null) {
          await _calendar.updateEvent(
            eventId: existingId,
            title: item.title,
            startDate: start,
            endDate: end,
            description: Patch.set(_descriptionPrefix),
            timeZone: 'UTC',
          );
        } else {
          mapping[key] = await _calendar.createEvent(
            calendarId: calendarId,
            title: item.title,
            startDate: start,
            endDate: end,
            description: _descriptionPrefix,
            timeZone: 'UTC',
          );
          await _writeMap(_eventMapKey, mapping);
        }
      }
    }
    await _writeMap(_eventMapKey, mapping);
  }

  Future<void> _projectContacts(
    List<PimItem> items,
    Set<String> enabledCollectionIds,
  ) async {
    final mapping = await _readMap(_contactMapKey);
    final contacts = items
        .where(
          (item) =>
              item.kind == PimItemKind.contact &&
              !item.conflict &&
              enabledCollectionIds.contains(item.spaceId),
        )
        .toList(growable: false);
    final activeKeys = contacts.map(_itemKey).toSet();

    final managedKeys = mapping.keys
        .where(
          (key) => enabledCollectionIds.any(
            (collectionId) => key.startsWith('$collectionId/'),
          ),
        )
        .toSet();
    for (final staleKey in managedKeys.difference(activeKeys)) {
      final contactId = mapping[staleKey];
      if (contactId != null && await FlutterContacts.get(contactId) != null) {
        await FlutterContacts.delete(contactId);
      }
      mapping.remove(staleKey);
      await _writeMap(_contactMapKey, mapping);
    }

    for (final item in contacts) {
      final key = _itemKey(item);
      final existingId = mapping[key];
      final phones = item.phone?.trim().isNotEmpty == true
          ? <Phone>[Phone(number: item.phone!.trim())]
          : const <Phone>[];
      final emails = item.email?.trim().isNotEmpty == true
          ? <Email>[Email(address: item.email!.trim())]
          : const <Email>[];
      if (existingId == null) {
        mapping[key] = await FlutterContacts.create(
          Contact(
            name: Name(first: item.title),
            phones: phones,
            emails: emails,
          ),
        );
        await _writeMap(_contactMapKey, mapping);
        continue;
      }

      final existing = await FlutterContacts.get(
        existingId,
        properties: const {
          ContactProperty.name,
          ContactProperty.phone,
          ContactProperty.email,
        },
      );
      if (existing == null) {
        mapping[key] = await FlutterContacts.create(
          Contact(
            name: Name(first: item.title),
            phones: phones,
            emails: emails,
          ),
        );
        await _writeMap(_contactMapKey, mapping);
      } else {
        await FlutterContacts.update(
          existing.copyWith(
            name: Name(first: item.title),
            phones: phones,
            emails: emails,
          ),
        );
      }
    }
    await _writeMap(_contactMapKey, mapping);
  }

  Future<String> _getOrCreateCalendarId() async {
    final stored = await _read(_calendarIdKey);
    final calendars = await _calendar.listCalendars();
    if (stored != null && calendars.any((entry) => entry.id == stored)) {
      return stored;
    }
    for (final entry in calendars) {
      if (entry.name == _calendarName && !entry.readOnly) {
        await _write(_calendarIdKey, entry.id);
        return entry.id;
      }
    }
    final created = await _calendar.createCalendar(
      name: _calendarName,
      colorHex: '#0A7F73',
    );
    await _write(_calendarIdKey, created);
    return created;
  }

  String get _calendarName {
    final scope = _accountScope;
    if (scope == null) {
      throw StateError('System projection account is not configured.');
    }
    return 'Kamori ${scope.substring(0, 8).toUpperCase()}';
  }

  DateTime _parseCompactUtc(String? value, {required String field}) {
    final normalized = value?.trim() ?? '';
    final match = RegExp(
      r'^(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})Z$',
    ).firstMatch(normalized);
    if (match == null) {
      throw FormatException('Invalid calendar $field timestamp.');
    }
    final parsed = DateTime.utc(
      int.parse(match.group(1)!),
      int.parse(match.group(2)!),
      int.parse(match.group(3)!),
      int.parse(match.group(4)!),
      int.parse(match.group(5)!),
      int.parse(match.group(6)!),
    );
    if (parsed.year != int.parse(match.group(1)!) ||
        parsed.month != int.parse(match.group(2)!) ||
        parsed.day != int.parse(match.group(3)!) ||
        parsed.hour != int.parse(match.group(4)!) ||
        parsed.minute != int.parse(match.group(5)!) ||
        parsed.second != int.parse(match.group(6)!)) {
      throw FormatException('Invalid calendar $field timestamp.');
    }
    return parsed;
  }

  String _itemKey(PimItem item) => '${item.spaceId}/${item.projectionId}';

  Future<Set<String>> _readSet(String key) async {
    final encoded = await _read(key);
    if (encoded == null || encoded.isEmpty) return <String>{};
    final decoded = jsonDecode(encoded);
    if (decoded is! List) return <String>{};
    return decoded
        .whereType<String>()
        .where((value) => value.isNotEmpty)
        .toSet();
  }

  Future<void> _writeSet(String key, Set<String> value) =>
      _write(key, jsonEncode(value.toList()..sort()));

  Future<Map<String, String>> _readMap(String key) async {
    final encoded = await _read(key);
    if (encoded == null || encoded.isEmpty) {
      return <String, String>{};
    }
    final decoded = jsonDecode(encoded) as Map<String, dynamic>;
    return decoded.map((mapKey, value) => MapEntry(mapKey, value as String));
  }

  Future<void> _writeMap(String key, Map<String, String> value) =>
      _write(key, jsonEncode(value));

  String _scopedKey(String key) {
    final scope = _accountScope;
    if (scope == null) {
      throw StateError('System projection account is not configured.');
    }
    return '$key.$scope';
  }

  Future<String?> _read(String key) => _storage.read(
        key: _scopedKey(key),
        aOptions: _androidOptions,
        iOptions: _iosOptions,
      );

  Future<void> _write(String key, String value) => _storage.write(
        key: _scopedKey(key),
        value: value,
        aOptions: _androidOptions,
        iOptions: _iosOptions,
      );
}
