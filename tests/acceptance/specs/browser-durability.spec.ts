import { expect, test, type Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import type { AddressInfo } from "node:net";
import { decode, encode } from "@msgpack/msgpack";

// Real Chromium, Web Locks, HttpOnly cookies, IndexedDB, production auth client,
// AppWorkspace and WASM. Only the app shell and HTTP peer are synthetic.
const space = "00000000-0000-0000-0000-000000000041";
let server: ReturnType<typeof createServer>;
let origin: string;
let bundle: string;
let refreshes: { session: string; cookie: string; csrf: string; id: string }[];
let bootstraps: { session: string; cookie: string }[];
let rotations: Map<string, string>;
let failure: "none" | "lost" | "partial";
let holdRefresh: (() => Promise<void>) | undefined;
let online: boolean;
let loseAppend: boolean;
let appends: string[];
let operations: Map<string, { envelope: any; space_seq: number }>;
let devices: { device_id: string; signing_public_key: Uint8Array }[];
const reply = (response: ServerResponse, body: unknown, status = 200) => {
  response.writeHead(status, { "content-type": "application/msgpack" });
  response.end(encode(body));
};
const body = async (request: IncomingMessage): Promise<any> => {
  const chunks: Buffer[] = [];
  for await (const chunk of request) chunks.push(Buffer.from(chunk));
  return chunks.length ? decode(Buffer.concat(chunks)) : {};
};

test.beforeAll(async () => {
  const compiled = spawnSync("bun", [new URL("../fixtures/browserDurability.bundle.mjs", import.meta.url).pathname], {
    encoding: "utf8", maxBuffer: 16 * 1024 * 1024,
  });
  if (compiled.status !== 0) throw new Error(compiled.stderr);
  bundle = compiled.stdout;
  const wasm = readFileSync(new URL("../../../apps/web-frontend/src/lib/wasm/crypto-core-lib/crypto_core_lib_bg.wasm", import.meta.url));
  server = createServer(async (request, response) => {
    try {
      const url = new URL(request.url!, origin);
      if (url.pathname === "/fixture.js") {
        response.writeHead(200, { "content-type": "text/javascript" }); response.end(bundle); return;
      }
      if (url.pathname === "/crypto_core_lib_bg.wasm") {
        response.writeHead(200, { "content-type": "application/wasm" }); response.end(wasm); return;
      }
      if (url.pathname === "/") {
        response.writeHead(200, { "content-type": "text/html" });
        response.end('<html><body><script type="module" src="/fixture.js"></script></body></html>'); return;
      }
      const session = url.pathname.split("/")[1];
      const cookie = /(?:^|; )refresh=([^;]*)/.exec(request.headers.cookie ?? "")?.[1] ?? "A";
      if (url.pathname.endsWith("/auth/csrf")) {
        bootstraps.push({ session, cookie }); reply(response, { csrf_token: cookie }); return;
      }
      if (url.pathname.endsWith("/auth/refresh")) {
        const payload = await body(request);
        const csrf = request.headers["x-kamori-csrf-token"] as string;
        refreshes.push({ session, cookie, csrf, id: payload.rotation_request_id });
        if (csrf !== cookie) { reply(response, { message: "CSRF mismatch" }, 401); return; }
        if (holdRefresh) await holdRefresh();
        const key = `${session}:${cookie}:${payload.rotation_request_id}`;
        const next = rotations.get(key) ?? `${cookie}1`;
        rotations.set(key, next);
        const outcome = failure; failure = "none";
        const encoded = encode({ access_token: `access-${next}`, csrf_token: next });
        response.writeHead(200, { "content-type": "application/msgpack", "content-length": encoded.length,
          "connection": "close", ...(outcome === "lost" ? {} : { "set-cookie": `refresh=${next}; HttpOnly; SameSite=Strict; Path=/${session}` }) });
        response.end(outcome !== "none" ? encoded.slice(0, 2) : encoded); return;
      }
      if (!online) { reply(response, { message: "Synthetic offline network" }, 503); return; }
      if (url.pathname === "/pim/operations" && request.method === "POST") {
        const { envelope } = await body(request);
        appends.push(envelope.client_op_id);
        const duplicate = operations.has(envelope.client_op_id);
        const stored = operations.get(envelope.client_op_id) ?? { envelope, space_seq: operations.size + 1 };
        operations.set(envelope.client_op_id, stored);
        if (loseAppend) {
          loseAppend = false;
          response.writeHead(200, { "content-type": "application/msgpack", "content-length": 100, connection: "close" });
          response.end(new Uint8Array([0x81])); return;
        }
        reply(response, { accepted: true, duplicate, space_seq: stored.space_seq }); return;
      }
      if (url.pathname === "/pim/operations") {
        const since = Number(url.searchParams.get("since"));
        reply(response, { operations: [...operations.values()].filter(op => op.space_seq > since), next_cursor: operations.size }); return;
      }
      if (url.pathname === "/pim/spaces") {
        reply(response, { spaces: [{ space_id: space, key_epoch: 1, role: "editor", history_start_seq: 0,
          current_state_start_seq: 0, encrypted_metadata: new Uint8Array(), device_key_packages: [] }] }); return;
      }
      if (url.pathname.endsWith("/devices")) { reply(response, { devices }); return; }
      if (url.pathname === "/pim/spaces/trash") { reply(response, { spaces: [] }); return; }
      if (url.pathname === "/pim/spaces/recovery-key-packages") { reply(response, { packages: [] }); return; }
      if (url.pathname.endsWith("/device-key-packages") || url.pathname.endsWith("/recovery-key-package")) {
        reply(response, {}); return;
      }
      reply(response, { message: `Unexpected fixture request: ${request.method} ${url.pathname}` }, 404);
    } catch (error) {
      response.destroy(error as Error);
    }
  });
  await new Promise<void>(resolve => server.listen(0, "127.0.0.1", resolve));
  origin = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
});
test.afterAll(async () => {
  if (!server) return;
  server.closeAllConnections();
  await new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
});
test.afterEach(async ({}, info) => {
  const path = info.outputPath("durability-boundaries.json");
  writeFileSync(path, JSON.stringify({ refreshes, bootstraps, appends, distinctRotations: rotations?.size, distinctOperations: operations?.size }, null, 2));
  await info.attach("durability-boundaries", { path, contentType: "application/json" });
});
test.beforeEach(() => {
  refreshes = []; bootstraps = []; rotations = new Map(); failure = "none"; holdRefresh = undefined;
  online = false; loseAppend = false; appends = []; operations = new Map(); devices = [];
});
async function open(page: Page) {
  await page.goto(origin);
  await page.waitForFunction(() => Boolean((window as any).durability));
}
async function refresh(page: Page, session = "alice") {
  return page.evaluate(base => (window as any).durability.cloudApi.refresh(base), `${origin}/${session}`);
}
async function pendingAttempts(page: Page) {
  return page.evaluate(async () => {
    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDB.open("kamori-auth-runtime");
      request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error);
    });
    try {
      return await new Promise<any[]>((resolve, reject) => {
        const request = database.transaction("refresh-attempts").objectStore("refresh-attempts").getAll();
        request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error);
      });
    } finally { database.close(); }
  });
}

