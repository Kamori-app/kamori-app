import { test, expect } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { createServer } from "node:http";
import type { AddressInfo } from "node:net";

let server: ReturnType<typeof createServer>;
let baseURL: string;
const source = readFileSync(new URL("../../fixtures/pim/zoned-exceptions.ics", import.meta.url), "utf8");
test.use({ timezoneId: "UTC" });
test.beforeAll(async () => {
  const bundle = spawnSync("bun", [new URL("../fixtures/taskEditor.bundle.mjs", import.meta.url).pathname, "calendarCorpus.js"], { encoding: "utf8" });
  if (bundle.status !== 0) throw new Error(bundle.stderr);
  server = createServer((_request, response) => {
    response.writeHead(200, { "content-type": "text/html" });
    response.end(`<html><body><script type="module">${bundle.stdout}</script></body></html>`);
  });
  await new Promise<void>(resolve => server.listen(0, "127.0.0.1", resolve));
  baseURL = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
});
test.afterAll(async () => {
  if (!server) return;
  server.closeAllConnections();
  await new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
});

test("mounted agenda displays the selected week's override and edits its original series", async ({ page }) => {
  await page.clock.install({ time: new Date("2026-11-08T12:00:00Z") });
  await page.goto(baseURL);
  await page.evaluate(source => (window as any).openCalendar([source]), source);
  await page.getByRole("button", { name: "Agenda", exact: true }).click();
  // Today selects the same existing weekly window used by week view.
  const today = page.getByRole("button", { name: "Today", exact: true });
  if (await today.count()) await today.click();
  await expect(page.getByRole("heading", { name: "Detached override", exact: true })).toBeVisible();
  await expect(page.getByText("Leave this exception untouched", { exact: true })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Imported zoned master", exact: true })).toHaveCount(0);
  await page.getByRole("button", { name: "Edit series", exact: true }).click();
  await expect(page.getByLabel("Title", { exact: true })).toHaveValue("Imported zoned master");
  await page.getByLabel("Title", { exact: true }).fill("Series renamed");
  await page.getByRole("button", { name: "Save event", exact: true }).click();
  await page.waitForFunction(() => (window as any).saved);
  expect(await page.evaluate(() => (window as any).saved.item.resourceId)).toBe("series-0");
  expect(await page.evaluate(() => Object.keys((window as any).saved.fields).some(key => key.startsWith("recurrence_") && key !== "recurrence_rule"))).toBe(false);
  await page.getByRole("button", { name: "Next", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Detached override", exact: true })).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "Imported zoned master", exact: true })).toHaveCount(2);
});

test("mounted month and week show moved override content and preserve delete identity", async ({ page }) => {
  await page.clock.install({ time: new Date("2026-11-08T12:00:00Z") });
  await page.goto(baseURL);
  await page.evaluate(source => (window as any).openCalendar([source]), source);
  await expect(page.getByTitle("Edit series").filter({ hasText: "Detached override" })).toHaveCount(1);
  await page.getByRole("button", { name: "Today", exact: true }).click();
  await page.getByRole("button", { name: "Week", exact: true }).click();
  await expect(page.getByText("Detached override", { exact: true })).toBeVisible();
  await expect(page.getByText("Override room", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Agenda", exact: true }).click();
  await page.getByRole("button", { name: "Delete series", exact: true }).click();
  expect(await page.evaluate(() => (window as any).deleted.resourceId)).toBe("series-0");
});

test("mounted agenda lists a multiday occurrence once and reports unsupported imports", async ({ page }) => {
  const allDay = readFileSync(new URL("../../fixtures/pim/all-day-exceptions.ics", import.meta.url), "utf8")
    .replace("DTEND;VALUE=DATE:20261031", "DTEND;VALUE=DATE:20261102");
  const unsupported = readFileSync(new URL("../../fixtures/pim/unsupported-period-range.ics", import.meta.url), "utf8");
  await page.clock.install({ time: new Date("2026-10-31T12:00:00Z") });
  await page.goto(baseURL);
  await page.evaluate(sources => (window as any).openCalendar(sources), [allDay, unsupported]);
  await page.getByRole("button", { name: "Agenda", exact: true }).click();
  await page.getByRole("button", { name: "Today", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Imported all-day master", exact: true })).toHaveCount(1);
  await expect(page.getByRole("status")).toContainText("cannot be displayed here");
  await expect(page.getByRole("heading", { name: "Imported advanced master", exact: true })).toHaveCount(0);
});

test("agenda retains distinct original instances moved to the same actual time", async ({ page }) => {
  const collision = source.replace("END:VCALENDAR", [
    "BEGIN:VEVENT", "UID:synthetic-zoned@example.invalid",
    "RECURRENCE-ID;TZID=America/New_York:20261113T193000",
    "DTSTART;TZID=America/New_York:20261107T203000", "DTEND;TZID=America/New_York:20261107T204500",
    "SUMMARY:Another moved instance", "END:VEVENT", "END:VCALENDAR",
  ].join("\r\n"));
  await page.clock.install({ time: new Date("2026-11-08T12:00:00Z") });
  await page.goto(baseURL);
  await page.evaluate(source => (window as any).openCalendar([source]), collision);
  await page.getByRole("button", { name: "Agenda", exact: true }).click();
  await page.getByRole("button", { name: "Today", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Detached override", exact: true })).toHaveCount(1);
  await expect(page.getByRole("heading", { name: "Another moved instance", exact: true })).toHaveCount(1);
});
