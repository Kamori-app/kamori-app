import 'dart:convert';
import 'dart:io';

import 'package:dav_bridge_mobile/src/models/bridge_models.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:dav_bridge_mobile/src/i18n/app_localizations.dart';
import 'package:dav_bridge_mobile/src/state/bridge_controller.dart';
import 'package:dav_bridge_mobile/src/state/bridge_state.dart';
import 'package:dav_bridge_mobile/src/ui/pim_screen.dart';

void main() {
  final corpus = jsonDecode(
    File('../../tests/fixtures/pim/corpus.json').readAsStringSync(),
  ) as List<dynamic>;
  for (final entry in corpus) {
    final fixture = entry as Map<String, dynamic>;
    final fields = fixture['fields'] as Map<String, dynamic>;
    if (fixture['kind'] == 'contact') continue;
    // Rust tests validate these shared expected values against the actual ICS.
    // Mobile receives typed fields from FRB, so this adapter only builds its
    // existing model; it does not parse or reimplement iCalendar.
    PimTemporal? recordTemporal(Map<String, dynamic> value,
            [String prefix = '']) =>
        value['${prefix}kind'] == null
            ? null
            : PimTemporal(
                kind: value['${prefix}kind'] as String,
                date: value['${prefix}date'] as String?,
                utc: value['${prefix}utc'] as String?,
                local: value['${prefix}local'] as String?,
                timezone: value['${prefix}timezone'] as String?,
              );
    PimTemporal? temporal(String key) => fields[key] == null
        ? null
        : recordTemporal(fields[key]['value'] as Map<String, dynamic>);
    List<Map<String, dynamic>> records(String key) =>
        (fields[key]?['value'] as List<dynamic>? ?? [])
            .cast<Map<String, dynamic>>();

    final item = PimItem(
      spaceId: 'synthetic-space',
      resourceId: fixture['uid'] as String,
      projectionId: fixture['file'] as String,
      headOperationId: 'synthetic-head',
      kind: fixture['kind'] == 'task'
          ? PimItemKind.task
          : PimItemKind.calendarEvent,
      title: fields['title']['value'] as String,
      completed: false,
      notes: fields['notes']['value'] as String,
      location: fields['location']?['value'] as String?,
      startsAt: temporal('starts_at'),
      endsAt: temporal('ends_at'),
      dueAt: temporal('due_at'),
      recurrenceRule: fields['recurrence_rule']['value'] as String,
      recurrenceUnsupported:
          (fields['recurrence_unsupported']?['value'] as List<dynamic>? ?? [])
              .cast<String>(),
      recurrenceExdates: records('recurrence_exdates')
          .map((value) => recordTemporal(value)!)
          .toList(),
      recurrenceRdates: records('recurrence_rdates')
          .map((value) => recordTemporal(value)!)
          .toList(),
      recurrenceOverrides: records('recurrence_overrides')
          .map((value) => PimRecurrenceOverride(
                recurrenceId: recordTemporal(value, 'recurrence_id_')!,
                startsAt: recordTemporal(value, 'starts_at_'),
                endsAt: recordTemporal(value, 'ends_at_'),
                title: value['title'] as String?,
                notes: value['notes'] as String?,
                location: value['location'] as String?,
                cancelled: value['cancelled'] == 'true',
              ))
          .toList(),
      priority: fields['priority']?['value'] as int? ?? 0,
      reminderMinutes: fields['reminder_minutes']?['value'] as int?,
      categories: (fields['categories']?['value'] as List<dynamic>? ?? [])
          .cast<String>(),
    );
    test('${fixture['file']}: imported values survive opening the draft', () {
      final draft = PimDraft.fromItem(item);
      expect(draft.recurrenceRule, item.recurrenceRule);
      expect(draft.priority, item.priority);
      expect(draft.reminderMinutes, item.reminderMinutes);
      expect(draft.startsAt, same(item.startsAt));
      expect(draft.endsAt, same(item.endsAt));
      expect(draft.dueAt, same(item.dueAt));
      expect(draft.categories, item.categories);
      expect(draft.title, item.title);
      expect(draft.notes, item.notes);
    });
    if (fixture['file'] == 'zoned-exceptions.ics') {
      testWidgets(
          'mounted mobile calendar renders override and edits the source series',
          (tester) async {
        tester.view.physicalSize = const Size(900, 1200);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.resetPhysicalSize);
        addTearDown(tester.view.resetDevicePixelRatio);
        final controller = _CorpusController(item);
        await tester.pumpWidget(ProviderScope(
          overrides: [bridgeControllerProvider.overrideWith(() => controller)],
          child: const MaterialApp(
            localizationsDelegates: [AppLocalizations.delegate],
            home: PimScreen(initialKind: PimItemKind.calendarEvent),
          ),
        ));
        await tester.pumpAndSettle();
        tester
            .widget<CalendarDatePicker>(find.byType(CalendarDatePicker))
            .onDateChanged(DateTime.parse('2026-11-08T01:30:00Z').toLocal());
        await tester.pumpAndSettle();
        expect(find.text('Detached override'), findsOneWidget);
        expect(find.textContaining('Override room'), findsOneWidget);
        expect(find.textContaining('Leave this exception untouched'),
            findsOneWidget);
        await tester.tap(find.text('Detached override'));
        await tester.pumpAndSettle();
        expect(find.text('Edit series'), findsOneWidget);
        expect(find.text('Imported zoned master'), findsOneWidget);
        await tester.ensureVisible(find.text('Save'));
        await tester.tap(find.text('Save'));
        await tester.pumpAndSettle();
        expect(controller.saved?.resourceId, item.resourceId);
        expect(controller.saved?.projectionId, item.projectionId);
        expect(controller.saved?.title, item.title);
        expect(controller.saved?.startsAt?.local, item.startsAt?.local);
      });
    }
    if (item.kind != PimItemKind.calendarEvent) continue;
    test(
        '${fixture['file']}: recurrence set applies exceptions and retains time policy',
        () {
      for (final sample in fixture['display'] as List<dynamic>) {
        final day = sample['instant'] != null
            ? DateTime.parse(sample['instant'] as String).toLocal()
            : DateTime.parse(sample['day'] as String);
        expect(pimIntervalOccursOnDate(item, day), sample['occurs'],
            reason: sample.toString());
        final occurrences = pimOccurrencesOnDate(item, day);
        expect(occurrences, hasLength(sample['occurs'] == true ? 1 : 0));
        if (sample['title'] != null) {
          expect(occurrences.single.title ?? item.title, sample['title']);
        }
        if (sample['start'] != null) {
          expect(occurrences.single.startsAt.toUtc(),
              DateTime.parse(sample['start'] as String));
        }
        if (sample['end'] != null) {
          expect(occurrences.single.endsAt.toUtc(),
              DateTime.parse(sample['end'] as String));
        }
        if (sample['start_day'] != null) {
          expect(occurrences.single.startsAt,
              DateTime.parse(sample['start_day'] as String));
        }
        if (sample['end_day'] != null) {
          expect(occurrences.single.endsAt,
              DateTime.parse(sample['end_day'] as String));
        }
        if (sample['location'] != null) {
          expect(
              occurrences.single.location ?? item.location, sample['location']);
        }
        if (sample['notes'] != null) {
          expect(occurrences.single.notes ?? item.notes, sample['notes']);
        }
      }
    });
  }

  test('rich PIM draft preserves all editable fields', () {
    final item = PimItem(
      spaceId: 'space-1',
      resourceId: 'resource-1',
      projectionId: 'resource-1.vcf',
      headOperationId: 'operation-1',
      kind: PimItemKind.contact,
      title: 'Dr Alice Example Jr',
      completed: false,
      notes: 'Encrypted note',
      categories: const ['Friends', 'Work'],
      namePrefix: 'Dr',
      givenName: 'Alice',
      middleName: 'B',
      familyName: 'Example',
      nameSuffix: 'Jr',
      emails: const [
        PimLabeledValue(label: 'work', value: 'alice@example.com'),
        PimLabeledValue(label: 'personal', value: 'a@example.net'),
      ],
      phones: const [
        PimLabeledValue(label: 'mobile', value: '+995555123456'),
      ],
      addresses: const [
        PimPostalAddress(
          label: 'home',
          street: '1 Rustaveli Ave',
          locality: 'Tbilisi',
          postalCode: '0108',
          country: 'Georgia',
        ),
      ],
      organization: 'Kamori',
      jobTitle: 'Engineer',
      birthday: '1990-08-28',
      url: 'https://kamori.app',
      favorite: true,
    );

    final draft = PimDraft.fromItem(item);

    expect(draft.namePrefix, 'Dr');
    expect(draft.middleName, 'B');
    expect(draft.nameSuffix, 'Jr');
    expect(draft.emails, hasLength(2));
    expect(draft.addresses.single.locality, 'Tbilisi');
    expect(draft.categories, ['Friends', 'Work']);
    expect(draft.favorite, isTrue);
  });

  test('typed temporal values preserve all-day and UTC semantics', () {
    final allDay = PimTemporal.allDay(DateTime(2026, 8, 28));
    final instant = PimTemporal.utc(DateTime.utc(2026, 8, 28, 14, 30));

    expect(allDay.kind, 'date');
    expect(allDay.toDateTime(), DateTime(2026, 8, 28));
    expect(instant.kind, 'utc');
    expect(instant.toDateTime(), DateTime.utc(2026, 8, 28, 14, 30));
  });

  test('calendar filtering treats an all-day end as exclusive', () {
    const item = PimItem(
      spaceId: 'space-1',
      resourceId: 'resource-1',
      projectionId: 'resource-1.ics',
      headOperationId: 'operation-1',
      kind: PimItemKind.calendarEvent,
      title: 'Release day',
      completed: false,
      startsAt: PimTemporal(kind: 'date', date: '2026-08-28'),
      endsAt: PimTemporal(kind: 'date', date: '2026-08-29'),
    );

    expect(pimIntervalOccursOnDate(item, DateTime(2026, 8, 28)), isTrue);
    expect(pimIntervalOccursOnDate(item, DateTime(2026, 8, 29)), isFalse);
  });

  test('zoned temporal keeps wall time and derives the matching instant', () {
    final temporal = PimTemporal.zoned(
      DateTime(2026, 8, 28, 18, 30),
      'Asia/Tbilisi',
    );

    expect(temporal.local, '20260828T183000');
    expect(temporal.utc, '20260828T143000Z');
    expect(temporal.toEditorDateTime(), DateTime(2026, 8, 28, 18, 30));
  });

  test('calendar filtering expands weekly recurrence and missing DTEND', () {
    const recurring = PimItem(
      spaceId: 'space-1',
      resourceId: 'resource-1',
      projectionId: 'resource-1.ics',
      headOperationId: 'operation-1',
      kind: PimItemKind.calendarEvent,
      title: 'Weekly review',
      completed: false,
      startsAt: PimTemporal(kind: 'date', date: '2026-08-28'),
      endsAt: PimTemporal(kind: 'date', date: '2026-08-29'),
      recurrenceRule: 'FREQ=WEEKLY',
    );
    const withoutEnd = PimItem(
      spaceId: 'space-1',
      resourceId: 'resource-2',
      projectionId: 'resource-2.ics',
      headOperationId: 'operation-2',
      kind: PimItemKind.calendarEvent,
      title: 'Open-ended event',
      completed: false,
      startsAt: PimTemporal(kind: 'date', date: '2026-08-28'),
    );

    expect(pimIntervalOccursOnDate(recurring, DateTime(2026, 9, 4)), isTrue);
    expect(pimIntervalOccursOnDate(recurring, DateTime(2026, 9, 5)), isFalse);
    expect(pimIntervalOccursOnDate(withoutEnd, DateTime(2026, 8, 28)), isTrue);
  });

  PimItem weekly(PimTemporal start, PimTemporal end) => PimItem(
        spaceId: 'space-1',
        resourceId: 'weekly',
        projectionId: 'weekly.ics',
        headOperationId: 'op',
        kind: PimItemKind.calendarEvent,
        title: 'Weekly',
        completed: false,
        startsAt: start,
        endsAt: end,
        recurrenceRule: 'FREQ=WEEKLY',
      );

  test('UTC weekly recurrence anchors UTC fields in every device timezone', () {
    final item = weekly(
      const PimTemporal(kind: 'utc', utc: '20260828T180000Z'),
      const PimTemporal(kind: 'utc', utc: '20260828T183000Z'),
    );
    expect(pimIntervalOccursOnDate(item, DateTime(2026, 9, 4)), isTrue);
    expect(pimIntervalOccursOnDate(item, DateTime(2026, 9, 5)), isFalse);
    expect(pimIntervalOccursOnDate(item, DateTime(2026, 9, 3)), isFalse);
  });

  test('zoned weekly recurrence retains DST wall time and actual instant', () {
    final item = weekly(
      PimTemporal.zoned(DateTime(2026, 10, 23, 19, 30), 'America/New_York'),
      PimTemporal.zoned(DateTime(2026, 10, 23, 19, 45), 'America/New_York'),
    );
    // The viewer's calendar day follows the actual instant, not floating fields.
    final before = DateTime.utc(2026, 10, 30, 23, 30).toLocal();
    final after = DateTime.utc(2026, 11, 7, 0, 30).toLocal();
    expect(pimIntervalOccursOnDate(item, before), isTrue);
    expect(pimIntervalOccursOnDate(item, after), isTrue);
    expect(pimIntervalOccursOnDate(item, after.add(const Duration(days: 1))),
        isFalse);
  });
}

// Only the application shell/backend is substituted. Production occurrence
// calculation, widgets, editor initialization and save-draft creation run intact.
class _CorpusController extends BridgeController {
  _CorpusController(this.item);
  final PimItem item;
  PimDraft? saved;
  @override
  BridgeState build() => BridgeState.initial().copyWith(
        pimItems: [item],
        collections: [
          CollectionEntry(
              id: item.spaceId, name: 'Synthetic', cmk: List.filled(32, 1))
        ],
      );
  @override
  Future<void> loadPimItems() async {}
  @override
  Future<bool> savePimItem(PimDraft draft) async {
    saved = draft;
    return true;
  }
}