test("two real tabs serialize shared-cookie refresh and preserve an unrelated session", async ({ page, context }) => {
  await context.addCookies(["alice", "bob"].map(session => ({ name: "refresh", value: "A", domain: "127.0.0.1", path: `/${session}`, httpOnly: true, sameSite: "Strict" as const })));
  await open(page);
  const second = await context.newPage(); await open(second);
  let release!: () => void;
  const gate = new Promise<void>(resolve => { release = resolve; });
  holdRefresh = () => gate;
  const first = refresh(page);
  await expect.poll(() => refreshes.length).toBe(1);
  const next = refresh(second);
  await expect.poll(() => second.evaluate(async () => (await navigator.locks.query()).pending?.length)).toBe(1);
  expect(bootstraps).toEqual([{ session: "alice", cookie: "A" }]);
  expect(refreshes).toHaveLength(1);
  release(); holdRefresh = undefined;
  expect((await first).access_token).toBe("access-A1");
  expect((await next).access_token).toBe("access-A11");
  expect(refreshes.map(({ session, cookie, csrf }) => ({ session, cookie, csrf }))).toEqual([
    { session: "alice", cookie: "A", csrf: "A" }, { session: "alice", cookie: "A1", csrf: "A1" },
  ]);
  expect(refreshes[0].id).toMatch(/^[0-9a-f-]{36}$/);
  expect(refreshes[1].id).not.toBe(refreshes[0].id);
  expect(await pendingAttempts(page)).toEqual([]);
  expect((await context.cookies(`${origin}/bob`)).find(cookie => cookie.name === "refresh")?.value).toBe("A");
  expect((await refresh(second, "bob")).access_token).toBe("access-A1");
  expect(refreshes).toHaveLength(3);
  expect(refreshes[2].session).toBe("bob");
});

