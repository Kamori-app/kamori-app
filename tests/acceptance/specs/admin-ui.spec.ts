import { expect, test, type Page } from "@playwright/test";
import { decode, encode } from "@msgpack/msgpack";
import { uiHost } from "./uiHost";
let host: Awaited<ReturnType<typeof uiHost>>;
test.beforeAll(async () => { host = await uiHost("admin"); });
test.afterAll(async () => { await host?.close(); });
const options = new TextEncoder().encode(JSON.stringify({ challenge: "AQID", rpId: "localhost", userVerification: "required", allowCredentials: [{ id: "BAUG", type: "public-key" }] }));
const assertion = { id: "synthetic-key", rawId: "AQID", type: "public-key", response: { clientDataJSON: "BAUG", authenticatorData: "BwgJ", signature: "CgsM", userHandle: null }, clientExtensionResults: {} };
let recordedCalls: Call[] = [];
test.afterEach(async ({}, info) => {
  if (info.status !== info.expectedStatus) console.log("Admin HTTP boundary evidence:", JSON.stringify(recordedCalls, (_key, value) => value instanceof Uint8Array ? Array.from(value) : value));
  await info.attach("admin-http-boundaries", { contentType: "application/json", body: JSON.stringify(recordedCalls, (_key, value) => value instanceof Uint8Array ? Array.from(value) : value, 2) });
});
type Call = { path: string; method: string; body: any; authorization?: string; accept?: string; contentType?: string };
async function consoleUi(page: Page) {
  const calls: Call[] = [];
  recordedCalls = calls;
  let rejectTotp = false;
  let rejectMutation = false;
  let settingValue = true;
  let version = 2;
  let holdMutation = false;
  let releaseMutation: () => void = () => {};
  await page.addInitScript(() => {
    const state = window as any;
    state.authenticatorCalls = [];
    state.authenticatorMode = "ok";
    Object.defineProperty(navigator.credentials, "get", { configurable: true, value: async ({ publicKey }: any) => {
      state.authenticatorCalls.push({ challenge: Array.from(new Uint8Array(publicKey.challenge)), rpId: publicKey.rpId, userVerification: publicKey.userVerification, allowedId: Array.from(new Uint8Array(publicKey.allowCredentials[0].id)) });
      if (state.authenticatorMode === "cancel") throw new DOMException("Cancelled", "NotAllowedError");
      if (state.authenticatorMode === "hold") await new Promise(resolve => { state.releaseAssertion = resolve; });
      return { id: "synthetic-key", rawId: new Uint8Array([1, 2, 3]).buffer, type: "public-key", response: { clientDataJSON: new Uint8Array([4, 5, 6]).buffer, authenticatorData: new Uint8Array([7, 8, 9]).buffer, signature: new Uint8Array([10, 11, 12]).buffer, userHandle: null }, getClientExtensionResults: () => ({}) };
    }});
  });
  await page.route("**/admin-api/**", async route => {
    const request = route.request();
    const path = new URL(request.url()).pathname;
    const raw = request.postDataBuffer();
    const body = raw ? decode(raw) : undefined;
    const headers = request.headers();
    calls.push({ path, method: request.method(), body, authorization: headers.authorization, accept: headers.accept, contentType: headers["content-type"] });
    let response: any;
    let status = 200;
    if (path.endsWith("/start")) response = { flow_id: "flow-1", public_key_credential_request_options: options };
    else if (path === "/admin-api/auth/finish") {
      if (rejectTotp) { status = 401; response = { message: "Invalid TOTP", request_id: "synthetic-401" }; }
      else response = { token: "operator-session", expires_at_unix_ms: 2000000000000 };
    } else if (path === "/admin-api/auth/reauth/finish") response = { token: "one-use-reauth", expires_at_unix_ms: 2000000000000 };
    else if (path === "/admin-api/dashboard") response = { active_accounts: 3, suspended_accounts: 0, total_blob_storage_bytes: 42, pending_blobs: 0, pending_object_deletions: 0, registration_enabled: settingValue, beta_account_limit: 100, latest_migration: "test", jobs: [], security_keys: [{ id: "key-1", name: "Operator key", created_at_unix_ms: 1, last_used_at_unix_ms: null }] };
    else if (path === "/admin-api/settings" && request.method() === "GET") response = { settings: [{ key: "registration_enabled", value: settingValue, version, updated_at_unix_ms: null, overridden: true }] };
    else if (path === "/admin-api/audit") response = { entries: [] };
    else if (path === "/admin-api/settings") {
      if (holdMutation) await new Promise<void>(resolve => { releaseMutation = resolve; });
      if (rejectMutation) { status = 409; response = { message: "Setting version conflict", request_id: "synthetic-409" }; }
      else { settingValue = (body as any).value; version++; response = { changed: true }; }
    } else throw new Error(`Unexpected HTTP ${request.method()} ${path}`);
    await route.fulfill({ status, contentType: "application/msgpack", body: Buffer.from(encode(response)) });
  });
  await page.goto(host.origin);
  return { calls, rejectTotp: (value: boolean) => { rejectTotp = value; }, rejectMutation: (value: boolean) => { rejectMutation = value; }, holdMutation: () => { holdMutation = true; }, releaseMutation: () => releaseMutation() };
}
async function signIn(page: Page) {
  await page.getByLabel("Operator username").fill(" operator ");
  await page.getByLabel("Current TOTP", { exact: true }).fill(" 123456 ");
  await page.getByRole("button", { name: "Continue with passkey" }).click();
}
const writes = (calls: Call[]) => calls.filter(c => c.path === "/admin-api/settings" && c.method === "POST");

