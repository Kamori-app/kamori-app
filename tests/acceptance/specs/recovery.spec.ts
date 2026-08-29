import { expect, test } from "@playwright/test";

import {
  createCollection,
  createContact,
  logout,
  openAppSection,
  signIn,
  signupAndSignIn,
  syncNow,
  uniqueAccount,
} from "./helpers";

test("@full recovery kit rotates credentials and restores encrypted data", async ({
  page,
}) => {
  const account = await signupAndSignIn(page, uniqueAccount("recovery"));
  await createCollection(page, "Recovery acceptance space");
  await openAppSection(page, "Contacts");
  await createContact(page, "Recoverable Contact", "recoverable@example.test");
  await logout(page);

  await page.getByRole("button", { name: "Recover account" }).click();
  const recovery = page.getByRole("region", { name: "Recover account" });
  await expect(recovery).toBeVisible();
  const newPassword = `Recovered-${crypto.randomUUID()}-Pass!`;
  await recovery.getByPlaceholder("Username").fill(account.username);
  await recovery
    .getByPlaceholder("24-word Data Recovery Kit")
    .fill(account.recoveryPhrase);
  await recovery
    .getByPlaceholder("New password", { exact: true })
    .fill(newPassword);
  await recovery.getByPlaceholder("Confirm new password").fill(newPassword);
  await recovery.getByRole("button", { name: "Recover account" }).click();
  await expect(
    page.getByText("Account recovered. Sign in with the new password."),
  ).toBeVisible();

  await signIn(page, { username: account.username, password: newPassword });
  await syncNow(page);
  await openAppSection(page, "Contacts");
  await expect(
    page.getByText("Recoverable Contact", { exact: true }),
  ).toBeVisible();

  await page
    .getByRole("link", { name: "Settings", exact: true })
    .filter({ visible: true })
    .click();
  await page
    .getByRole("link", { name: "Security", exact: true })
    .filter({ visible: true })
    .click();
  await page.getByRole("button", { name: "Data recovery" }).click();
  const downloadPromise = page.waitForEvent("download");
  await page
    .getByRole("button", { name: "Download recovery file" })
    .click();
  const download = await downloadPromise;
  expect(download.suggestedFilename()).toMatch(
    /^kamori-recovery-[0-9a-f]{16}\.txt$/,
  );
});
