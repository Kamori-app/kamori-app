import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:device_calendar_plus/device_calendar_plus.dart' hide Event;
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
    for (final item in items.where(
      (item) =>
          item.spaceId == collectionId &&
          item.kind == PimItemKind.calendarEvent &&
          !item.conflict &&
          item.recurrenceRule?.isNotEmpty == true,
    )) {
      _parseSystemRecurrence(item);
    }
    final enabled = await _readSet(_calendarCollectionsKey);
    enabled.add(collectionId);
    await _writeSet(_calendarCollectionsKey, enabled);
    await _projectCalendar(items, {collectionId});
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
    await _writeSet(_contactsCollectionsKey, enabled);
    await _projectContacts(items, {collectionId});
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
    for (final item in events) {
      final rule = item.recurrenceRule;
      if (rule == null || rule.isEmpty) continue;
      _parseSystemRecurrence(item);
    }
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
      final start = item.startsAt?.toDateTime();
      final explicitEnd = item.endsAt?.toDateTime();
      final isAllDay = item.startsAt?.isAllDay ?? false;
      final end = explicitEnd ??
          (isAllDay
              ? start?.add(const Duration(days: 1))
              : start?.add(const Duration(milliseconds: 1)));
      if (start == null || end == null || end.isBefore(start)) {
        continue;
      }
      final timezone = isAllDay ? null : item.startsAt?.timezone ?? 'UTC';
      final description = [
        _descriptionPrefix,
        if (item.notes?.trim().isNotEmpty == true) item.notes!.trim(),
      ].join('\n\n');
      RecurrenceRule? recurrenceRule;
      if (item.recurrenceRule?.isNotEmpty == true) {
        recurrenceRule = _parseSystemRecurrence(item);
      }
      final reminders = item.reminderMinutes == null
          ? null
          : [Duration(minutes: item.reminderMinutes!)];
      final key = _itemKey(item);
      final existingId = mapping[key];
      if (existingId == null) {
        mapping[key] = await _calendar.createEvent(
          calendarId: calendarId,
          title: item.title,
          startDate: start,
          endDate: end,
          isAllDay: isAllDay,
          description: description,
          location: item.location,
          timeZone: timezone,
          recurrenceRule: recurrenceRule,
          reminders: reminders,
        );
        await _writeMap(_eventMapKey, mapping);
      } else {
        final existing = await _calendar.getEvent(existingId);
        if (existing != null) {
          final desiredRecurrence = item.recurrenceRule;
          final storedRecurrence = existing.recurrenceRule?.rruleString;
          if (_normalizeRrule(desiredRecurrence) !=
              _normalizeRrule(storedRecurrence)) {
            await _calendar.deleteEvent(eventId: existingId);
            mapping[key] = await _calendar.createEvent(
              calendarId: calendarId,
              title: item.title,
              startDate: start,
              endDate: end,
              isAllDay: isAllDay,
              description: description,
              location: item.location,
              timeZone: timezone,
              recurrenceRule: recurrenceRule,
              reminders: reminders,
            );
            await _writeMap(_eventMapKey, mapping);
          } else {
            await _calendar.updateEvent(
              eventId: existingId,
              title: item.title,
              startDate: start,
              endDate: end,
              isAllDay: isAllDay,
              description: Patch.set(description),
              location: item.location == null
                  ? const Patch.clear()
                  : Patch.set(item.location!),
              timeZone: timezone,
              reminders: reminders == null
                  ? const Patch.clear()
                  : Patch.set(reminders),
            );
          }
        } else {
          mapping[key] = await _calendar.createEvent(
            calendarId: calendarId,
            title: item.title,
            startDate: start,
            endDate: end,
            isAllDay: isAllDay,
            description: description,
            location: item.location,
            timeZone: timezone,
            recurrenceRule: recurrenceRule,
            reminders: reminders,
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
      final phones = item.phones
          .where((value) => value.value.trim().isNotEmpty)
          .map((value) => Phone(
                number: value.value.trim(),
                label: _phoneLabel(value.label),
              ))
          .toList(growable: false);
      final emails = item.emails
          .where((value) => value.value.trim().isNotEmpty)
          .map((value) => Email(
                address: value.value.trim(),
                label: _emailLabel(value.label),
              ))
          .toList(growable: false);
      final addresses = item.addresses
          .map((value) => Address(
                street: value.street,
                city: value.locality,
                state: value.region,
                postalCode: value.postalCode,
                country: value.country,
                label: _addressLabel(value.label),
              ))
          .toList(growable: false);
      final organizations = item.organization?.trim().isNotEmpty == true ||
              item.jobTitle?.trim().isNotEmpty == true
          ? <Organization>[
              Organization(name: item.organization, jobTitle: item.jobTitle),
            ]
          : const <Organization>[];
      final websites = item.url?.trim().isNotEmpty == true
          ? <Website>[Website(url: item.url!.trim())]
          : const <Website>[];
      // iOS contact notes require a separately approved entitlement. Keep the
      // encrypted note in Kamori and project it only where the OS permits it.
      final notes = !Platform.isIOS && item.notes?.trim().isNotEmpty == true
          ? <Note>[Note(note: item.notes!.trim())]
          : const <Note>[];
      final birthday = DateTime.tryParse(item.birthday ?? '');
      final contactEvents = birthday == null
          ? const <Event>[]
          : <Event>[
              Event(
                year: birthday.year,
                month: birthday.month,
                day: birthday.day,
                label: const Label(EventLabel.birthday),
              ),
            ];
      final name = Name(
        first: item.givenName.isEmpty ? item.title : item.givenName,
        middle: item.middleName.isEmpty ? null : item.middleName,
        last: item.familyName.isEmpty ? null : item.familyName,
        prefix: item.namePrefix.isEmpty ? null : item.namePrefix,
        suffix: item.nameSuffix.isEmpty ? null : item.nameSuffix,
      );
      if (existingId == null) {
        mapping[key] = await FlutterContacts.create(
          Contact(
            name: name,
            phones: phones,
            emails: emails,
            addresses: addresses,
            organizations: organizations,
            websites: websites,
            events: contactEvents,
            notes: notes,
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
          ContactProperty.address,
          ContactProperty.organization,
          ContactProperty.website,
          ContactProperty.event,
          ContactProperty.note,
        },
      );
      if (existing == null) {
        mapping[key] = await FlutterContacts.create(
          Contact(
            name: name,
            phones: phones,
            emails: emails,
            addresses: addresses,
            organizations: organizations,
            websites: websites,
            events: contactEvents,
            notes: notes,
          ),
        );
        await _writeMap(_contactMapKey, mapping);
      } else {
        await FlutterContacts.update(
          existing.copyWith(
            name: name,
            phones: phones,
            emails: emails,
            addresses: addresses,
            organizations: organizations,
            websites: websites,
            events: contactEvents,
            notes: notes,
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

  String _itemKey(PimItem item) => '${item.spaceId}/${item.projectionId}';

  RecurrenceRule? _parseSystemRecurrence(PimItem item) {
    final value = item.recurrenceRule;
    if (value == null || value.isEmpty) return null;
    final parsed = RecurrenceRule.fromRruleString(value);
    if (parsed == null) {
      throw FormatException(
        'Cannot project “${item.title}”: the recurrence rule is not supported by this device.',
      );
    }
    return parsed;
  }

  String? _normalizeRrule(String? value) {
    if (value == null) return null;
    final trimmed = value.trim();
    return trimmed.toUpperCase().startsWith('RRULE:')
        ? trimmed.substring(6)
        : trimmed;
  }

  Label<EmailLabel> _emailLabel(String value) => switch (value.toLowerCase()) {
        'home' => const Label(EmailLabel.home),
        'work' => const Label(EmailLabel.work),
        'mobile' => const Label(EmailLabel.mobile),
        'school' => const Label(EmailLabel.school),
        'other' || '' => const Label(EmailLabel.other),
        _ => Label(EmailLabel.custom, value),
      };

  Label<PhoneLabel> _phoneLabel(String value) => switch (value.toLowerCase()) {
        'home' => const Label(PhoneLabel.home),
        'work' => const Label(PhoneLabel.work),
        'mobile' || 'cell' => const Label(PhoneLabel.mobile),
        'fax' => const Label(PhoneLabel.workFax),
        'other' || '' => const Label(PhoneLabel.other),
        _ => Label(PhoneLabel.custom, value),
      };

  Label<AddressLabel> _addressLabel(String value) =>
      switch (value.toLowerCase()) {
        'home' => const Label(AddressLabel.home),
        'work' => const Label(AddressLabel.work),
        'other' || '' => const Label(AddressLabel.other),
        _ => Label(AddressLabel.custom, value),
      };

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
