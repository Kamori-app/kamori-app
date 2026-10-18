import 'dart:convert';

import 'package:crypto/crypto.dart';
import 'package:dav_bridge_mobile/src/models/bridge_models.dart';
import 'package:dav_bridge_mobile/src/rust/rust_bridge_api.dart';
import 'package:dav_bridge_mobile/src/rust/rust_bridge_frb.dart';
import 'package:dav_bridge_mobile/src/services/system_projection_service.dart';
import 'package:device_calendar_plus/device_calendar_plus.dart';
import 'package:flutter/services.dart';
import 'package:flutter_contacts/flutter_contacts.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

// Run only on a fresh dedicated simulator. The runner grants provider access
// explicitly before the projection pass; the first pass leaves it undetermined.
void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  const projectionEnabled = bool.fromEnvironment('KAMORI_TEST_PROJECTION');
  const storage = FlutterSecureStorage();
  const iosOptions = IOSOptions(
    accessibility: KeychainAccessibility.first_unlock_this_device,
  );
  final calendar = DeviceCalendar.instance;

  testWidgets('packaged real Rust bridge generates keys and owns a session',
      (tester) async {
    final bridge = RustBridgeFactory.create();
    expect(bridge, isA<FrbRustBridgeApi>(),
        reason: 'Integration builds must explicitly disable the mock bridge');
    final first = await bridge.generateDeviceSecrets();
    final second = await bridge.generateDeviceSecrets();
    expect(first.signingPrivateKey, hasLength(32));
    expect(first.hpkePublicKey, hasLength(32));
    expect(first.hpkePrivateKey, hasLength(32));
    expect(first.deviceId, isNotEmpty);
    expect(second.deviceId, isNot(first.deviceId));
    expect(second.hpkePublicKey, isNot(first.hpkePublicKey));
    final lease = await bridge.acquireSessionLease();
    try {
      await bridge.importRefreshToken(
        refreshToken: 'synthetic-native-smoke-token',
        rotationRequestId: '00000000-0000-4000-8000-000000000015',
      );
      expect(await bridge.exportRefreshToken(), 'synthetic-native-smoke-token');
      expect(await bridge.exportRefreshRotationRequestId(),
          '00000000-0000-4000-8000-000000000015');
      await bridge.clearRefreshToken();
      expect(await bridge.exportRefreshToken(), isNull);
    } finally {
      lease.release();
    }
    final next = await bridge.acquireSessionLease();
    next.release();
  });

  testWidgets('installed background admission channel returns actual state',
      (tester) async {
    const channel = MethodChannel('kamori/background-sync');
    final admitted = await channel.invokeMethod<bool>('isScheduled');
    // No task was submitted in this fresh test app. Registration alone must
    // never be reported as scheduler admission.
    expect(admitted, isFalse);
    await expectLater(channel.invokeMethod<void>('unknown-test-method'),
        throwsA(isA<MissingPluginException>()));
  });

  testWidgets('disabled projection leaves native permissions unchanged',
      (tester) async {
    final contactBefore =
        await FlutterContacts.permissions.check(PermissionType.readWrite);
    final calendarBefore = await calendar.hasPermissions();
    final fixture = _ProjectionFixture(storage, iosOptions);
    try {
      final settings = await fixture.service.readSettings();
      expect(settings.calendarCollectionIds, isEmpty);
      expect(settings.contactsCollectionIds, isEmpty);
      await fixture.service
          .projectEnabled([fixture.contact(), fixture.event()]);
      expect(await FlutterContacts.permissions.check(PermissionType.readWrite),
          contactBefore);
      expect(await calendar.hasPermissions(), calendarBefore);
      if (!projectionEnabled) {
        expect(contactBefore, PermissionStatus.notDetermined);
        expect(calendarBefore, CalendarPermissionStatus.notDetermined);
      }
    } finally {
      await fixture.cleanup();
    }
  });

  testWidgets('opt-in production Contacts projection creates updates deletes',
      (tester) async {
    expect(await FlutterContacts.permissions.check(PermissionType.readWrite),
        PermissionStatus.granted);
    final fixture = _ProjectionFixture(storage, iosOptions);
    try {
      await fixture.service
          .enableContacts(fixture.collection, [fixture.contact()]);
      final id = await fixture.mappedId('kamori.projection.contacts.items');
      expect(id, isNotEmpty);
      final created = await FlutterContacts.get(id,
          properties: {ContactProperty.name, ContactProperty.email});
      expect(created?.name?.first, 'Synthetic');
      expect(created?.emails.single.address, 'native-test@example.invalid');
      expect((await fixture.service.readSettings()).contactsCollectionIds,
          contains(fixture.collection));
      await fixture.service.projectEnabled([fixture.contact(updated: true)]);
      expect(await fixture.mappedId('kamori.projection.contacts.items'), id);
      final updated = await FlutterContacts.get(id,
          properties: {ContactProperty.name, ContactProperty.email});
      expect(updated?.name?.first, 'Updated');
      expect(updated?.emails.single.address, 'updated@example.invalid');
      await fixture.service
          .disableContacts(fixture.collection, removeProjectedData: true);
      expect(
        await FlutterContacts.getAll(filter: ContactFilter.ids([id])),
        isEmpty,
      );
      expect((await fixture.service.readSettings()).contactsCollectionIds,
          isEmpty);
    } finally {
      await fixture.cleanup();
    }
  }, skip: !projectionEnabled);

  testWidgets('contact reconciliation repairs externally deleted projections',
      (tester) async {
    final fixture = _ProjectionFixture(storage, iosOptions);
    try {
      await fixture.service
          .enableContacts(fixture.collection, [fixture.contact()]);
      final original =
          await fixture.mappedId('kamori.projection.contacts.items');
      await FlutterContacts.delete(original);
      await fixture.service.projectEnabled([fixture.contact(updated: true)]);
      final replacement =
          await fixture.mappedId('kamori.projection.contacts.items');
      expect(replacement, isNot(original));
      expect(
          (await FlutterContacts.get(replacement,
                  properties: {ContactProperty.name}))
              ?.name
              ?.first,
          'Updated');
      await FlutterContacts.delete(replacement);
      await fixture.service
          .disableContacts(fixture.collection, removeProjectedData: true);
      expect((await fixture.service.readSettings()).contactsCollectionIds,
          isEmpty);
    } finally {
      await fixture.cleanup();
    }
  }, skip: !projectionEnabled);

  testWidgets('opt-in production Calendar projection creates updates deletes',
      (tester) async {
    expect(await calendar.hasPermissions(), CalendarPermissionStatus.granted);
    final fixture = _ProjectionFixture(storage, iosOptions);
    try {
      await fixture.service
          .enableCalendar(fixture.collection, [fixture.event()]);
      final id = await fixture.mappedId('kamori.projection.calendar.events');
      final created = await calendar.getEvent(id);
      expect(created?.title, 'Synthetic native event');
      expect(created?.location, 'Synthetic room');
      expect(created?.startDate.toUtc(), DateTime.utc(2030, 1, 2, 12));
      expect((await fixture.service.readSettings()).calendarCollectionIds,
          contains(fixture.collection));
      await fixture.service.projectEnabled([fixture.event(updated: true)]);
      expect(await fixture.mappedId('kamori.projection.calendar.events'), id);
      final updated = await calendar.getEvent(id);
      expect(updated?.title, 'Updated native event');
      expect(updated?.location, 'Updated room');
      await fixture.service
          .disableCalendar(fixture.collection, removeProjectedData: true);
      expect(await calendar.getEvent(id), isNull);
      expect((await fixture.service.readSettings()).calendarCollectionIds,
          isEmpty);
    } finally {
      await fixture.cleanup();
    }
  }, skip: !projectionEnabled);
}

