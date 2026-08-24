import { expect, test } from "@playwright/test";

import {
  createCollection,
  openAppSection,
  signupAndSignIn,
  syncNow,
  uniqueAccount,
} from "./helpers";

const inviteCodePattern = /^[A-Z0-9]{4}(?:-[A-Z0-9]{4}){3}$/;

test("@full editor sharing, single-use invites, and reader admission", async ({ browser }) => {
  const webUrl = process.env.KAMORI_ACCEPTANCE_WEB_URL ?? "http://127.0.0.1:14173";
  const ownerContext = await browser.newContext({ baseURL: webUrl });
  const editorContext = await browser.newContext({ baseURL: webUrl });
  const readerContext = await browser.newContext({ baseURL: webUrl });
  try {
    const owner = await ownerContext.newPage();
    await signupAndSignIn(owner, uniqueAccount("owner"));
    await createCollection(owner, "Shared acceptance space");
    await openAppSection(owner, "Tasks");
    await owner.getByPlaceholder("New task").fill("Owner shared task");
    await owner.getByRole("button", { name: "Add Task" }).click();
    await expect(owner.getByText("Task encrypted and synced.")).toBeVisible();

    await openAppSection(owner, "Spaces");
    await owner.getByRole("link", { name: "Members and access" }).click();
    await owner.getByLabel("Invite role").selectOption("editor");
    await owner.getByLabel("Invite expiry").selectOption("15");
    await owner.getByPlaceholder("Optional encrypted note for recipient").fill("editor acceptance note");
    await owner.getByRole("button", { name: "Generate Invite Code" }).click();
    const editorCode = (await owner.getByText(inviteCodePattern, { exact: true }).textContent())?.trim();
    expect(editorCode).toMatch(inviteCodePattern);

    const editor = await editorContext.newPage();
    await signupAndSignIn(editor, uniqueAccount("editor"));
    await openAppSection(editor, "Spaces");
    await editor.getByRole("link", { name: "Redeem invite code" }).click();
    await editor.getByPlaceholder("ABCD-EFGH-JKLM-NPQR").fill(editorCode ?? "");
    await editor.getByRole("button", { name: "Redeem Code" }).click();
    await expect(editor.getByText(/Invite redeemed for space/)).toBeVisible();
    await expect(editor.getByText("editor acceptance note", { exact: true })).toBeVisible();
    await syncNow(editor);
    await openAppSection(editor, "Tasks");
    await expect(editor.getByText("Owner shared task", { exact: true })).toBeVisible();

    await editor.getByPlaceholder("New task").fill("Editor shared task");
    await editor.getByRole("button", { name: "Add Task" }).click();
    await expect(editor.getByText("Task encrypted and synced.")).toBeVisible();
    await syncNow(owner);
    await openAppSection(owner, "Tasks");
    await expect(owner.getByText("Editor shared task", { exact: true })).toBeVisible();

    await openAppSection(editor, "Spaces");
    await editor.getByRole("link", { name: "Redeem invite code" }).click();
    await editor.getByPlaceholder("ABCD-EFGH-JKLM-NPQR").fill(editorCode ?? "");
    await editor.getByRole("button", { name: "Redeem Code" }).click();
    await expect(editor.getByText(/Invite redemption failed:/)).toBeVisible();

    await openAppSection(owner, "Spaces");
    await owner.getByRole("link", { name: "Members and access" }).click();
    await owner.getByLabel("Invite role").selectOption("reader");
    await owner.getByRole("button", { name: "Generate Invite Code" }).click();
    const issuedCode = owner.getByText(inviteCodePattern, { exact: true });
    await expect(issuedCode).not.toHaveText(editorCode ?? "");
    const readerCode = (await issuedCode.textContent())?.trim();
    expect(readerCode).toMatch(inviteCodePattern);

    const reader = await readerContext.newPage();
    await signupAndSignIn(reader, uniqueAccount("reader"));
    await openAppSection(reader, "Spaces");
    await reader.getByRole("link", { name: "Redeem invite code" }).click();
    await reader.getByPlaceholder("ABCD-EFGH-JKLM-NPQR").fill(readerCode ?? "");
    await reader.getByRole("button", { name: "Redeem Code" }).click();
    await expect(reader.getByText(/Invite redeemed for space/)).toBeVisible();
    await syncNow(reader);
    await openAppSection(reader, "Tasks");
    await expect(reader.getByText("Owner shared task", { exact: true })).toBeVisible();
    await expect(reader.getByPlaceholder("New task")).toBeDisabled();
    await expect(reader.getByRole("button", { name: "Add Task" })).toBeDisabled();
    await expect(reader.getByText("Reader must not write", { exact: true })).toHaveCount(0);
  } finally {
    await Promise.all([
      ownerContext.close(),
      editorContext.close(),
      readerContext.close(),
    ]);
  }
});
