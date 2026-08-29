import { expect, test } from "@playwright/test";

import {
  createCollection,
  createContact,
  createTask,
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
  const apiUrl =
    process.env.KAMORI_ACCEPTANCE_API_URL ?? "http://127.0.0.1:18080";
  const adminUrl =
    process.env.KAMORI_ACCEPTANCE_ADMIN_URL ?? "http://127.0.0.1:14174";
  await expect((await request.get(`${apiUrl}/health/ready`)).status()).toBe(
    200,
  );
  await expect((await request.get(adminUrl)).status()).toBe(200);

  const account = await signupAndSignIn(page, uniqueAccount("smoke"));
  await createCollection(page, "Acceptance PIM");

  await openAppSection(page, "Tasks");
  await context.setOffline(true);
  await createTask(page, "Offline acceptance task", { offline: true });
  await page.getByRole("checkbox", { name: "Complete task" }).check();
  await expect(page.getByText("Task completed.")).toBeVisible();

  await context.setOffline(false);
  await syncNow(page);

  await openAppSection(page, "Calendar");
  await page.getByRole("button", { name: "New event", exact: true }).click();
  const eventEditor = page.getByRole("dialog", { name: "New event" });
  await eventEditor
    .getByRole("textbox", { name: "Title" })
    .fill("Acceptance event");
  await eventEditor.getByLabel("Starts").fill("2030-01-02T10:00");
  await eventEditor.getByLabel("Ends").fill("2030-01-02T11:00");
  await eventEditor.getByRole("button", { name: "Save event" }).click();
  await expect(page.getByText("Changes encrypted and synced.")).toBeVisible();

  await openAppSection(page, "Contacts");
  await createContact(page, "Acceptance Contact", "acceptance@example.test");

  await page.reload();
  await expect(page.getByRole("region", { name: "Sign in" })).toBeVisible();
  await signIn(page, account);
  await openAppSection(page, "Tasks");
  await page.getByRole("button", { name: /Completed \(\d+\)/ }).click();
  await expect(
    page.getByText("Offline acceptance task", { exact: true }),
  ).toBeVisible();
  await expect(page.getByText("Completed", { exact: true })).toBeVisible();
  await openAppSection(page, "Calendar");
  await page.getByRole("button", { name: "Agenda", exact: true }).click();
  await expect(
    page.getByText("Acceptance event", { exact: true }),
  ).toBeVisible();
  await openAppSection(page, "Contacts");
  await expect(
    page.getByText("Acceptance Contact", { exact: true }),
  ).toBeVisible();

  await logout(page);
  await signIn(page, account, { allowLocalUnlock: true });
  await syncNow(page);
  await openAppSection(page, "Tasks");
  await page.getByRole("button", { name: /Completed \(\d+\)/ }).click();
  await expect(
    page.getByText("Offline acceptance task", { exact: true }),
  ).toBeVisible();
  await expect(page.getByText("Completed", { exact: true })).toBeVisible();

  await page.reload();
  await expect(page.getByRole("button", { name: "Log out" })).toBeVisible();
  await page.getByRole("button", { name: /Completed \(\d+\)/ }).click();
  await expect(
    page.getByText("Offline acceptance task", { exact: true }),
  ).toBeVisible();
});
