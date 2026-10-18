import 'dart:convert';

import 'package:crypto/crypto.dart';
import 'package:device_calendar_plus_android/device_calendar_plus_android.dart';
import 'package:flutter/services.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:dav_bridge_mobile/src/models/bridge_models.dart';
import 'package:dav_bridge_mobile/src/services/system_projection_service.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  const channel = MethodChannel('device_calendar_plus_android');
  const contactsChannel = MethodChannel('flutter_contacts');
  late List<MethodCall> contactCalls;
  late Map<String, dynamic>? projectedContact;
  const base = 'https://synthetic.example.invalid';
  final scope = sha256.convert(utf8.encode('$base\nalice')).toString();
  final mapKey = 'kamori.projection.calendar.events.$scope';
  final settingsKey = 'kamori.projection.calendar.collections.v2.$scope';
  const storage = FlutterSecureStorage();
  late NativeSystemProjectionService service;
  late Map<String, Map<String, dynamic>> provider;
  late List<MethodCall> calls;
  late bool deleteFails;
  late bool readFails;
  PimItem event(String metadata) => PimItem(
        spaceId: 'enabled',
        resourceId: 'resource',
        projectionId: 'imported.ics',
        headOperationId: 'head',
        kind: PimItemKind.calendarEvent,
        title: 'Imported',
        completed: false,
        startsAt: const PimTemporal(kind: 'utc', utc: '20261023T120000Z'),
        endsAt: const PimTemporal(kind: 'utc', utc: '20261023T130000Z'),
        recurrenceRule: 'FREQ=WEEKLY',
        recurrenceExdates: metadata == 'exdates'
            ? const [PimTemporal(kind: 'utc', utc: '20261030T120000Z')]
            : const [],
        recurrenceRdates: metadata == 'rdates'
            ? const [PimTemporal(kind: 'utc', utc: '20261029T120000Z')]
            : const [],
        recurrenceOverrides: metadata == 'overrides'
            ? const [
                PimRecurrenceOverride(
                    recurrenceId:
                        PimTemporal(kind: 'utc', utc: '20261030T120000Z'),
                    cancelled: true)
              ]
            : const [],
        recurrenceUnsupported:
            metadata == 'unsupported' ? const ['RDATE;VALUE=PERIOD'] : const [],
      );
  Map<String, dynamic> nativeEvent(String id) => {
        'eventId': id,
        'instanceId': id,
        'calendarId': 'calendar',
        'title': 'Old projection',
        'description': 'Kamori encrypted projection\n\nOld notes',
        'startDate': DateTime.utc(2026, 10, 23, 12).millisecondsSinceEpoch,
        'endDate': DateTime.utc(2026, 10, 23, 13).millisecondsSinceEpoch,
        'isAllDay': false,
        'availability': 'busy',
        'status': 'confirmed',
        'isRecurring': true,
        'recurrenceRule': 'FREQ=WEEKLY',
      };
  Future<Map<String, dynamic>> mapping() async =>
      jsonDecode((await storage.read(key: mapKey)) ?? '{}')
          as Map<String, dynamic>;
  setUp(() {
    DeviceCalendarPlusAndroid.registerWith();
    calls = [];
    contactCalls = [];
    projectedContact = null;
    provider = {};
    deleteFails = false;
    readFails = false;
    FlutterSecureStorage.setMockInitialValues({
      'kamori.projection.calendar.id.$scope': 'calendar',
      'kamori.projection.contacts.items.$scope':
          '{"contacts/sentinel":"untouched"}',
    });
    service = NativeSystemProjectionService()
      ..configureAccount(cloudBaseUrl: base, username: 'alice');
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, (call) async {
      calls.add(call);
      final args = call.arguments == null
          ? <String, dynamic>{}
          : Map<String, dynamic>.from(call.arguments as Map);
      switch (call.method) {
        case 'requestPermissions':
          return 'granted';
        case 'listCalendars':
          return [
            {'id': 'calendar', 'name': 'Existing Kamori', 'readOnly': false}
          ];
        case 'getEvent':
          if (readFails) {
            throw PlatformException(
                code: 'provider-unavailable',
                message: 'synthetic read failure');
          }
          return provider[args['eventId']];
        case 'createEvent':
          final id = 'created-${provider.length}';
          provider[id] = {...nativeEvent(id), ...args};
          return id;
        case 'updateEvent':
          provider[args['eventId']]!.addAll(Map.fromEntries(
              args.entries.where((entry) => entry.value != null)));
          return null;
        case 'deleteEvent':
          if (deleteFails && args['eventId'] == 'owned') {
            throw PlatformException(
                code: 'provider-unavailable',
                message: 'synthetic deletion failure');
          }
          provider.remove(args['eventId']);
          return null;
        default:
          throw StateError('Unexpected calendar provider call ${call.method}');
      }
    });
  });
  setUp(() {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(contactsChannel, (call) async {
      contactCalls.add(call);
      final args = Map<String, dynamic>.from(call.arguments as Map);
      switch (call.method) {
        case 'crud.create':
          projectedContact = {
            ...Map<String, dynamic>.from(args['contact'] as Map),
            'id': 'contact-owned'
          };
          return 'contact-owned';
        case 'crud.getAll':
          return projectedContact == null ? [] : [projectedContact];
        case 'crud.update':
          projectedContact = Map<String, dynamic>.from(args['contact'] as Map);
          return null;
        default:
          throw StateError('Unexpected contact call ${call.method}');
      }
    });
  });
  tearDown(() => TestDefaultBinaryMessengerBinding
      .instance.defaultBinaryMessenger
      .setMockMethodCallHandler(contactsChannel, null));
  tearDown(() => TestDefaultBinaryMessengerBinding
      .instance.defaultBinaryMessenger
      .setMockMethodCallHandler(channel, null));

  for (final metadata in ['exdates', 'rdates', 'overrides', 'unsupported']) {
    test(
        'fresh $metadata projection refuses before permissions or native writes',
        () async {
      await expectLater(service.enableCalendar('enabled', [event(metadata)]),
          throwsA(isA<FormatException>()));
      expect(calls, isEmpty);
      expect(provider, isEmpty);
      expect((await service.readSettings()).calendarCollectionIds, isEmpty);
      expect(await mapping(), isEmpty);
    });
  }
  Future<void> existing() async {
    await storage.write(key: settingsKey, value: '["enabled"]');
    await storage.write(
        key: mapKey,
        value:
            '{"enabled/imported.ics":"owned","other/sentinel.ics":"unrelated"}');
    provider['owned'] = nativeEvent('owned');
    provider['unrelated'] = nativeEvent('unrelated');
  }

  test('existing wrong master is removed and mapping persisted before refusal',
      () async {
    await existing();
    await expectLater(service.projectEnabled([event('exdates')]),
        throwsA(isA<FormatException>()));
    expect(provider.keys, ['unrelated']);
    expect(await mapping(), {'other/sentinel.ics': 'unrelated'});
    expect(calls.map((call) => call.method), ['getEvent', 'deleteEvent']);
    expect((await service.readSettings()).calendarCollectionIds, {'enabled'});
    expect(await storage.read(key: 'kamori.projection.contacts.items.$scope'),
        '{"contacts/sentinel":"untouched"}');
    // The persisted deletion survives a new service instance and causes no retry writes.
    calls.clear();
    final restarted = NativeSystemProjectionService()
      ..configureAccount(cloudBaseUrl: base, username: 'alice');
    await expectLater(restarted.projectEnabled([event('exdates')]),
        throwsA(isA<FormatException>()));
    expect(calls, isEmpty);
  });
  test('provider deletion errors retain mapping and original OS copy for retry',
      () async {
    await existing();
    deleteFails = true;
    await expectLater(service.projectEnabled([event('overrides')]),
        throwsA(isA<PlatformException>()));
    expect(provider.containsKey('owned'), isTrue);
    expect((await mapping())['enabled/imported.ics'], 'owned');
    expect(calls.map((call) => call.method), ['getEvent', 'deleteEvent']);
    deleteFails = false;
    await expectLater(service.projectEnabled([event('overrides')]),
        throwsA(isA<FormatException>()));
    expect(provider.containsKey('owned'), isFalse);
    expect((await mapping()).containsKey('enabled/imported.ics'), isFalse);
  });
  test('provider read failure preserves mapping and every OS entry', () async {
    await existing();
    readFails = true;
    await expectLater(service.projectEnabled([event('exdates')]),
        throwsA(isA<PlatformException>()));
    expect(provider.keys.toSet(), {'owned', 'unrelated'});
    expect((await mapping())['enabled/imported.ics'], 'owned');
    expect(calls.map((call) => call.method), ['getEvent']);
  });
  test('storage failure after deletion retries missing event mapping removal',
      () async {
    await existing();
    final failing = _FailingMappingStorage(mapKey);
    final withFailure = NativeSystemProjectionService(storage: failing)
      ..configureAccount(cloudBaseUrl: base, username: 'alice');
    await expectLater(withFailure.projectEnabled([event('exdates')]),
        throwsA(isA<PlatformException>()));
    expect(provider.keys, ['unrelated']);
    expect((await mapping())['enabled/imported.ics'], 'owned');
    calls.clear();
    // Fresh service reads the retained durable mapping and observes absence.
    await expectLater(service.projectEnabled([event('exdates')]),
        throwsA(isA<FormatException>()));
    expect(await mapping(), {'other/sentinel.ics': 'unrelated'});
    expect(calls.map((call) => call.method), ['getEvent']);
  });
  test('mapped foreign provider entry is never deleted', () async {
    await existing();
    provider['owned']!['calendarId'] = 'foreign-calendar';
    await expectLater(
        service.projectEnabled([event('rdates')]), throwsA(isA<StateError>()));
    expect(provider.containsKey('owned'), isTrue);
    expect((await mapping())['enabled/imported.ics'], 'owned');
    expect(calls.map((call) => call.method), ['getEvent']);
  });
  for (final failCleanup in [false, true]) {
    test(
        'mixed reconciliation continues supported update/delete and contacts with cleanup failure=$failCleanup',
        () async {
      await existing();
      await storage.write(key: settingsKey, value: '["enabled","supported"]');
      await storage.write(
          key: 'kamori.projection.contacts.collections.v2.$scope',
          value: '["contacts"]');
      await storage.write(
          key: 'kamori.projection.contacts.items.$scope', value: '{}');
      await storage.write(
          key: mapKey,
          value: jsonEncode({
            'enabled/imported.ics': 'owned',
            'supported/current.ics': 'supported',
            'supported/stale.ics': 'stale',
          }));
      provider['supported'] = nativeEvent('supported');
      provider['stale'] = nativeEvent('stale');
      const supported = PimItem(
        spaceId: 'supported',
        resourceId: 'supported',
        projectionId: 'current.ics',
        headOperationId: 'head',
        kind: PimItemKind.calendarEvent,
        title: 'Updated supported event',
        completed: false,
        startsAt: PimTemporal(kind: 'utc', utc: '20261023T120000Z'),
        endsAt: PimTemporal(kind: 'utc', utc: '20261023T130000Z'),
        recurrenceRule: 'FREQ=WEEKLY',
      );
      const contact = PimItem(
          spaceId: 'contacts',
          resourceId: 'contact',
          projectionId: 'contact.vcf',
          headOperationId: 'head',
          kind: PimItemKind.contact,
          title: 'Updated contact',
          completed: false);
      deleteFails = failCleanup;
      await expectLater(
          service.projectEnabled([event('exdates'), supported, contact]),
          throwsA(failCleanup
              ? isA<PlatformException>()
                  .having((error) => error.code, 'code', 'provider-unavailable')
              : isA<FormatException>()));
      expect(provider['supported']!['title'], 'Updated supported event');
      expect(provider.containsKey('stale'), isFalse);
      expect(contactCalls.map((call) => call.method), contains('crud.create'));
      expect(projectedContact!['name']['first'], 'Updated contact');
      expect(
          jsonDecode((await storage.read(
              key: 'kamori.projection.contacts.items.$scope'))!),
          {'contacts/contact.vcf': 'contact-owned'});
      expect((await mapping()).containsKey('supported/stale.ics'), isFalse);
      expect(
          (await mapping()).containsKey('enabled/imported.ics'), failCleanup);
      expect(provider.containsKey('owned'), failCleanup);
      expect(calls.any((call) => call.method == 'requestPermissions'), isFalse);
      deleteFails = false;
      calls.clear();
      contactCalls.clear();
      await expectLater(
          service.projectEnabled([event('exdates'), supported, contact]),
          throwsA(isA<FormatException>()));
      expect((await mapping()).containsKey('enabled/imported.ics'), isFalse);
      expect(provider.containsKey('owned'), isFalse);
      expect(calls.any((call) => call.method == 'updateEvent'), isTrue);
      expect(contactCalls.map((call) => call.method), contains('crud.update'));
    });
  }
  test(
      'ordinary supported weekly series still projects through real provider API',
      () async {
    await storage.write(key: settingsKey, value: '["enabled"]');
    await service.projectEnabled([event('none')]);
    expect(provider.values.single['recurrenceRule'], contains('FREQ=WEEKLY'));
    expect((await mapping())['enabled/imported.ics'], provider.keys.single);
    expect(calls.any((call) => call.method == 'requestPermissions'), isFalse);
  });
}

class _FailingMappingStorage extends FlutterSecureStorage {
  _FailingMappingStorage(this.mappingKey);
  final String mappingKey;
  @override
  Future<void> write(
      {required String key,
      required String? value,
      AppleOptions? iOptions,
      AndroidOptions? aOptions,
      LinuxOptions? lOptions,
      WebOptions? webOptions,
      AppleOptions? mOptions,
      WindowsOptions? wOptions}) async {
    if (key == mappingKey) throw PlatformException(code: 'storage-unavailable');
    await super.write(
        key: key,
        value: value,
        iOptions: iOptions,
        aOptions: aOptions,
        lOptions: lOptions,
        webOptions: webOptions,
        mOptions: mOptions,
        wOptions: wOptions);
  }
}
