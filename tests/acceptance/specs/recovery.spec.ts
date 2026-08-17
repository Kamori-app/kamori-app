import { expect, test } from "@playwright/test";

import {
  createCollection,
  logout,
  signIn,
  signupAndSignIn,
  uniqueAccount,
} from "./helpers";

test("@full recovery kit rotates credentials and restores encrypted data", async ({ page }) => {
  const account = await signupAndSignIn(page, uniqueAccount("recovery"));
  await createCollection(page, "Recovery acceptance space");
  await page.getByPlaceholder("Full name").fill("Recoverable Contact");
  await page.getByPlaceholder("Email").fill("recoverable@example.test");
  await page.getByRole("button", { name: "Add Contact" }).click();
  await expect(page.getByText("Contact encrypted and synced.")).toBeVisible();
  await logout(page);

  await page.getByRole("button", { name: "Sign In", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Sign In" });
  const newPassword = `Recovered-${crypto.randomUUID()}-Pass!`;
  await dialog.getByPlaceholder("Username").fill(account.username);
  await dialog.getByPlaceholder("24-word data recovery kit").fill(account.recoveryPhrase);
  await dialog.getByPlaceholder("New password", { exact: true }).fill(newPassword);
  await dialog.getByPlaceholder("Confirm new password").fill(newPassword);
  await dialog.getByRole("button", { name: "Recover Account" }).click();
  await expect(page.getByText(/Account recovery completed:/)).toBeVisible();

  await signIn(page, { username: account.username, password: newPassword });
  await page.getByRole("button", { name: "Sync Now" }).click();
  await expect(page.getByText("Recoverable Contact", { exact: true })).toBeVisible();
});
