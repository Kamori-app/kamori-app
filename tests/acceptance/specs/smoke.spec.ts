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

  await page.setViewportSize({ width: 375, height: 812 });
  await expect(page.getByRole("button", { name: "Sync now" })).toContainText(
    "Sync",
  );
  await page
    .getByRole("button", { name: "Menu", exact: true })
    .first()
    .click();
  const mobileMenu = page.locator("#mobile-application-menu");
  await expect(mobileMenu.getByRole("link", { name: "Calendar" })).toBeVisible();
  await expect(mobileMenu.getByRole("link", { name: "Tasks" })).toBeVisible();
  await mobileMenu.getByRole("button", { name: "Close menu" }).click();
  await page.setViewportSize({ width: 1280, height: 720 });

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
  // Capture once in the browser's timezone; restore navigates to this exact
  // fixture week even if the run crosses midnight, Monday, or a month boundary.
  const eventDate = await page.evaluate(() => {
    const now = new Date();
    return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(now.getDate()).padStart(2, "0")}`;
  });
  await eventEditor.getByLabel("Starts").fill(`${eventDate}T10:00`);
  await eventEditor.getByLabel("Ends").fill(`${eventDate}T11:00`);
  await eventEditor.getByRole("button", { name: "Save event" }).click();
  await expect(page.getByText("Changes encrypted and synced.")).toBeVisible();

  await openAppSection(page, "Contacts");
  await createContact(page, "Acceptance Contact", "acceptance@example.test");
  const contactCard = page
    .locator("article")
    .filter({ has: page.getByText("Acceptance Contact", { exact: true }) });
  await contactCard.getByRole("button", { name: "Delete" }).click();
  await expect(page.getByText("Acceptance Contact", { exact: true })).toHaveCount(0);
  await openAppSection(page, "Trash");
  const trashedContact = page
    .locator("article")
    .filter({ has: page.getByText("Acceptance Contact", { exact: true }) });
  await expect(trashedContact).toBeVisible();
  await trashedContact.getByRole("button", { name: "Restore" }).click();
  await expect(page.getByText("Item restored from Trash.")).toBeVisible();
  await openAppSection(page, "Contacts");
  await expect(
    page.getByText("Acceptance Contact", { exact: true }),
  ).toBeVisible();

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
  await page.getByRole("button", { name: "Today", exact: true }).click();
  const agendaHeading = page.getByRole("heading", { level: 2, name: / – / });
  const shownRange = await agendaHeading.innerText();
  const fixtureWeek = await page.evaluate(({ eventDate, shownRange }) => {
    const target = new Date(`${eventDate}T12:00:00`);
    const shownEnd = new Date(shownRange.split(" – ")[1]);
    const civilDay = (date: Date) => Date.UTC(date.getFullYear(), date.getMonth(), date.getDate()) / 86_400_000;
    const offset = Math.ceil((civilDay(target) - civilDay(shownEnd)) / 7);
    const monday = new Date(target);
    monday.setDate(target.getDate() - (target.getDay() + 6) % 7);
    const sunday = new Date(monday); sunday.setDate(monday.getDate() + 6);
    return { offset, heading: `${new Intl.DateTimeFormat("en", { month: "short", day: "numeric" }).format(monday)} – ${new Intl.DateTimeFormat("en", { month: "short", day: "numeric", year: "numeric" }).format(sunday)}` };
  }, { eventDate, shownRange });
  expect(Number.isInteger(fixtureWeek.offset)).toBe(true);
  for (let week = 0; week < Math.abs(fixtureWeek.offset); week++) {
    await page.getByRole("button", { name: fixtureWeek.offset < 0 ? "Previous" : "Next", exact: true }).click();
  }
  await expect(agendaHeading).toHaveText(fixtureWeek.heading);
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
