import { expect, test } from "@playwright/test";

import {
  createCollection,
  logout,
  openAppSection,
  signIn,
  signupAndSignIn,
  syncNow,
  uniqueAccount,
} from "./helpers";

test("@smoke infrastructure and encrypted offline PIM round-trip", async ({
  page,
  context,
  request,
}) => {
  const apiUrl = process.env.KAMORI_ACCEPTANCE_API_URL ?? "http://127.0.0.1:18080";
  const adminUrl = process.env.KAMORI_ACCEPTANCE_ADMIN_URL ?? "http://127.0.0.1:14174";
  await expect((await request.get(`${apiUrl}/health/ready`)).status()).toBe(200);
  await expect((await request.get(adminUrl)).status()).toBe(200);

  const account = await signupAndSignIn(page, uniqueAccount("smoke"));
  await createCollection(page, "Acceptance PIM");

  await openAppSection(page, "Tasks");
  await context.setOffline(true);
  await page.getByPlaceholder("New task").fill("Offline acceptance task");
  await page.getByRole("button", { name: "Add Task" }).click();
  await expect(page.getByText("Task saved to the encrypted offline outbox.")).toBeVisible();
  await expect(page.getByText("Offline acceptance task", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Complete" }).click();
  await expect(page.getByText("Task completed.")).toBeVisible();

  await context.setOffline(false);
  await syncNow(page);

  await openAppSection(page, "Calendar");
  await page.getByPlaceholder("Event title").fill("Acceptance event");
  await page.locator('input[type="datetime-local"]').nth(0).fill("2030-01-02T10:00");
  await page.locator('input[type="datetime-local"]').nth(1).fill("2030-01-02T11:00");
  await page.getByRole("button", { name: "Add Event" }).click();
  await expect(page.getByText("Event encrypted and synced.")).toBeVisible();

  await openAppSection(page, "Contacts");
  await page.getByPlaceholder("Full name").fill("Acceptance Contact");
  await page.getByPlaceholder("Email").fill("acceptance@example.test");
  await page.getByRole("button", { name: "Add Contact" }).click();
  await expect(page.getByText("Contact encrypted and synced.")).toBeVisible();

  await page.reload();
  await expect(page.getByRole("region", { name: "Sign in" })).toBeVisible();
  await signIn(page, account);
  await openAppSection(page, "Tasks");
  await expect(page.getByText("Offline acceptance task", { exact: true })).toBeVisible();
  await expect(page.getByText("Completed", { exact: true })).toBeVisible();
  await openAppSection(page, "Calendar");
  await expect(page.getByText("Acceptance event", { exact: true })).toBeVisible();
  await openAppSection(page, "Contacts");
  await expect(page.getByText("Acceptance Contact", { exact: true })).toBeVisible();

  await logout(page);
  await signIn(page, account, { allowLocalUnlock: true });
  await syncNow(page);
  await openAppSection(page, "Tasks");
  await expect(page.getByText("Offline acceptance task", { exact: true })).toBeVisible();
  await expect(page.getByText("Completed", { exact: true })).toBeVisible();

  await page.reload();
  await expect(page.getByRole("button", { name: "Log out" })).toBeVisible();
  await expect(page.getByText("Offline acceptance task", { exact: true })).toBeVisible();
});
