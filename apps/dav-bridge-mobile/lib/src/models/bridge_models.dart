import 'package:rrule/rrule.dart';
import 'package:timezone/data/latest.dart' as tz_data;
import 'package:timezone/timezone.dart' as tz;

/// Result of an authentication attempt.
class LoginResult {
  const LoginResult({
    this.username,
    this.accessToken,
    this.totpContinuationToken,
    this.deviceEnrollmentToken,
    required this.totpVerified,
    this.accountMasterKey,
  });

  final String? username;
  final String? accessToken;
  final String? totpContinuationToken;
  final String? deviceEnrollmentToken;
  final bool totpVerified;
  final List<int>? accountMasterKey;
}

class DeviceSecrets {
  const DeviceSecrets({
    required this.deviceId,
    required this.signingPrivateKey,
    required this.hpkePrivateKey,
    required this.hpkePublicKey,
  });

  final String deviceId;
  final List<int> signingPrivateKey;
  final List<int> hpkePrivateKey;
  final List<int> hpkePublicKey;
}

class ProvisionResult {
  const ProvisionResult({
    required this.accessToken,
    required this.device,
    required this.collections,
  });

  final String accessToken;
  final DeviceSecrets device;
  final List<CollectionEntry> collections;
}

/// Result of issuing a short-lived collection invite code.
class IssuedInviteCode {
  const IssuedInviteCode({
    required this.code,
    required this.ttlMinutes,
    required this.keyEpoch,
    required this.currentStateStartSeq,
    required this.collectionKey,
  });

  final String code;
  final int ttlMinutes;
  final int keyEpoch;
  final int currentStateStartSeq;
  final List<int> collectionKey;
}

/// Result of redeeming an invite code.
class RedeemedInvite {
  const RedeemedInvite({
    required this.collectionId,
    required this.role,
    required this.keyEpoch,
    this.historyStartSeq = 0,
    this.currentStateStartSeq = 0,
    required this.collectionKey,
  });

  final String collectionId;
  final String role;
  final int keyEpoch;
  final int historyStartSeq;
  final int currentStateStartSeq;
  final List<int> collectionKey;
}

/// In-memory collection descriptor used by the mobile bridge UI.
class CollectionEntry {
  const CollectionEntry({
    required this.id,
    required this.name,
    required this.cmk,
    this.keyEpoch = 1,
    this.historyStartSeq = 0,
    this.currentStateStartSeq = 0,
    this.role = 'owner',
  });

  final String id;
  final String name;
  final List<int> cmk;
  final int keyEpoch;
  final int historyStartSeq;
  final int currentStateStartSeq;
  final String role;
}

enum PimItemKind {
  calendarEvent('calendar_event'),
  task('task'),
  contact('contact');

  const PimItemKind(this.wireName);

  final String wireName;

  static PimItemKind fromWireName(String value) => switch (value) {
        'calendar_event' => PimItemKind.calendarEvent,
        'task' => PimItemKind.task,
        'contact' => PimItemKind.contact,
        _ => throw FormatException('Unknown PIM item kind: $value'),
      };
}

class PimTemporal {
  const PimTemporal({
    required this.kind,
    this.date,
    this.utc,
    this.local,
    this.timezone,
  });

  factory PimTemporal.utc(DateTime value) => PimTemporal(
        kind: 'utc',
        utc: _formatCompactUtc(value.toUtc()),
      );

  factory PimTemporal.allDay(DateTime value) => PimTemporal(
        kind: 'date',
        date:
            '${value.year.toString().padLeft(4, '0')}-${value.month.toString().padLeft(2, '0')}-${value.day.toString().padLeft(2, '0')}',
      );

  factory PimTemporal.zoned(DateTime wallTime, String timezone) {
    _ensureTimeZonesInitialized();
    final location = tz.getLocation(timezone);
    final instant = tz.TZDateTime(
      location,
      wallTime.year,
      wallTime.month,
      wallTime.day,
      wallTime.hour,
      wallTime.minute,
      wallTime.second,
    );
    return PimTemporal(
      kind: 'zoned_datetime',
      local: _formatCompactLocal(wallTime),
      timezone: timezone,
      utc: _formatCompactUtc(instant.toUtc()),
    );
  }

  final String kind;
  final String? date;
  final String? utc;
  final String? local;
  final String? timezone;

  bool get isAllDay => kind == 'date';

