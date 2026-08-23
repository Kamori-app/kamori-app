import { expect, type Page } from "@playwright/test";

export interface AcceptanceAccount {
  username: string;
  password: string;
  recoveryPhrase: string;
}

export const uniqueAccount = (prefix: string): AcceptanceAccount => ({
  username: `${prefix}_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`,
  password: `Kamori-${crypto.randomUUID()}-Pass!`,
  recoveryPhrase: "",
});

export const signup = async (
  page: Page,
  account: AcceptanceAccount,
): Promise<AcceptanceAccount> => {
  await page.goto("/app?start=signup");
  const dialog = page.getByRole("dialog", { name: "Create account" });
  await expect(dialog).toBeVisible();
  await dialog.getByPlaceholder("Username").fill(account.username);
  await dialog.getByPlaceholder("Password", { exact: true }).fill(account.password);
  await dialog.getByPlaceholder("Confirm password").fill(account.password);
  await dialog.getByRole("button", { name: "Create account" }).click();

  const words = dialog.locator("ol li");
  await expect(words).toHaveCount(24);
  const recoveryWords = (await words.allTextContents()).map((entry) =>
    entry.replace(/^\d+\.\s*/, "").trim(),
  );
  const recoveryPhrase = recoveryWords.join(" ");
  await dialog
    .getByPlaceholder("Type word 24 to confirm")
    .fill(recoveryWords.at(-1) ?? "");
  await dialog
    .getByRole("button", { name: "I saved the kit — create account" })
    .click();
  await expect(page.getByRole("dialog", { name: "Sign in" })).toBeVisible();
  return { ...account, recoveryPhrase };
};

export const signIn = async (
  page: Page,
  account: Pick<AcceptanceAccount, "username" | "password">,
  options: { allowLocalUnlock?: boolean } = {},
): Promise<void> => {
  let dialog = page.getByRole("dialog", { name: "Sign in" });
  if (!(await dialog.isVisible())) {
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    dialog = page.getByRole("dialog", { name: "Sign in" });
  }
  await dialog.getByPlaceholder("Username").fill(account.username);
  await dialog.getByPlaceholder("Password", { exact: true }).fill(account.password);
  if (options.allowLocalUnlock) {
    await dialog.getByRole("checkbox").check();
  }
  await dialog.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(page.getByText("Authenticated", { exact: true })).toBeVisible();
};

export const signupAndSignIn = async (
  page: Page,
  account: AcceptanceAccount,
): Promise<AcceptanceAccount> => {
  const registered = await signup(page, account);
  await signIn(page, registered);
  return registered;
};

export const createCollection = async (
  page: Page,
  name: string,
): Promise<string> => {
  await page.getByPlaceholder("Collection name").fill(name);
  await page.getByRole("button", { name: "Create Collection" }).click();
  await expect(page.getByText(`Collection "${name}" created.`)).toBeVisible();
  const collection = page
    .locator("[data-space-id]")
    .filter({ has: page.getByText(name, { exact: true }) });
  await expect(collection).toHaveCount(1);
  const id = await collection.getAttribute("data-space-id");
  if (!id) throw new Error("created collection has no space id");
  return id;
};

export const captureAccessToken = (page: Page): (() => string) => {
  let token = "";
  page.on("request", (request) => {
    const authorization = request.headers().authorization ?? "";
    if (authorization.startsWith("Bearer ")) {
      token = authorization.slice("Bearer ".length);
    }
  });
  return () => {
    if (!token) throw new Error("no authenticated browser request was observed");
    return token;
  };
};

export const logout = async (page: Page): Promise<void> => {
  await page.getByRole("button", { name: "Log out" }).click();
  await expect(page.getByText("Not signed in", { exact: true })).toBeVisible();
};
