import { describe, expect, test } from "bun:test";
import { encode } from "@msgpack/msgpack";

import {
  decodePimOperation,
  decodePimSnapshot,
  dateToIcalendarUtc,
  localDateTimeToIcalendarUtc,
  projectionFields,
  projectionProperty,
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
