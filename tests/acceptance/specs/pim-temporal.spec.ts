import { test, expect } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { createServer } from "node:http";
import type { AddressInfo } from "node:net";

// Compile the actual editor and its real children; only the application-shell
// environment is stubbed because this fixture has no SvelteKit server.
let server: ReturnType<typeof createServer>;
let baseURL: string;
test.use({ timezoneId: "UTC" });
test.beforeAll(async () => {
  const bundle = spawnSync("bun", [new URL("../fixtures/taskEditor.bundle.mjs", import.meta.url).pathname], { encoding: "utf8" });
  if (bundle.status !== 0) throw new Error(bundle.stderr);
  server = createServer((_request, response) => {
    response.writeHead(200, { "content-type": "text/html" });
    response.end(`<html><body><script type="module">${bundle.stdout}</script></body></html>`);
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  baseURL = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
});
test.afterAll(async () => {
  if (!server) return;
  server.closeAllConnections();
  await new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
});

for (const [name, due] of [
  ["foreign timezone", { type: "record", value: { kind: "zoned_datetime", local: "20260828T183000", timezone: "Asia/Tbilisi", utc: "20260828T143000Z" } }],
  ["date-only", { type: "record", value: { kind: "date", date: "2026-08-28" } }],
  ["nonzero seconds", { type: "record", value: { kind: "zoned_datetime", local: "20260828T183047", timezone: "Asia/Tbilisi", utc: "20260828T143047Z" } }],
]) {
  test(`task title save preserves ${name} due record in a UTC browser`, async ({ page }) => {
    await page.goto(baseURL);
    await page.evaluate((value) => (window as any).openTask(value), due);
    await page.getByRole("button", { name: "Edit", exact: true }).click();
    await page.getByLabel("Title", { exact: true }).fill("Renamed");
    await page.getByRole("button", { name: "Save task", exact: true }).click();
    await page.waitForFunction(() => (window as any).saved);
    expect(await page.evaluate(() => (window as any).saved.title)).toEqual({ type: "text", value: "Renamed" });
    expect(await page.evaluate(() => (window as any).saved.due_at)).toEqual(due);
  });
}
test("explicit task due edit retains the original timezone and correct instant", async ({ page }) => {
    await page.goto(baseURL);
    await page.evaluate(() => (window as any).openTask({ type: "record", value: { kind: "zoned_datetime", local: "20260828T183000", timezone: "Asia/Tbilisi", utc: "20260828T143000Z" } }));
    await page.getByRole("button", { name: "Edit", exact: true }).click();
    await page.getByLabel("Due date and time", { exact: true }).fill("2026-08-28T19:15");
    await page.getByRole("button", { name: "Save task", exact: true }).click();
    await page.waitForFunction(() => (window as any).saved);
    expect(await page.evaluate(() => (window as any).saved.due_at)).toEqual({ type: "record", value: {
      kind: "zoned_datetime", local: "20260828T191500", timezone: "Asia/Tbilisi", utc: "20260828T151500Z",
    } });
});
