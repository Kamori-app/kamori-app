import 'dart:convert';

import 'package:crypto/crypto.dart';
import 'package:device_calendar_plus/device_calendar_plus.dart';
import 'package:flutter_contacts/flutter_contacts.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';

import 'package:dav_bridge_mobile/src/models/bridge_models.dart';

class SystemProjectionSettings {
  const SystemProjectionSettings({
    required this.calendarEnabled,
    required this.contactsEnabled,
  });

  final bool calendarEnabled;
  final bool contactsEnabled;
}

abstract class SystemProjectionService {
  void configureAccount({
    required String cloudBaseUrl,
    required String username,
  });

  Future<SystemProjectionSettings> readSettings();

  Future<void> enableCalendar(List<PimItem> items);

  Future<void> enableContacts(List<PimItem> items);

  Future<void> disableCalendar({required bool removeProjectedData});

  Future<void> disableContacts({required bool removeProjectedData});

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

  static const _calendarEnabledKey = 'kamori.projection.calendar.enabled';
  static const _contactsEnabledKey = 'kamori.projection.contacts.enabled';
  static const _calendarIdKey = 'kamori.projection.calendar.id';
  static const _eventMapKey = 'kamori.projection.calendar.events';
  static const _contactMapKey = 'kamori.projection.contacts.items';
  static const _calendarName = 'Kamori';
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
      calendarEnabled: await _readBool(_calendarEnabledKey),
      contactsEnabled: await _readBool(_contactsEnabledKey),
    );
  }

  @override
  Future<void> enableCalendar(List<PimItem> items) async {
    final status = await _calendar.requestPermissions(
      level: CalendarAccessLevel.full,
    );
    if (status != CalendarPermissionStatus.granted) {
      throw StateError('Calendar permission was not granted.');
    }
    await _writeBool(_calendarEnabledKey, true);
    await _projectCalendar(items);
  }

  @override
  Future<void> enableContacts(List<PimItem> items) async {
    final status = await FlutterContacts.permissions.request(
      PermissionType.readWrite,
    );
    if (status != PermissionStatus.granted &&
        status != PermissionStatus.limited) {
      throw StateError('Contacts permission was not granted.');
    }
    await _writeBool(_contactsEnabledKey, true);
    await _projectContacts(items);
  }

  @override
  Future<void> disableCalendar({required bool removeProjectedData}) async {
    if (removeProjectedData) {
      final ids = (await _readMap(_eventMapKey)).values.toSet();
      for (final id in ids) {
        await _ignoreMissing(() => _calendar.deleteEvent(eventId: id));
      }
      await _writeMap(_eventMapKey, const <String, String>{});
    }
    await _writeBool(_calendarEnabledKey, false);
  }

  @override
  Future<void> disableContacts({required bool removeProjectedData}) async {
    if (removeProjectedData) {
      final ids = (await _readMap(_contactMapKey)).values.toSet();
      for (final id in ids) {
        await _ignoreMissing(() => FlutterContacts.delete(id));
      }
      await _writeMap(_contactMapKey, const <String, String>{});
    }
    await _writeBool(_contactsEnabledKey, false);
  }

  @override
  Future<void> projectEnabled(List<PimItem> items) async {
    final settings = await readSettings();
    if (settings.calendarEnabled) {
      await _projectCalendar(items);
    }
    if (settings.contactsEnabled) {
      await _projectContacts(items);
    }
  }

  Future<void> _projectCalendar(List<PimItem> items) async {
    final calendarId = await _getOrCreateCalendarId();
    final mapping = await _readMap(_eventMapKey);
    final events = items
        .where((item) => item.kind == PimItemKind.calendarEvent)
        .toList(growable: false);
    final activeKeys = events.map(_itemKey).toSet();

    for (final staleKey in mapping.keys.toSet().difference(activeKeys)) {
      final eventId = mapping.remove(staleKey);
      if (eventId != null) {
        await _ignoreMissing(() => _calendar.deleteEvent(eventId: eventId));
      }
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
          description: '$_descriptionPrefix\n$key',
          timeZone: 'UTC',
        );
      } else {
        try {
          await _calendar.updateEvent(
            eventId: existingId,
            title: item.title,
            startDate: start,
            endDate: end,
            description: Patch.set('$_descriptionPrefix\n$key'),
            timeZone: 'UTC',
          );
        } catch (_) {
          mapping[key] = await _calendar.createEvent(
            calendarId: calendarId,
            title: item.title,
            startDate: start,
            endDate: end,
            description: '$_descriptionPrefix\n$key',
            timeZone: 'UTC',
          );
        }
      }
    }
    await _writeMap(_eventMapKey, mapping);
  }

  Future<void> _projectContacts(List<PimItem> items) async {
    final mapping = await _readMap(_contactMapKey);
    final contacts = items
        .where((item) => item.kind == PimItemKind.contact)
        .toList(growable: false);
    final activeKeys = contacts.map(_itemKey).toSet();

    for (final staleKey in mapping.keys.toSet().difference(activeKeys)) {
      final contactId = mapping.remove(staleKey);
      if (contactId != null) {
        await _ignoreMissing(() => FlutterContacts.delete(contactId));
      }
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

  DateTime _parseCompactUtc(String? value, {required String field}) {
    final normalized = value?.trim() ?? '';
    final match = RegExp(
      r'^(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})Z$',
    ).firstMatch(normalized);
    if (match == null) {
      throw FormatException('Invalid calendar $field timestamp.');
    }
    return DateTime.utc(
      int.parse(match.group(1)!),
      int.parse(match.group(2)!),
      int.parse(match.group(3)!),
      int.parse(match.group(4)!),
      int.parse(match.group(5)!),
      int.parse(match.group(6)!),
    );
  }

  String _itemKey(PimItem item) => '${item.spaceId}/${item.resourceId}';

  Future<void> _ignoreMissing(Future<void> Function() operation) async {
    try {
      await operation();
    } catch (_) {
      // The user may have removed a projection directly in the system app.
    }
  }

  Future<bool> _readBool(String key) async => (await _read(key)) == 'true';

  Future<void> _writeBool(String key, bool value) =>
      _write(key, value.toString());

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
