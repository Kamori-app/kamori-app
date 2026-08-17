import { defineConfig, devices } from "@playwright/test";

const webBaseUrl = process.env.KAMORI_ACCEPTANCE_WEB_URL ?? "http://127.0.0.1:14173";

export default defineConfig({
  testDir: "./specs",
  fullyParallel: false,
  workers: 1,
  retries: process.env.CI ? 1 : 0,
  timeout: 60_000,
  expect: {
    timeout: 15_000,
  },
  outputDir: "artifacts/test-results",
  reporter: [
    ["line"],
    ["html", { outputFolder: "artifacts/report", open: "never" }],
  ],
  use: {
    baseURL: webBaseUrl,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    video: "retain-on-failure",
    ...devices["Desktop Chrome"],
  },
});