class _ProjectionFixture {
  _ProjectionFixture(this.storage, this.options) {
    service.configureAccount(cloudBaseUrl: url, username: username);
  }

  static const url = 'https://native-test.example.invalid';
  final username = 'synthetic-${DateTime.now().microsecondsSinceEpoch}';
  final String collection = 'native-test-collection';
  final FlutterSecureStorage storage;
  final IOSOptions options;
  late final service = NativeSystemProjectionService(storage: storage);
  String get scope => sha256.convert(utf8.encode('$url\n$username')).toString();
  String scoped(String key) => '$key.$scope';

  Future<String> mappedId(String key) async {
    final value = await storage.read(key: scoped(key), iOptions: options);
    final mapping = jsonDecode(value!) as Map<String, dynamic>;
    return mapping['$collection/native-test-projection'] as String;
  }

  PimItem contact({bool updated = false}) => PimItem(
        spaceId: collection,
        resourceId: 'native-test-resource',
        projectionId: 'native-test-projection',
        headOperationId: 'native-test-operation',
        kind: PimItemKind.contact,
        title: 'Synthetic contact',
        givenName: updated ? 'Updated' : 'Synthetic',
        familyName: username,
        emails: [
          PimLabeledValue(
            value: updated
                ? 'updated@example.invalid'
                : 'native-test@example.invalid',
            label: 'work',
          )
        ],
        notes: 'Must stay encrypted; iOS notes entitlement is not enabled.',
        completed: false,
      );

  PimItem event({bool updated = false}) => PimItem(
        spaceId: collection,
        resourceId: 'native-test-resource',
        projectionId: 'native-test-projection',
        headOperationId: 'native-test-operation',
        kind: PimItemKind.calendarEvent,
        title: updated ? 'Updated native event' : 'Synthetic native event',
        startsAt: PimTemporal.utc(DateTime.utc(2030, 1, 2, 12)),
        endsAt: PimTemporal.utc(DateTime.utc(2030, 1, 2, 13)),
        location: updated ? 'Updated room' : 'Synthetic room',
        reminderMinutes: 10,
        completed: false,
      );

  Future<void> cleanup() async {
    // Unique account scope: these maps contain only IDs created by this fixture.
    final contacts = await storage.read(
        key: scoped('kamori.projection.contacts.items'), iOptions: options);
    if (contacts != null) {
      final ids = (jsonDecode(contacts) as Map<String, dynamic>)
          .values
          .cast<String>()
          .toList();
      if (ids.isNotEmpty) {
        for (final contact
            in await FlutterContacts.getAll(filter: ContactFilter.ids(ids))) {
          await FlutterContacts.delete(contact.id!);
        }
      }
    }
    final events = await storage.read(
        key: scoped('kamori.projection.calendar.events'), iOptions: options);
    if (events != null) {
      for (final id in (jsonDecode(events) as Map<String, dynamic>)
          .values
          .cast<String>()) {
        if (await DeviceCalendar.instance.getEvent(id) != null) {
          await DeviceCalendar.instance.deleteEvent(eventId: id);
        }
      }
    }
    final calendarKey = scoped('kamori.projection.calendar.id');
    final calendarId = await storage.read(key: calendarKey, iOptions: options);
    if (calendarId != null) {
      await DeviceCalendar.instance.deleteCalendar(calendarId);
    }
    for (final key in [
      'kamori.projection.calendar.collections.v2',
      'kamori.projection.contacts.collections.v2',
      'kamori.projection.calendar.id',
      'kamori.projection.calendar.events',
      'kamori.projection.contacts.items',
    ]) {
      await storage.delete(key: scoped(key), iOptions: options);
    }
  }
}
