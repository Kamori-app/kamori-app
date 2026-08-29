import 'package:dav_bridge_mobile/src/models/bridge_models.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
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
}