for (const outcome of ["lost", "partial"] as const) {
  test(`${outcome} refresh response survives page close and retries the correct cookie generation`, async ({ page, context }) => {
    await context.addCookies(["alice", "bob"].map(session => ({ name: "refresh", value: "A", domain: "127.0.0.1", path: `/${session}`, httpOnly: true, sameSite: "Strict" as const })));
    await open(page); failure = outcome;
    await expect(refresh(page)).rejects.toThrow();
    expect(refreshes).toHaveLength(1);
    const originalId = refreshes[0].id;
    expect((await pendingAttempts(page)).map(record => record.requestId)).toEqual([originalId]);
    expect((await context.cookies(`${origin}/alice`)).find(cookie => cookie.name === "refresh")?.value).toBe(outcome === "lost" ? "A" : "A1");
    await page.close();
    const reopened = await context.newPage(); await open(reopened);
    expect((await refresh(reopened)).access_token).toBe(outcome === "lost" ? "access-A1" : "access-A11");
    expect(refreshes).toHaveLength(2);
    expect(refreshes[1].cookie).toBe(outcome === "lost" ? "A" : "A1");
    expect(refreshes[1].csrf).toBe(refreshes[1].cookie);
    expect(refreshes[1].id === originalId).toBe(outcome === "lost");
    expect(rotations.size).toBe(outcome === "lost" ? 1 : 2);
    expect((await pendingAttempts(reopened)).map(record => record.requestId)).toEqual(outcome === "lost" ? [] : [originalId]);
    expect((await context.cookies(`${origin}/bob`)).find(cookie => cookie.name === "refresh")?.value).toBe("A");
    expect(refreshes.every(request => request.session === "alice")).toBe(true);
  });
}

async function start(page: Page, account = "alice", endpoint = `${origin}/pim`) {
  const device = await page.evaluate(args => (window as any).durability.start(args), { endpoint, account, space });
  if (!devices.some(item => item.device_id === device.deviceId)) {
    devices.push({ device_id: device.deviceId, signing_public_key: new Uint8Array(device.signingPublicKey) });
  }
  await expect(page.getByRole("button", { name: "New task", exact: true })).toBeVisible();
  // The real controller has completed initial hydration and its offline sync.
  await expect.poll(() => page.evaluate(() => (window as any).durability.sync.phase)).toBe("error");
}
async function save(page: Page, title: string) {
  await page.getByRole("button", { name: "New task", exact: true }).click();
  const editor = page.getByRole("dialog", { name: "New task" });
  await editor.getByRole("textbox", { name: "Title" }).fill(title);
  await editor.getByRole("button", { name: "Save task" }).click();
  await expect(editor).not.toBeVisible();
  await expect(page.getByText(title, { exact: true })).toBeVisible();
}
async function snapshot(page: Page) {
  return page.evaluate(async () => ({
    state: await (window as any).durability.vault.loadMaterializedPimState(),
    queue: await (window as any).durability.vault.listQueuedOperationEnvelopes(),
  }));
}

test("acknowledged edits from concurrent tabs stay queued and materialized after both pages close", async ({ page, context }) => {
  await open(page); await start(page);
  const second = await context.newPage(); await open(second); await start(second);
  await Promise.all([save(page, "First tab edit"), save(second, "Second tab edit")]);
  const saved = await snapshot(page);
  expect(saved.queue).toHaveLength(2);
  expect(saved.state.items.map((item: any) => item.title).sort()).toEqual(["First tab edit", "Second tab edit"]);
  expect(saved.state.operations).toHaveLength(2);
  await page.close(); await second.close();
  const reopened = await context.newPage(); await open(reopened); await start(reopened);
  for (const title of ["First tab edit", "Second tab edit"]) await expect(reopened.getByText(title, { exact: true })).toBeVisible();
  const restored = await snapshot(reopened);
  expect(restored.state).toEqual(saved.state);
  expect(restored.queue.map((operation: any) => operation.client_op_id)).toEqual(saved.queue.map((operation: any) => operation.client_op_id));
});

