import { expect, test, type Page } from "@playwright/test";
import { uiHost } from "./uiHost";
let host: Awaited<ReturnType<typeof uiHost>>;
test.beforeAll(async () => { host = await uiHost("desktop"); });
test.afterAll(async () => { await host?.close(); });

type Call = { command: string; payload?: any };
async function desktop(page: Page, restored = false, running = false) {
  const calls: Call[] = [];
  const failures = new Map<string, string>();
  const pending = new Map<string, { resolve: (value?: any) => void; reject: (error: Error) => void }>();
  const holds = new Set<string>();
  let collections = [{ id: "space-1", name: "Cached personal", synced_items: 4 }];
  await page.exposeFunction("uiIpc", async (call: Call) => {
    calls.push(call);
    if (failures.has(call.command)) throw new Error(failures.get(call.command));
    if (holds.has(call.command)) return new Promise((resolve, reject) => pending.set(call.command, { resolve, reject }));
    switch (call.command) {
      case "configure_backend": case "apply_window_preferences": return;
      case "restore_session": return restored;
      case "password_login": restored = true; return { totp_verified: true };
      case "dashboard_snapshot": return { has_access_token: restored, server: { running, bind_addr: "127.0.0.1:8181" }, collections_total: collections.length, synced_items_total: 4 };
      case "start_local_server": running = true; return { running, bind_addr: "127.0.0.1:8181" };
      case "stop_local_server": running = false; return { running, bind_addr: "127.0.0.1:8181" };
      case "sync_now": return 3;
      case "list_collections": return collections;
      case "create_collection": {
        const collection = { id: "space-2", name: call.payload.name, synced_items: 0 };
        collections = [...collections, collection]; return collection;
      }
      default: throw new Error(`Unexpected IPC ${call.command}`);
    }
  });
  await page.goto(host.origin);
  await expect.poll(() => calls.filter(c => c.command === "dashboard_snapshot").length).toBeGreaterThan(0);
  return { calls, failures, holds, pending };
}

test("desktop is sign-in-only, blocks blank credentials, and recovers after a rejected sign-in", async ({ page }) => {
  const ipc = await desktop(page);
  await expect(page.getByRole("heading", { name: "Sign in", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: /register|sign up|create account/i })).toHaveCount(0);
  await expect(page.getByRole("link", { name: /register|sign up|create account/i })).toHaveCount(0);
  await page.locator("form input").nth(0).fill("   ");
  await page.locator("form input").nth(1).fill("synthetic-password");
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(page.getByText("Enter your username to continue.")).toBeVisible();
  expect(ipc.calls.filter(c => c.command === "password_login")).toEqual([]);
  await page.locator("form input").nth(0).fill("  alice  ");
  await page.locator("form input").nth(2).fill(" 123456 ");
  ipc.holds.add("password_login");
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(page.getByRole("button", { name: "Signing in…" })).toBeDisabled();
  await expect(page.getByRole("button", { name: "Continue in browser" })).toBeDisabled();
  expect(ipc.calls.at(-1)).toEqual({ command: "password_login", payload: { username: "alice", password: "synthetic-password", totpCode: "123456" } });
  ipc.pending.get("password_login")!.reject(new Error("Invalid second factor"));
  await expect(page.getByText("Sign-in failed: Invalid second factor")).toBeVisible();
  await expect(page.getByRole("button", { name: "Sign in", exact: true })).toBeEnabled();
  ipc.holds.clear();
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Desktop control center" })).toBeVisible();
  await expect(page.getByText("Signed in with password.")).toBeVisible();
  expect(ipc.calls.filter(c => c.command === "password_login")).toHaveLength(2);
});

test("restored desktop preserves cached controls through offline sync failure and recovery", async ({ page }) => {
  const ipc = await desktop(page, true, true);
  await expect(page.getByRole("heading", { name: "Desktop control center" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Start DAV bridge", exact: true })).toBeDisabled();
  await expect(page.getByRole("button", { name: "Stop DAV bridge", exact: true })).toBeEnabled();
  ipc.failures.set("sync_now", "Synthetic offline network");
  await page.getByRole("button", { name: "Sync now", exact: true }).click();
  await expect(page.getByText("Synthetic offline network", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Sync now", exact: true })).toBeEnabled();
  await page.getByRole("button", { name: "Spaces", exact: true }).click();
  await expect(page.getByText("Cached personal", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Dashboard", exact: true }).click();
  ipc.failures.clear();
  await page.getByRole("button", { name: "Sync now", exact: true }).click();
  await expect(page.getByText("Sync complete: applied 3 encrypted operations.")).toBeVisible();
  await page.getByRole("button", { name: "Stop DAV bridge", exact: true }).click();
  await expect(page.getByRole("button", { name: "Start DAV bridge", exact: true })).toBeEnabled();
  await expect(page.getByRole("button", { name: "Sync now", exact: true })).toBeDisabled();
  expect(ipc.calls.filter(c => ["sync_now", "stop_local_server"].includes(c.command))).toEqual([
    { command: "sync_now", payload: {} }, { command: "sync_now", payload: {} }, { command: "stop_local_server", payload: {} },
  ]);
  expect(ipc.calls.slice(0, 3)).toEqual([
    { command: "configure_backend", payload: { cloudBaseUrl: "https://api.kamori.app" } },
    { command: "restore_session", payload: {} },
    { command: "apply_window_preferences", payload: { closeBehavior: "quit", showTrayIcon: false } },
  ]);
});

test("desktop space creation rejects whitespace and sends a trimmed name before refreshing the visible list", async ({ page }) => {
  const ipc = await desktop(page, true);
  await page.getByRole("button", { name: "Spaces", exact: true }).click();
  await expect(page.getByText("Cached personal", { exact: true })).toBeVisible();
  await page.getByPlaceholder("Personal", { exact: true }).fill("   ");
  await page.getByRole("button", { name: "Create space", exact: true }).click();
  expect(ipc.calls.filter(c => c.command === "create_collection")).toEqual([]);
  await page.getByPlaceholder("Personal", { exact: true }).fill("  Work calendar  ");
  await page.getByRole("button", { name: "Create space", exact: true }).click();
  await expect(page.getByText("Work calendar", { exact: true })).toBeVisible();
  await expect(page.getByPlaceholder("Personal", { exact: true })).toHaveValue("");
  expect(ipc.calls.filter(c => c.command === "create_collection")).toEqual([
    { command: "create_collection", payload: { name: "Work calendar" } },
  ]);
});
