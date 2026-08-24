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

test("@full recovery kit rotates credentials and restores encrypted data", async ({ page }) => {
  const account = await signupAndSignIn(page, uniqueAccount("recovery"));
  await createCollection(page, "Recovery acceptance space");
  await openAppSection(page, "Contacts");
  await page.getByPlaceholder("Full name").fill("Recoverable Contact");
  await page.getByPlaceholder("Email").fill("recoverable@example.test");
  await page.getByRole("button", { name: "Add Contact" }).click();
  await expect(page.getByText("Contact encrypted and synced.")).toBeVisible();
  await logout(page);

  await page.getByRole("button", { name: "Recover account" }).click();
  const recovery = page.getByRole("region", { name: "Recover account" });
  await expect(recovery).toBeVisible();
  const newPassword = `Recovered-${crypto.randomUUID()}-Pass!`;
  await recovery.getByPlaceholder("Username").fill(account.username);
  await recovery.getByPlaceholder("24-word Data Recovery Kit").fill(account.recoveryPhrase);
  await recovery.getByPlaceholder("New password", { exact: true }).fill(newPassword);
  await recovery.getByPlaceholder("Confirm new password").fill(newPassword);
  await recovery.getByRole("button", { name: "Recover account" }).click();
  await expect(page.getByText("Account recovered. Sign in with the new password.")).toBeVisible();

  await signIn(page, { username: account.username, password: newPassword });
  await syncNow(page);
  await openAppSection(page, "Contacts");
  await expect(page.getByText("Recoverable Contact", { exact: true })).toBeVisible();
});
