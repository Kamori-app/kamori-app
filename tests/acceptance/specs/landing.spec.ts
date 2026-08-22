import { expect, test } from "@playwright/test";

test("@smoke landing language and section navigation stay aligned", async ({ page }) => {
  const response = await page.goto("/");
  const contentSecurityPolicy = response?.headers()["content-security-policy"] ?? "";
  expect(contentSecurityPolicy).toContain("nonce-");
  expect(contentSecurityPolicy).toContain("https://api.github.com");

  const navigation = page.getByRole("navigation", { name: "Primary navigation" });
  expect(await navigation.getByRole("link").allTextContents()).toEqual([
    "Product",
    "How it works",
    "Apps",
    "Security",
    "Questions",
  ]);
  await expect(navigation.getByRole("link", { name: "Product" })).toHaveAttribute("href", "#product");
  await expect(navigation.getByRole("link", { name: "How it works" })).toHaveAttribute(
    "href",
    "#how-it-works",
  );
  await expect(navigation.getByRole("link", { name: "Apps" })).toHaveAttribute("href", "#apps");
  await expect(navigation.getByRole("link", { name: "Security" })).toHaveAttribute(
    "href",
    "#security",
  );
  await expect(navigation.getByRole("link", { name: "Questions" })).toHaveAttribute(
    "href",
    "#questions",
  );

  await page.getByRole("link", { name: "RU", exact: true }).click();
  await expect(page.locator("html")).toHaveAttribute("lang", "ru");
  await expect(page.locator("h1")).toContainText("Календарь");
  await expect(page).toHaveURL(/\?lang=ru$/);
  await expect
    .poll(() => page.evaluate(() => localStorage.getItem("kamori.locale")))
    .toBe("ru");

  await page.reload();
  await expect(page.locator("h1")).toContainText("Календарь");
  await page.getByRole("link", { name: "EN", exact: true }).click();
  await expect(page.locator("html")).toHaveAttribute("lang", "en");
  await expect(page.locator("h1")).toContainText("Your calendar");
});
