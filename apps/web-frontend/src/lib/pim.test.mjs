import { describe, expect, test } from "bun:test";
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