  DateTime? toDateTime() {
    if (kind == 'date' && date != null) return DateTime.tryParse(date!);
    final value = utc;
    if (value == null) {
      final localValue = local;
      if (localValue == null || localValue.length != 15) return null;
      final wallTime = _parseCompactLocal(localValue);
      final zone = timezone;
      if (wallTime == null || zone == null || zone.isEmpty) return null;
      try {
        _ensureTimeZonesInitialized();
        return tz.TZDateTime(
          tz.getLocation(zone),
          wallTime.year,
          wallTime.month,
          wallTime.day,
          wallTime.hour,
          wallTime.minute,
          wallTime.second,
        ).toUtc();
      } on ArgumentError {
        return null;
      }
    }
    final match = RegExp(
      r'^(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})Z$',
    ).firstMatch(value);
    if (match == null) return null;
    return DateTime.utc(
      int.parse(match.group(1)!),
      int.parse(match.group(2)!),
      int.parse(match.group(3)!),
      int.parse(match.group(4)!),
      int.parse(match.group(5)!),
      int.parse(match.group(6)!),
    );
  }

  DateTime? toEditorDateTime() {
    if (kind == 'date' && date != null) return DateTime.tryParse(date!);
    if (kind == 'zoned_datetime' && local != null) {
      return _parseCompactLocal(local!);
    }
    return toDateTime()?.toLocal();
  }
}

bool _timeZonesInitialized = false;

void _ensureTimeZonesInitialized() {
  if (_timeZonesInitialized) return;
  tz_data.initializeTimeZones();
  _timeZonesInitialized = true;
}

DateTime? _parseCompactLocal(String value) {
  final match = RegExp(
    r'^(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})$',
  ).firstMatch(value);
  if (match == null) return null;
  return DateTime(
    int.parse(match.group(1)!),
    int.parse(match.group(2)!),
    int.parse(match.group(3)!),
    int.parse(match.group(4)!),
    int.parse(match.group(5)!),
    int.parse(match.group(6)!),
  );
}

String _formatCompactUtc(DateTime value) {
  String two(int number) => number.toString().padLeft(2, '0');
  return '${value.year.toString().padLeft(4, '0')}${two(value.month)}${two(value.day)}T${two(value.hour)}${two(value.minute)}${two(value.second)}Z';
}

String _formatCompactLocal(DateTime value) {
  String two(int number) => number.toString().padLeft(2, '0');
  return '${value.year.toString().padLeft(4, '0')}${two(value.month)}${two(value.day)}T${two(value.hour)}${two(value.minute)}${two(value.second)}';
}

class PimLabeledValue {
  const PimLabeledValue({
    required this.label,
    required this.value,
    this.rawHead,
  });

  final String label;
  final String value;
  final String? rawHead;
}

class PimPostalAddress {
  const PimPostalAddress({
    this.label = '',
    this.rawHead,
    this.poBox = '',
    this.extended = '',
    this.street = '',
    this.locality = '',
    this.region = '',
    this.postalCode = '',
    this.country = '',
  });

  final String label;
  final String? rawHead;
  final String poBox;
  final String extended;
  final String street;
  final String locality;
  final String region;
  final String postalCode;
  final String country;
}

class PimRecurrenceOverride {
  const PimRecurrenceOverride(
      {required this.recurrenceId,
      this.startsAt,
      this.endsAt,
      this.title,
      this.notes,
      this.location,
      this.cancelled = false});
  final PimTemporal recurrenceId;
  final PimTemporal? startsAt;
  final PimTemporal? endsAt;
  final String? title;
  final String? notes;
  final String? location;
  final bool cancelled;
}

class PimCalendarOccurrence {
  const PimCalendarOccurrence(
      {required this.startsAt,
      required this.endsAt,
      required this.allDay,
      this.title,
      this.notes,
      this.location});
  final DateTime startsAt;
  final DateTime endsAt;
  final bool allDay;
  final String? title;
  final String? notes;
  final String? location;
}

class PimItem {
  const PimItem({
    required this.spaceId,
    required this.resourceId,
    required this.projectionId,
    required this.headOperationId,
    required this.kind,
    required this.title,
    required this.completed,
    this.completedAt,
    this.notes,
    this.startsAt,
    this.endsAt,
    this.dueAt,
    this.priority = 0,
    this.location,
    this.recurrenceRule,
    this.recurrenceExdates = const [],
    this.recurrenceRdates = const [],
    this.recurrenceOverrides = const [],
    this.recurrenceUnsupported = const [],
    this.reminderMinutes,
    this.categories = const [],
    this.namePrefix = '',
    this.givenName = '',
    this.middleName = '',
    this.familyName = '',
    this.nameSuffix = '',
    this.emails = const [],
    this.phones = const [],
    this.addresses = const [],
    this.organization,
    this.jobTitle,
    this.birthday,
    this.url,
    this.favorite = false,
    this.conflict = false,
  });