test("single-tab durable edits stay account and endpoint scoped across unlock and page reopen", async ({ page, context }) => {
  await open(page); await start(page); await save(page, "Alice durable edit");
  const alice = await snapshot(page);
  expect(alice.queue).toHaveLength(1);
  await start(page, "bob");
  expect((await snapshot(page)).queue).toEqual([]);
  expect((await snapshot(page)).state.items).toEqual([]);
  await expect(page.getByText("Alice durable edit", { exact: true })).not.toBeVisible();
  await save(page, "Bob durable edit");
  await start(page, "alice", `${origin}/other-endpoint`);
  expect((await snapshot(page)).queue).toEqual([]);
  expect((await snapshot(page)).state.items).toEqual([]);
  await start(page);
  expect((await snapshot(page)).state).toEqual(alice.state);
  await expect(page.getByText("Bob durable edit", { exact: true })).not.toBeVisible();
  await page.close();
  const reopened = await context.newPage(); await open(reopened); await start(reopened);
  expect((await snapshot(reopened)).state).toEqual(alice.state);
  expect((await snapshot(reopened)).queue).toEqual(alice.queue);
});

test("concurrent tabs keep edits isolated while one tab switches account", async ({ page, context }) => {
  await open(page); await start(page);
  const second = await context.newPage(); await open(second); await start(second, "bob");
  await Promise.all([save(page, "Alice private task"), save(second, "Bob private task")]);
  const alice = await snapshot(page); const bob = await snapshot(second);
  expect(alice.state.items.map((item: any) => item.title)).toEqual(["Alice private task"]);
  expect(bob.state.items.map((item: any) => item.title)).toEqual(["Bob private task"]);
  expect(alice.queue).toHaveLength(1); expect(bob.queue).toHaveLength(1);
  await start(second, "alice");
  expect((await snapshot(second)).state).toEqual(alice.state);
  await expect(second.getByText("Bob private task", { exact: true })).not.toBeVisible();
  await save(page, "Alice second task");
  await start(second, "bob");
  expect((await snapshot(second)).state).toEqual(bob.state);
  expect((await snapshot(second)).queue).toEqual(bob.queue);
  expect((await snapshot(page)).state.items.map((item: any) => item.title).sort()).toEqual(["Alice private task", "Alice second task"]);
});

test("lost sync acknowledgement is retried idempotently through the production controller", async ({ page }) => {
  await open(page); await start(page);
  online = true; loseAppend = true;
  await save(page, "Ack survives retry");
  expect(appends).toHaveLength(1);
  const lost = await snapshot(page);
  expect(lost.queue).toHaveLength(1);
  expect(lost.queue[0].client_op_id).toBe(appends[0]);
  await page.evaluate(() => (window as any).durability.requestManualSync());
  await expect.poll(async () => (await snapshot(page)).queue.length).toBe(0);
  expect(appends).toEqual([lost.queue[0].client_op_id, lost.queue[0].client_op_id]);
  expect(operations.size).toBe(1);
  const synced = await snapshot(page);
  expect(synced.state.items.map((item: any) => item.title)).toEqual(["Ack survives retry"]);
  expect(synced.state.operations).toHaveLength(1);
  expect(synced.state.operations[0].spaceSeq).toBe(1);
  expect(synced.state.cursors[space]).toBe(1);
  const previousSync = await page.evaluate(() => (window as any).durability.sync.lastSuccessAt);
  await page.evaluate(() => (window as any).durability.requestManualSync());
  await expect.poll(() => page.evaluate(() => (window as any).durability.sync.lastSuccessAt)).toBeGreaterThan(previousSync);
  expect((await snapshot(page)).state).toEqual(synced.state);
  expect(appends).toHaveLength(2);
});
