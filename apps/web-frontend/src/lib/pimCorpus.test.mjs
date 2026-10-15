import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { calendarOccurrencesOnDay, projectionFields, projectionProperty, recurringIntervalOccursOnDay } from "./pim.ts";

const root = new URL("../../../../tests/fixtures/pim/", import.meta.url);
const corpus = JSON.parse(readFileSync(new URL("corpus.json", root), "utf8"));

describe("shared imported PIM corpus", () => {
  for (const fixture of corpus) {
    const source = readFileSync(new URL(fixture.file, root), "utf8");
    test(`${fixture.file}: canonical field contract preserves imported values`, () => {
      const fields = projectionFields(source, fixture.kind);
      for (const [name, expected] of Object.entries(fixture.fields)) {
        expect(fields[name]).toEqual(expected);
      }
      expect(projectionProperty(source, fixture.kind, "UID")).toBe(fixture.uid);
    });
    if (fixture.kind !== "calendar_event") continue;
    test(`${fixture.file}: recurrence set applies exceptions, DST, COUNT and exclusive ends`, () => {
      const fields = projectionFields(source, fixture.kind);
      for (const sample of fixture.display) {
        // Instants choose the viewer's local calendar day; all-day values do not
        // shift with viewer zone.
        const day = sample.instant ? new Date(sample.instant) : new Date(`${sample.day}T00:00:00`);
        expect(recurringIntervalOccursOnDay(fields.starts_at, fields.ends_at,
          fields.recurrence_rule.value, day, fields)).toBe(sample.occurs);
        const occurrences = calendarOccurrencesOnDay(fields, day);
        expect(occurrences.length).toBe(sample.occurs ? 1 : 0);
        if (sample.title) expect(occurrences[0].title ?? fields.title.value).toBe(sample.title);
        if (sample.start) expect(occurrences[0].startsAt.toISOString()).toBe(new Date(sample.start).toISOString());
        if (sample.end) expect(occurrences[0].endsAt.toISOString()).toBe(new Date(sample.end).toISOString());
        if (sample.start_day) expect(occurrences[0].startsAt).toEqual(new Date(`${sample.start_day}T00:00:00`));
        if (sample.end_day) expect(occurrences[0].endsAt).toEqual(new Date(`${sample.end_day}T00:00:00`));
        if (sample.location) expect(occurrences[0].location ?? fields.location.value).toBe(sample.location);
        if (sample.notes) expect(occurrences[0].notes ?? fields.notes.value).toBe(sample.notes);
      }
    });
  }
});