  final String spaceId;
  final String resourceId;
  final String projectionId;
  final String headOperationId;
  final PimItemKind kind;
  final String title;
  final bool completed;
  final String? completedAt;
  final String? notes;
  final PimTemporal? startsAt;
  final PimTemporal? endsAt;
  final PimTemporal? dueAt;
  final int priority;
  final String? location;
  final String? recurrenceRule;
  final List<PimTemporal> recurrenceExdates;
  final List<PimTemporal> recurrenceRdates;
  final List<PimRecurrenceOverride> recurrenceOverrides;
  final List<String> recurrenceUnsupported;
  final int? reminderMinutes;
  final List<String> categories;
  final String namePrefix;
  final String givenName;
  final String middleName;
  final String familyName;
  final String nameSuffix;
  final List<PimLabeledValue> emails;
  final List<PimLabeledValue> phones;
  final List<PimPostalAddress> addresses;
  final String? organization;
  final String? jobTitle;
  final String? birthday;
  final String? url;
  final bool favorite;
  final bool conflict;
}

class PimDraft {
  const PimDraft({
    required this.spaceId,
    this.resourceId,
    this.projectionId,
    this.headOperationId,
    required this.kind,
    required this.title,
    this.completed = false,
    this.completedAt,
    this.notes,
    this.startsAt,
    this.endsAt,
    this.dueAt,
    this.priority = 0,
    this.location,
    this.recurrenceRule,
    this.reminderMinutes,
    this.categories = const [],
    this.namePrefix = '',
    this.givenName = '',
    this.middleName = '',
    this.familyName = '',
    this.nameSuffix = '',
    this.emails = const [],
    this.phones = const [],
    this.addresses = const [],
    this.organization,
    this.jobTitle,
    this.birthday,
    this.url,
    this.favorite = false,
  });

  factory PimDraft.fromItem(PimItem item, {bool? completed}) => PimDraft(
        spaceId: item.spaceId,
        resourceId: item.resourceId,
        projectionId: item.projectionId,
        headOperationId: item.headOperationId,
        kind: item.kind,
        title: item.title,
        completed: completed ?? item.completed,
        completedAt: completed == true
            ? _formatCompactUtc(DateTime.now().toUtc())
            : completed == false
                ? null
                : item.completedAt,
        notes: item.notes,
        startsAt: item.startsAt,
        endsAt: item.endsAt,
        dueAt: item.dueAt,
        priority: item.priority,
        location: item.location,
        recurrenceRule: item.recurrenceRule,
        reminderMinutes: item.reminderMinutes,
        categories: item.categories,
        namePrefix: item.namePrefix,
        givenName: item.givenName,
        middleName: item.middleName,
        familyName: item.familyName,
        nameSuffix: item.nameSuffix,
        emails: item.emails,
        phones: item.phones,
        addresses: item.addresses,
        organization: item.organization,
        jobTitle: item.jobTitle,
        birthday: item.birthday,
        url: item.url,
        favorite: item.favorite,
      );

  final String spaceId;
  final String? resourceId;
  final String? projectionId;
  final String? headOperationId;
  final PimItemKind kind;
  final String title;
  final bool completed;
  final String? completedAt;
  final String? notes;
  final PimTemporal? startsAt;
  final PimTemporal? endsAt;
  final PimTemporal? dueAt;
  final int priority;
  final String? location;
  final String? recurrenceRule;
  final int? reminderMinutes;
  final List<String> categories;
  final String namePrefix;
  final String givenName;
  final String middleName;
  final String familyName;
  final String nameSuffix;
  final List<PimLabeledValue> emails;
  final List<PimLabeledValue> phones;
  final List<PimPostalAddress> addresses;
  final String? organization;
  final String? jobTitle;
  final String? birthday;
  final String? url;
  final bool favorite;
}

/// Applies the imported recurrence set while retaining the series for editing.
bool pimIntervalOccursOnDate(PimItem item, DateTime date) =>
    pimOccurrencesOnDate(item, date).isNotEmpty;

