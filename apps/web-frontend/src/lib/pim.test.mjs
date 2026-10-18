import { describe, expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { encode } from "@msgpack/msgpack";

import {
  CURRENT_PIM_SCHEMA_VERSION,
  decodePimOperation,
  decodePimSnapshot,
  encodePimOperation,
  encodePimSnapshot,
  dateToIcalendarUtc,
  intervalOccursOnDay,
  localDateTimeToTemporal,
  localDateTimeToIcalendarUtc,
  projectionFields,
  projectionProperty,
  recurringIntervalOccursOnDay,
  temporalToDate,
  temporalToInputValue,
} from "./pim.ts";

const RESOURCE_ID = "b61c6782-eddd-4bbf-a6fe-4e610720a10c";
const HEAD_ID = "90664f99-0982-45e2-a76b-61ff3539703c";

describe("encrypted PIM codecs", () => {
  test("encodes browser dates as compact UTC iCalendar values", () => {
    expect(dateToIcalendarUtc(new Date("2026-08-23T12:34:56Z"))).toBe(
      "20260823T123456Z",
    );
    expect(localDateTimeToIcalendarUtc("2026-08-23T12:34")).toMatch(
      /^\d{8}T\d{6}Z$/,
    );
    expect(() => localDateTimeToIcalendarUtc("2026-02-30T12:34")).toThrow(
      /does not exist/i,
    );
  });

  test("accepts a well-formed field operation", () => {
    const operation = decodePimOperation(
      encode({
        operation: "upsert",
        resource_kind: "task",
        resource_id: RESOURCE_ID,
        dependencies: [HEAD_ID],
        fields: {
          title: { type: "text", value: "Ship beta" },
          completed: { type: "boolean", value: false },
        },
        raw_projection: new Uint8Array(),
      }),
    );
    expect(operation.resource_id).toBe(RESOURCE_ID);
    expect(operation.schema_version).toBe(1);
  });

  test("accepts rich schema v2 values and rejects future schemas", () => {
    const payload = {
      operation: "upsert",
      schema_version: CURRENT_PIM_SCHEMA_VERSION,
      resource_kind: "task",
      resource_id: RESOURCE_ID,
      dependencies: [HEAD_ID],
      fields: {
        due_at: {
          type: "record",
          value: {
            kind: "zoned_datetime",
            local: "20260828T183000",
            timezone: "Asia/Tbilisi",
            utc: "20260828T143000Z",
          },
        },
        categories: { type: "text_list", value: ["Release", "Work"] },
      },
      raw_projection: new Uint8Array(),
    };
    expect(decodePimOperation(encode(payload)).schema_version).toBe(2);
    expect(() =>
      decodePimOperation(encode({ ...payload, schema_version: 3 })),
    ).toThrow(/unsupported schema version/i);
  });

  test("rejects values that only satisfy a TypeScript cast", () => {
    expect(() =>
      decodePimOperation(
        encode({
          operation: "upsert",
          resource_kind: "task",
          resource_id: RESOURCE_ID,
          dependencies: [],
          fields: { completed: { type: "boolean", value: "false" } },
          raw_projection: new Uint8Array(),
        }),
      ),
    ).toThrow("invalid");
  });

  test("refuses to encode invalid operations and snapshots", () => {
    expect(() => encodePimOperation({
      operation: "delete",
      resource_kind: "task",
      resource_id: RESOURCE_ID,
      dependencies: [HEAD_ID, RESOURCE_ID],
      projection_resource_id: null,
    })).toThrow(/invalid envelope/i);
    expect(() => encodePimSnapshot({
      schema_version: 2,
      covers_through_space_seq: 1,
      resource_kind: "contact",
      resource_id: RESOURCE_ID,
      branches: [],
    })).toThrow(/invalid/i);
  });

  test("preserves a tombstone snapshot with an empty projection", () => {
    const snapshot = decodePimSnapshot(
      encode({
        schema_version: 2,
        covers_through_space_seq: 42,
        resource_kind: "contact",
        resource_id: RESOURCE_ID,
        branches: [{
          projection_resource_id: "alice.vcf",
          head_operation_id: HEAD_ID,
          deleted: true,
          materialized_projection: new Uint8Array(),
        }],
      }),
    );
    expect(snapshot.branches[0].deleted).toBe(true);
  });

  test("rejects a live snapshot without materialized data", () => {
    expect(() =>
      decodePimSnapshot(
        encode({
          schema_version: 2,
          covers_through_space_seq: 42,
          resource_kind: "contact",
          resource_id: RESOURCE_ID,
          branches: [{
            projection_resource_id: "alice.vcf",
            head_operation_id: HEAD_ID,
            deleted: false,
            materialized_projection: new Uint8Array(),
          }],
        }),
      ),
    ).toThrow("must contain");
  });

  test("rejects multi-parent PIM v1 operations", () => {
    const encoded = encode({
      operation: "delete",
      resource_kind: "task",
      resource_id: "00000000-0000-4000-8000-000000000001",
      dependencies: [
        "00000000-0000-4000-8000-000000000002",
        "00000000-0000-4000-8000-000000000003",
      ],
      projection_resource_id: null,
    });

    expect(() => decodePimOperation(encoded)).toThrow(/invalid envelope/i);
  });

  test("reads parameterized and folded vCard properties", () => {
    const projection = [
      "BEGIN:VCARD",
      "VERSION:4.0",
      "UID:alice",
      "FN:Alice Ex",
      " ample",
      "item1.EMAIL;TYPE=work:alice@example.com",
      "END:VCARD",
      "",
    ].join("\r\n");

    expect(projectionProperty(projection, "contact", "FN")).toBe("Alice Example");
    expect(projectionFields(projection, "contact")).toMatchObject({
      title: { type: "text", value: "Alice Example" },
      email: { type: "text", value: "alice@example.com" },
    });
  });

  test("extracts rich fields without losing quoted custom labels", () => {
    const projection = [
      "BEGIN:VCARD",
      "VERSION:4.0",
      "UID:alice",
      "FN:Alice Example",
      "N:Example;Alice;;;",
      'EMAIL;X-KAMORI-LABEL="personal;secure":alice@example.com',
      "TEL;TYPE=cell:+995555123456",
      "ADR;TYPE=home:;;1 Rustaveli Ave;Tbilisi;;0108;Georgia",
      "ORG:Kamori",
      "TITLE:Engineer",
      "BDAY:1990-08-28",
      "X-KAMORI-FAVORITE:TRUE",
      "END:VCARD",
      "",
    ].join("\r\n");

    expect(projectionFields(projection, "contact")).toMatchObject({
      name: {
        type: "record",
        value: { family: "Example", given: "Alice" },
      },
      emails: {
        type: "records",
        value: [{ label: "personal;secure", value: "alice@example.com" }],
      },
      phones: {
        type: "records",
        value: [{ label: "cell", value: "+995555123456" }],
      },
      organization: { type: "text", value: "Kamori" },
      favorite: { type: "boolean", value: true },
    });
  });

  test("extracts managed reminders and timezone-aware event instants", () => {
    const projection = [
      "BEGIN:VCALENDAR",
      "VERSION:2.0",
      "BEGIN:VEVENT",
      "UID:event-1",
      "SUMMARY:Release review",
      "DTSTART;TZID=Asia/Tbilisi:20260828T183000",
      "DTEND;TZID=Asia/Tbilisi:20260828T193000",
      "RRULE:FREQ=WEEKLY;BYDAY=FR",
      "BEGIN:VALARM",
      "X-KAMORI-MANAGED:TRUE",
      "TRIGGER:-PT15M",
      "ACTION:DISPLAY",
      "END:VALARM",
      "END:VEVENT",
      "END:VCALENDAR",
      "",
    ].join("\r\n");
    const fields = projectionFields(projection, "calendar_event");
    expect(fields).toMatchObject({
      reminder_minutes: { type: "integer", value: 15 },
      recurrence_rule: { type: "text", value: "FREQ=WEEKLY;BYDAY=FR" },
    });
    expect(temporalToDate(fields.starts_at)?.toISOString()).toBe(
      "2026-08-28T14:30:00.000Z",
    );
    expect(temporalToInputValue(fields.starts_at)).toBe("2026-08-28T18:30");
  });

  test("creates a typed temporal value from a browser-local input", () => {
    const temporal = localDateTimeToTemporal("2026-08-28T18:30");
    expect(temporal.kind).toBe("zoned_datetime");
    expect(temporal.local).toBe("20260828T183000");
    expect(temporal.utc).toMatch(/^\d{8}T\d{6}Z$/);
  });

  test("keeps an explicit IANA wall time and UTC instant consistent", () => {
    const temporal = localDateTimeToTemporal(
      "2026-08-28T18:30",
      "Asia/Tbilisi",
    );
    expect(temporal).toEqual({
      kind: "zoned_datetime",
      local: "20260828T183000",
      timezone: "Asia/Tbilisi",
      utc: "20260828T143000Z",
    });
  });

  test("treats event end as exclusive in calendar day views", () => {
    const start = new Date(2026, 7, 28);
    const exclusiveEnd = new Date(2026, 7, 29);
    expect(intervalOccursOnDay(start, exclusiveEnd, new Date(2026, 7, 28))).toBe(true);
    expect(intervalOccursOnDay(start, exclusiveEnd, new Date(2026, 7, 29))).toBe(false);
  });

  test("expands recurring events and keeps events without DTEND visible", () => {
    const weeklyStart = {
      type: "record",
      value: { kind: "date", date: "2026-08-28" },
    };
    const weeklyEnd = {
      type: "record",
      value: { kind: "date", date: "2026-08-29" },
    };
    expect(recurringIntervalOccursOnDay(
      weeklyStart,
      weeklyEnd,
      "FREQ=WEEKLY",
      new Date(2026, 8, 4),
    )).toBe(true);
    expect(recurringIntervalOccursOnDay(
      weeklyStart,
      weeklyEnd,
      "FREQ=WEEKLY",
      new Date(2026, 8, 5),
    )).toBe(false);
    expect(recurringIntervalOccursOnDay(
      weeklyStart,
      undefined,
      "",
      new Date(2026, 7, 28),
    )).toBe(true);
  });

  test("reads the recurrence master instead of an exception", () => {
    const projection = [
      "BEGIN:VCALENDAR",
      "VERSION:2.0",
      "BEGIN:VEVENT",
      "UID:event-1",
      "SUMMARY:Master",
      "END:VEVENT",
      "BEGIN:VEVENT",
      "UID:event-1",
      "RECURRENCE-ID:20260823T120000Z",
      "SUMMARY:Exception",
      "END:VEVENT",
      "END:VCALENDAR",
      "",
    ].join("\r\n");

    expect(projectionProperty(projection, "calendar_event", "SUMMARY")).toBe("Master");
  });
});


// These fixtures specify UTC calendar days. Isolate their viewer zone rather
// than changing process.env.TZ for the host suite or concurrent tests.
const recurringIntervalOccursOnUtcDay = (start, end, rule, day) => {
  const args = JSON.stringify([start, end, rule, [day.getFullYear(), day.getMonth(), day.getDate()]]);
  const probe = `
    import { recurringIntervalOccursOnDay } from ${JSON.stringify(new URL("./pim.ts", import.meta.url).pathname)};
    const [start, end, rule, day] = ${args};
    console.log(recurringIntervalOccursOnDay(start, end, rule, new Date(...day)));
  `;
  const result = Bun.spawnSync([process.execPath, "--eval", probe], { env: { ...process.env, TZ: "UTC" } });
  expect(result.exitCode).toBe(0);
  return JSON.parse(result.stdout.toString());
};

describe("timed recurrence across zones", () => {
  const zoned = (input, zone) => ({ type: "record", value: localDateTimeToTemporal(input, zone) });
  test("weekly Tbilisi 07:00 stays on Friday in a UTC browser", () => {
    const start = zoned("2026-08-28T07:00", "Asia/Tbilisi");
    const end = zoned("2026-08-28T07:30", "Asia/Tbilisi");
    expect(recurringIntervalOccursOnUtcDay(start, end, "FREQ=WEEKLY", new Date(2026, 8, 3))).toBe(false);
    expect(recurringIntervalOccursOnUtcDay(start, end, "FREQ=WEEKLY", new Date(2026, 8, 4))).toBe(true);
  });
  test("negative-offset midnight recurrence uses the actual next-day instant", () => {
    const start = zoned("2026-08-28T23:45", "America/Los_Angeles");
    const end = zoned("2026-08-29T00:15", "America/Los_Angeles");
    expect(recurringIntervalOccursOnUtcDay(start, end, "FREQ=WEEKLY", new Date(2026, 8, 4))).toBe(false);
    expect(recurringIntervalOccursOnUtcDay(start, end, "FREQ=WEEKLY", new Date(2026, 8, 5))).toBe(true);
  });
  test("weekly DST recurrence keeps local wall time when its UTC day changes", () => {
    const start = zoned("2026-10-23T19:30", "America/New_York");
    const end = zoned("2026-10-23T19:45", "America/New_York");
    expect(recurringIntervalOccursOnUtcDay(start, end, "FREQ=WEEKLY", new Date(2026, 9, 30))).toBe(true);
    expect(recurringIntervalOccursOnUtcDay(start, end, "FREQ=WEEKLY", new Date(2026, 10, 6))).toBe(false);
    expect(recurringIntervalOccursOnUtcDay(start, end, "FREQ=WEEKLY", new Date(2026, 10, 7))).toBe(true);
  });
});

describe("recurrence browser timezone isolation", () => {
  for (const browserZone of ["UTC", "Asia/Tbilisi", "America/Los_Angeles"]) {
    test(`compares zoned occurrences as actual instants under ${browserZone}`, () => {
      const probe = `
        import { localDateTimeToTemporal, recurringIntervalOccursOnDay } from ${JSON.stringify(new URL("./pim.ts", import.meta.url).pathname)};
        const fixtures = [
          ["2026-08-28T07:00", "2026-08-28T07:30", "Asia/Tbilisi", "2026-09-04T03:00:00Z"],
          ["2026-08-28T16:00", "2026-08-28T16:30", "America/Los_Angeles", "2026-09-04T23:00:00Z"],
          ["2026-10-23T19:30", "2026-10-23T19:45", "America/New_York", "2026-11-07T00:30:00Z"],
          ["2026-08-28T23:30", "2026-08-28T23:45", "Asia/Tbilisi", "2026-09-04T19:30:00Z"],
          // Browser DST changes after this instant but before the floating anchor.
          ["2026-10-25T11:30", "2026-10-25T11:45", "Asia/Tbilisi", "2026-11-01T07:30:00Z"],
        ];
        console.log(JSON.stringify(fixtures.map(([s, e, zone, instant]) => {
          const start = { type: "record", value: localDateTimeToTemporal(s, zone) };
          const end = { type: "record", value: localDateTimeToTemporal(e, zone) };
          const day = new Date(instant);
          return [-1, 0, 1].map(offset => recurringIntervalOccursOnDay(start, end, "FREQ=WEEKLY", new Date(day.getFullYear(), day.getMonth(), day.getDate() + offset)));
        })));
      `;
      const result = Bun.spawnSync([process.execPath, "--eval", probe], { env: { ...process.env, TZ: browserZone } });
      expect(result.exitCode).toBe(0);
      expect(JSON.parse(result.stdout.toString())).toEqual([
        [false, true, false], [false, true, false],
        [false, true, false], [false, true, false], [false, true, false],
      ]);
    });
  }
});

describe("zoned recurrence UTC UNTIL and COUNT", () => {
  const start = { type: "record", value: { kind: "zoned_datetime", local: "20260828T070000", timezone: "Asia/Tbilisi", utc: "20260828T030000Z" } };
  const end = { type: "record", value: { kind: "zoned_datetime", local: "20260828T073000", timezone: "Asia/Tbilisi", utc: "20260828T033000Z" } };
  test("UTC UNTIL includes an occurrence exactly at its instant", () => {
    expect(recurringIntervalOccursOnUtcDay(start, end, "FREQ=WEEKLY;UNTIL=20260904T030000Z", new Date(2026, 8, 4))).toBe(true);
    expect(recurringIntervalOccursOnUtcDay(start, end, "FREQ=WEEKLY;UNTIL=20260904T025959Z", new Date(2026, 8, 4))).toBe(false);
  });
  test("COUNT and weekday constraints still count the original anchor", () => {
    expect(recurringIntervalOccursOnUtcDay(start, end, "FREQ=WEEKLY;BYDAY=FR;COUNT=2", new Date(2026, 8, 4))).toBe(true);
    expect(recurringIntervalOccursOnUtcDay(start, end, "FREQ=WEEKLY;BYDAY=FR;COUNT=2", new Date(2026, 8, 11))).toBe(false);
  });
  test("positive offset after midnight occurs on the previous UTC day", () => {
    const s = { type: "record", value: localDateTimeToTemporal("2026-08-28T00:15", "Asia/Tbilisi") };
    const e = { type: "record", value: localDateTimeToTemporal("2026-08-28T00:45", "Asia/Tbilisi") };
    expect(recurringIntervalOccursOnUtcDay(s, e, "FREQ=WEEKLY", new Date(2026, 8, 3))).toBe(true);
    expect(recurringIntervalOccursOnUtcDay(s, e, "FREQ=WEEKLY", new Date(2026, 8, 4))).toBe(false);
  });
});

describe("expired zoned recurrence bounded work", () => {
  for (const [name, end, until, expected] of [
    ["expired short interval", "20260828T033000Z", "20260828T030000Z", []],
    ["last interval still overlaps despite old UNTIL", "26000828T033000Z", "20260828T030000Z", ["2026-08-28T03:00:00.000Z"]],
    ["retains every overlapping instance through inclusive UNTIL", "26000828T033000Z", "20260828T030001Z", ["2026-08-28T03:00:00.000Z", "2026-08-28T03:00:01.000Z"]],
  ]) {
    test(name, () => {
      const probe = `
        import { calendarOccurrencesOnDay } from ${JSON.stringify(new URL("./pim.ts", import.meta.url).pathname)};
        const start = { type: "record", value: { kind: "zoned_datetime", local: "20260828T070000", timezone: "Asia/Tbilisi", utc: "20260828T030000Z" } };
        const end = { type: "record", value: { kind: "utc", utc: ${JSON.stringify(end)} } };
        const fields = { starts_at: start, ends_at: end, recurrence_rule: { type: "text", value: "FREQ=SECONDLY;UNTIL=${until}" } };
        console.log(JSON.stringify(calendarOccurrencesOnDay(fields, new Date(2500, 7, 28)).map(value => value.startsAt.toISOString())));
      `;
      // Hundreds of years of SECONDLY expansion cannot finish before this generous
      // process deadline. Correctly bounded work needs only the expired rule's tail.
      const result = spawnSync(process.execPath, ["--eval", probe], {
        env: { ...process.env, TZ: "UTC" }, encoding: "utf8", timeout: 10000, killSignal: "SIGKILL",
      });
      expect(result.error).toBeUndefined();
      expect(result.status).toBe(0);
      expect(JSON.parse(result.stdout)).toEqual(expected);
    }, 15000);
  }
});
