import { defineConfig, devices } from "@playwright/test";
import { tmpdir } from "node:os";
import { join } from "node:path";
export default defineConfig({
  testDir: "./specs", testMatch: /(?:(?:desktop|admin|invite)-ui|pim-recurrence)\.spec\.ts$/,
  fullyParallel: false, workers: 1, retries: 0, timeout: 30_000,
  expect: { timeout: 5_000 },
  outputDir: join(tmpdir(), "kamori-ui-gates", process.env.KAMORI_UI_GATE ?? "focused"),
  reporter: [["line"]],
  use: { ...devices["Desktop Chrome"], trace: "retain-on-failure", screenshot: "only-on-failure" },
});