// Cancelling or rejecting the second factor must leave controls inaccessible and permit retry.
test("operator authentication waits for the authenticator and recovers from cancellation and invalid TOTP", async ({ page }) => {
  const http = await consoleUi(page);
  await page.evaluate(() => { (window as any).authenticatorMode = "cancel"; });
  await signIn(page);
  await expect(page.getByText(/Sign-in failed: The passkey request was cancelled/)).toBeVisible();
  expect(http.calls.map(c => c.path)).toEqual(["/admin-api/auth/start"]);
  await expect(page.getByRole("heading", { name: "Runtime controls" })).toHaveCount(0);
  await page.evaluate(() => { (window as any).authenticatorMode = "hold"; });
  http.rejectTotp(true);
  await signIn(page);
  await expect.poll(() => page.evaluate(() => (window as any).authenticatorCalls.length)).toBe(2);
  await expect(page.getByRole("button", { name: "Continue with passkey" })).toBeDisabled();
  expect(http.calls.filter(c => c.path.endsWith("/finish"))).toEqual([]);
  await page.evaluate(() => (window as any).releaseAssertion());
  await expect(page.getByText("Sign-in failed: Invalid TOTP (reference synthetic-401)")).toBeVisible();
  await expect(page.getByRole("button", { name: "Continue with passkey" })).toBeEnabled();
  http.rejectTotp(false);
  await page.evaluate(() => { (window as any).authenticatorMode = "ok"; });
  await signIn(page);
  await expect(page.getByRole("heading", { name: "Runtime controls" })).toBeVisible();
  await expect(page.getByLabel("Current TOTP", { exact: true })).toHaveCount(0);
  expect(http.calls[0]).toMatchObject({ path: "/admin-api/auth/start", method: "POST", body: { username: "operator" }, accept: "application/msgpack", contentType: "application/msgpack" });
  const finish = http.calls.find(c => c.path === "/admin-api/auth/finish")!;
  expect(finish.body).toEqual({ username: "operator", flow_id: "flow-1", credential: expect.any(Uint8Array), totp_code: "123456" });
  expect(JSON.parse(new TextDecoder().decode(finish.body.credential))).toEqual(assertion);
  expect(await page.evaluate(() => (window as any).authenticatorCalls[0])).toEqual({ challenge: [1, 2, 3], rpId: "localhost", userVerification: "required", allowedId: [4, 5, 6] });
  expect(http.calls.filter(c => ["/admin-api/dashboard", "/admin-api/settings", "/admin-api/audit"].includes(c.path)).every(c => c.authorization === "Bearer operator-session")).toBe(true);
});