List<PimCalendarOccurrence> pimOccurrencesOnDate(PimItem item, DateTime date) {
  if (item.recurrenceUnsupported.isNotEmpty) return [];
  final temporal = item.startsAt;
  if (temporal == null || temporal.toDateTime() == null) return [];
  DateTime? axis(PimTemporal? value) {
    final instant = value?.toDateTime();
    if (instant == null) return null;
    return value!.isAllDay
        ? DateTime.utc(instant.year, instant.month, instant.day)
        : DateTime.fromMillisecondsSinceEpoch(instant.millisecondsSinceEpoch,
            isUtc: true);
  }

  final start = axis(temporal)!;
  final floatingStart = _floatingTemporal(temporal);
  // Explicit DTEND defines an exact duration for every instance. Date-only
  // values use the civil axis; timed values use converted absolute instants.
  final end = axis(item.endsAt) ??
      start.add(temporal.isAllDay
          ? const Duration(days: 1)
          : const Duration(milliseconds: 1));
  final duration = end.difference(start);
  final dayStart = DateTime(date.year, date.month, date.day);
  final dayEnd = DateTime(date.year, date.month, date.day + 1);
  final excluded = item.recurrenceExdates
      .map((value) => axis(value)?.millisecondsSinceEpoch)
      .toSet();
  final replaced = item.recurrenceOverrides
      .map((value) => axis(value.recurrenceId)?.millisecondsSinceEpoch)
      .toSet();
  final candidates = <int, DateTime>{start.millisecondsSinceEpoch: start};
  final recurrence = item.recurrenceRule;
  if (recurrence != null && recurrence.isNotEmpty) {
    try {
      final rule = RecurrenceRule.fromString(
          recurrence.toUpperCase().startsWith('RRULE:')
              ? recurrence
              : 'RRULE:$recurrence');
      final searchStart = DateTime.utc(date.year, date.month, date.day)
          .subtract(duration)
          .subtract(const Duration(days: 1));
      final searchEnd = DateTime.utc(date.year, date.month, date.day + 2);
      if (!searchEnd.isBefore(floatingStart)) {
        for (final floating in rule.getInstances(
            start: floatingStart,
            after: searchStart.isBefore(floatingStart)
                ? floatingStart
                : searchStart,
            includeAfter: true,
            before: searchEnd,
            includeBefore: true)) {
          final occurrence = temporal.isAllDay
              ? floating
              : _recurrenceInstant(temporal, floating);
          candidates[occurrence.millisecondsSinceEpoch] = occurrence;
        }
      }
    } on FormatException {
      // Preserve the original visible anchor for an unsupported rule.
    }
  }
  for (final value in item.recurrenceRdates) {
    final occurrence = axis(value);
    if (occurrence != null) {
      candidates[occurrence.millisecondsSinceEpoch] = occurrence;
    }
  }
  final output = <PimCalendarOccurrence>[];
  void add(DateTime begin, DateTime end, bool allDay,
      [PimRecurrenceOverride? override]) {
    final displayedStart =
        allDay ? DateTime(begin.year, begin.month, begin.day) : begin.toLocal();
    final displayedEnd =
        allDay ? DateTime(end.year, end.month, end.day) : end.toLocal();
    if (displayedStart.isBefore(dayEnd) && displayedEnd.isAfter(dayStart)) {
      output.add(PimCalendarOccurrence(
          startsAt: displayedStart,
          endsAt: displayedEnd,
          allDay: allDay,
          title: override?.title,
          notes: override?.notes,
          location: override?.location));
    }
  }

  for (final entry in candidates.entries) {
    if (!excluded.contains(entry.key) && !replaced.contains(entry.key)) {
      // DTSTART is an explicit interval, including its absolute DTEND across
      // offset changes. Subsequent occurrences use the same exact duration.
      final end = entry.key == start.millisecondsSinceEpoch
          ? axis(item.endsAt) ?? entry.value.add(duration)
          : entry.value.add(duration);
      add(entry.value, end, temporal.isAllDay);
    }
  }
  final seenOverrides = <int>{};
  for (final override in item.recurrenceOverrides) {
    final identity = axis(override.recurrenceId)?.millisecondsSinceEpoch;
    if (identity == null ||
        !seenOverrides.add(identity) ||
        override.cancelled ||
        excluded.contains(identity)) {
      continue;
    }
    final begin = axis(override.startsAt);
    if (begin == null) continue;
    add(begin, axis(override.endsAt) ?? begin.add(duration),
        override.startsAt!.isAllDay, override);
  }
  output.sort((left, right) => left.startsAt.compareTo(right.startsAt));
  return output;
}

DateTime _floatingTemporal(PimTemporal value) {
  // UTC rules recur in UTC fields; the editor converts them to device time.
  final wall = value.kind == 'utc'
      ? value.toDateTime()!
      : value.toEditorDateTime() ?? value.toDateTime()!;
  return DateTime.utc(
    wall.year,
    wall.month,
    wall.day,
    wall.hour,
    wall.minute,
    wall.second,
  );
}

DateTime _recurrenceInstant(PimTemporal template, DateTime floating) {
  if (template.kind == 'zoned_datetime' && template.timezone != null) {
    return PimTemporal.zoned(
      DateTime(
        floating.year,
        floating.month,
        floating.day,
        floating.hour,
        floating.minute,
        floating.second,
      ),
      template.timezone!,
    ).toDateTime()!.toLocal();
  }
  if (template.kind == 'utc') return floating.toLocal();
  return DateTime(
    floating.year,
    floating.month,
    floating.day,
    floating.hour,
    floating.minute,
    floating.second,
  );
}
