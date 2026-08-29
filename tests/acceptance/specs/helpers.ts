import { expect, type Page } from "@playwright/test";

export interface AcceptanceAccount {
  username: string;
  password: string;
  recoveryPhrase: string;
}

export type AppSection = "Today" | "Tasks" | "Calendar" | "Contacts" | "Spaces";

export const openAppSection = async (
  page: Page,
  section: AppSection,
): Promise<void> => {
  await page
    .getByRole("link", { name: section, exact: true })
    .filter({ visible: true })
    .click();
};

export const uniqueAccount = (prefix: string): AcceptanceAccount => ({
  username: `${prefix}_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`,
  password: `Kamori-${crypto.randomUUID()}-Pass!`,
  recoveryPhrase: "",
});

export const signup = async (
  page: Page,
  account: AcceptanceAccount,
): Promise<AcceptanceAccount> => {
  await page.goto("/app/sign-up");
  const surface = page.getByRole("region", { name: "Create account" });
  await expect(surface).toBeVisible();
  await surface.getByPlaceholder("Username").fill(account.username);
  await surface
    .getByPlaceholder("Password", { exact: true })
    .fill(account.password);
  await surface.getByPlaceholder("Confirm password").fill(account.password);
  await surface.getByRole("button", { name: "Create account" }).click();

  const words = surface.locator("ol li");
  await expect(words).toHaveCount(24);
  const recoveryWords = (await words.allTextContents()).map((entry) =>
    entry.replace(/^\d+\.\s*/, "").trim(),
  );
  const recoveryPhrase = recoveryWords.join(" ");
  await expect(
    surface.getByRole("button", { name: "Download recovery file" }),
  ).toBeVisible();
  await surface.getByPlaceholder(/24th word/).fill(recoveryWords.at(-1) ?? "");
  await surface
    .getByRole("button", { name: "I saved the kit — create account" })
    .click();
  await expect(page.getByRole("region", { name: "Sign in" })).toBeVisible();
  return { ...account, recoveryPhrase };
};

export const signIn = async (
  page: Page,
  account: Pick<AcceptanceAccount, "username" | "password">,
  options: { allowLocalUnlock?: boolean } = {},
): Promise<void> => {
  let surface = page.getByRole("region", { name: "Sign in" });
  if (!(await surface.isVisible())) {
    await page.goto("/app/sign-in");
    surface = page.getByRole("region", { name: "Sign in" });
  }
  await expect(surface).toBeVisible();
  await surface.getByPlaceholder("Username").fill(account.username);
  await surface
    .getByPlaceholder("Password", { exact: true })
    .fill(account.password);
  if (options.allowLocalUnlock) {
    await surface.getByRole("checkbox").check();
  }
  await surface.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(page.getByRole("button", { name: "Log out" })).toBeVisible();
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
  await openAppSection(page, "Spaces");
  await page.getByPlaceholder("Space name").fill(name);
  await page.getByRole("button", { name: "Create Space" }).click();
  await expect(page.getByText(`Space "${name}" created.`)).toBeVisible();
  const collection = page
    .locator("[data-space-id]")
    .filter({ has: page.getByText(name, { exact: true }) });
  await expect(collection).toHaveCount(1);
  const id = await collection.getAttribute("data-space-id");
  if (!id) throw new Error("created collection has no space id");
  return id;
};

export const createTask = async (
  page: Page,
  title: string,
  options: { offline?: boolean } = {},
): Promise<void> => {
  await page.getByRole("button", { name: "New task", exact: true }).click();
  const editor = page.getByRole("dialog", { name: "New task" });
  await editor.getByRole("textbox", { name: "Title" }).fill(title);
  await editor.getByRole("button", { name: "Save task" }).click();
  await expect(
    page.getByText(
      options.offline
        ? "Changes saved to the encrypted offline outbox."
        : "Changes encrypted and synced.",
    ),
  ).toBeVisible();
  await expect(page.getByText(title, { exact: true })).toBeVisible();
};

export const createContact = async (
  page: Page,
  displayName: string,
  email: string,
): Promise<void> => {
  await page.getByRole("button", { name: "New contact", exact: true }).click();
  const editor = page.getByRole("dialog", { name: "New contact" });
  await editor.getByRole("textbox", { name: "Display name" }).fill(displayName);
  await editor.getByPlaceholder("name@example.com").fill(email);
  await editor.getByRole("button", { name: "Save contact" }).click();
  await expect(page.getByText("Changes encrypted and synced.")).toBeVisible();
  await expect(page.getByText(displayName, { exact: true })).toBeVisible();
};

export const syncNow = async (page: Page): Promise<void> => {
  await page.getByRole("button", { name: "Sync now", exact: true }).click();
  await expect(page.getByText(/Sync completed:/)).toBeVisible();
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
    if (!token)
      throw new Error("no authenticated browser request was observed");
    return token;
  };
};

export const logout = async (page: Page): Promise<void> => {
  await page.getByRole("button", { name: "Log out" }).click();
  await expect(page.getByRole("region", { name: "Sign in" })).toBeVisible();
};