test("setting mutation requires confirmation and a fresh ceremony, exposes pending, failure, and successful refresh", async ({ page }) => {
  const http = await consoleUi(page);
  await signIn(page);
  await expect(page.getByRole("heading", { name: "Runtime controls" })).toBeVisible();
  const apply = page.getByRole("button", { name: "Apply", exact: true });
  await page.getByLabel("registration_enabled", { exact: true }).fill("false");
  await page.getByLabel("Fresh TOTP for next action").fill("654321");
  await page.getByLabel("Reason (10–500 characters)").fill("Close registration for maintenance");
  await apply.click();
  await expect(page.getByText(/Setting update failed: .*confirmation/)).toBeVisible();
  expect(writes(http.calls)).toEqual([]);
  expect(http.calls.filter(c => c.path.includes("/reauth/"))).toEqual([]);
  await page.getByLabel("Typed setting confirmation").fill("SET registration_enabled");
  await page.getByLabel("Fresh TOTP for next action").fill("");
  await apply.click();
  await expect(page.getByText(/Current operator TOTP is required/)).toBeVisible();
  expect(writes(http.calls)).toEqual([]);
  await page.getByLabel("Fresh TOTP for next action").fill(" 654321 ");
  await page.evaluate(() => { (window as any).authenticatorMode = "hold"; });
  http.holdMutation(); http.rejectMutation(true);
  await apply.click();
  await expect.poll(() => page.evaluate(() => (window as any).authenticatorCalls.length)).toBe(2);
  await expect(apply).toBeDisabled();
  expect(writes(http.calls)).toEqual([]);
  expect(http.calls.filter(c => c.path === "/admin-api/auth/reauth/finish")).toEqual([]);
  await page.evaluate(() => (window as any).releaseAssertion());
  await expect.poll(() => writes(http.calls).length).toBe(1);
  await expect(apply).toBeDisabled();
  const write = writes(http.calls)[0];
  expect(write).toMatchObject({ authorization: "Bearer operator-session", contentType: "application/msgpack", body: { key: "registration_enabled", value: false, expected_version: 2, reauth_token: "one-use-reauth", reason: "Close registration for maintenance", confirmation: "SET registration_enabled" } });
  const reauth = http.calls.find(c => c.path === "/admin-api/auth/reauth/finish")!;
  expect(reauth.body.totp_code).toBe("654321");
  expect(JSON.parse(new TextDecoder().decode(reauth.body.credential))).toEqual(assertion);
  http.releaseMutation();
  await expect(page.getByText("Setting update failed: Setting version conflict (reference synthetic-409)")).toBeVisible();
  await expect(apply).toBeEnabled();
  await expect(page.getByLabel("Fresh TOTP for next action")).toHaveValue("");
  await expect(page.getByLabel("Typed setting confirmation")).toHaveValue("SET registration_enabled");
  await page.evaluate(() => { (window as any).authenticatorMode = "ok"; });
  await page.getByLabel("Fresh TOTP for next action").fill("987654");
  http.rejectMutation(false);
  await apply.click();
  await expect.poll(() => writes(http.calls).length).toBe(2);
  http.releaseMutation();
  await expect(page.getByText("registration_enabled updated and audited.")).toBeVisible();
  await expect(page.getByText(/v3 · database override/)).toBeVisible();
  await expect(page.getByLabel("registration_enabled", { exact: true })).toHaveValue("false");
  await expect(page.getByLabel("Typed setting confirmation")).toHaveValue("");
  await expect(page.getByLabel("Reason (10–500 characters)")).toHaveValue("");
  expect(http.calls.filter(c => c.path === "/admin-api/auth/reauth/finish").map(c => c.body.totp_code)).toEqual(["654321", "987654"]);
});
