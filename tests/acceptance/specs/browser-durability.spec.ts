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
let holdAppend: (() => Promise<void>) | undefined;
let holdRefresh: (() => Promise<void>) | undefined;
let online: boolean;
let loseAppend: boolean;
let appends: string[];
let appendAuthorizations: string[];
let appendStatus: number;
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
        appendAuthorizations.push(request.headers.authorization ?? "");
        const duplicate = operations.has(envelope.client_op_id);
        const stored = operations.get(envelope.client_op_id) ?? { envelope, space_seq: operations.size + 1 };
        operations.set(envelope.client_op_id, stored);
        if (loseAppend) {
          loseAppend = false;
          response.writeHead(200, { "content-type": "application/msgpack", "content-length": 100, connection: "close" });
          response.end(new Uint8Array([0x81])); return;
        }
        if (holdAppend) await holdAppend();
        if (appendStatus !== 200) { reply(response, { message: "Synthetic expired access" }, appendStatus); return; }
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
  writeFileSync(path, JSON.stringify({ refreshes, bootstraps, appends, appendAuthorizations, distinctRotations: rotations?.size, distinctOperations: operations?.size }, null, 2));
  await info.attach("durability-boundaries", { path, contentType: "application/json" });
});
test.beforeEach(() => {
  refreshes = []; bootstraps = []; rotations = new Map(); failure = "none"; holdRefresh = undefined;
  holdAppend = undefined; online = false; loseAppend = false; appends = []; appendAuthorizations = []; appendStatus = 200; operations = new Map(); devices = [];
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

async function start(page: Page, account = "alice", endpoint = `${origin}/pim`, keyByte = 7) {
  const device = await page.evaluate(args => (window as any).durability.start(args), { endpoint, account, space, keyByte });
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

test("a save waiting on the data-plane lock cannot cross an account switch", async ({ page, context }) => {
  await open(page); await start(page); await save(page, "Alice committed before switch");
  const committed = await snapshot(page);
  expect(committed.queue).toHaveLength(1);
  // Reopen to leave only the admitted save in the next lock's wait queue.
  await open(page); await start(page);
  const blocker = await context.newPage(); await open(blocker);
  await blocker.evaluate(() => {
    (window as any).blockHeld = false;
    void navigator.locks.request("kamori-web-data-plane-v1", async () => {
      (window as any).blockHeld = true;
      await new Promise<void>(resolve => { (window as any).releaseBlock = resolve; });
    });
  });
  await expect.poll(() => blocker.evaluate(() => (window as any).blockHeld)).toBe(true);
  await page.getByRole("button", { name: "New task", exact: true }).click();
  const editor = page.getByRole("dialog", { name: "New task" });
  await editor.getByRole("textbox", { name: "Title" }).fill("Alice pending at account switch");
  await editor.getByRole("button", { name: "Save task" }).click();
  await expect.poll(() => blocker.evaluate(async () => (await navigator.locks.query()).pending?.length)).toBe(1);
  // Same shell lifecycle as normal sign-out/sign-in: destroy, lock, unlock, mount.
  await page.evaluate(args => (window as any).durability.start(args), { endpoint: `${origin}/pim`, account: "bob", space });
  await blocker.evaluate(() => (window as any).releaseBlock());
  await expect.poll(() => page.evaluate(() => (window as any).durability.sync.phase)).toBe("error");
  const bob = await snapshot(page);
  expect(bob.state.items).toEqual([]);
  expect(bob.state.operations).toEqual([]);
  expect(bob.queue).toEqual([]);
  await expect(page.getByText("Alice pending at account switch", { exact: true })).not.toBeVisible();
  await start(page, "alice");
  expect((await snapshot(page)).state).toEqual(committed.state);
  expect((await snapshot(page)).queue).toEqual(committed.queue);
});

test("locking and reopening the same vault invalidates a waiting save", async ({ page, context }) => {
  await open(page); await start(page);
  const before = await snapshot(page);
  const blocker = await context.newPage(); await open(blocker);
  await blocker.evaluate(() => {
    (window as any).blockHeld = false;
    void navigator.locks.request("kamori-web-data-plane-v1", async () => {
      (window as any).blockHeld = true;
      await new Promise<void>(resolve => { (window as any).releaseBlock = resolve; });
    });
  });
  await expect.poll(() => blocker.evaluate(() => (window as any).blockHeld)).toBe(true);
  await page.getByRole("button", { name: "New task", exact: true }).click();
  const editor = page.getByRole("dialog", { name: "New task" });
  await editor.getByRole("textbox", { name: "Title" }).fill("Old unlock lifetime task");
  await editor.getByRole("button", { name: "Save task" }).click();
  await expect.poll(() => blocker.evaluate(async () => (await navigator.locks.query()).pending?.length)).toBe(1);
  await page.evaluate(async endpoint => {
    const vault = (window as any).durability.vault;
    vault.lockWebVault();
    await vault.unlockOrCreateWebVault(endpoint, "alice", new Uint8Array(32).fill(7));
  }, `${origin}/pim`);
  await blocker.evaluate(() => (window as any).releaseBlock());
  await expect(page.getByRole("button", { name: "Saving…", exact: true })).toHaveCount(0);
  expect((await snapshot(page)).state).toEqual(before.state);
  expect((await snapshot(page)).queue).toEqual(before.queue);
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


test("@smoke an admitted Alice acknowledgement cannot overwrite Bob after a distinct-key account switch", async ({ page }, info) => {
  await open(page); await start(page, "bob", `${origin}/pim`, 8);
  await save(page, "Bob durable sentinel");
  const bobBefore = await snapshot(page);
  await start(page); await save(page, "Alice private task");
  const aliceBefore = await snapshot(page);
  expect(bobBefore.queue).toHaveLength(1);
  expect(aliceBefore.queue).toHaveLength(1);
  expect(bobBefore.state.cursors).toEqual({});
  expect(aliceBefore.state.cursors).toEqual({});
  expect(aliceBefore.state.operations[0].spaceSeq).toBe(0);
  // Remove automatic work left by save. The next admitted operation is manual sync.
  await open(page); await start(page);
  let release!: () => void;
  const gate = new Promise<void>(resolve => { release = resolve; });
  holdAppend = () => gate; online = true;
  await page.evaluate(() => (window as any).durability.requestManualSync());
  await expect.poll(() => appends.length).toBe(1);
  expect(appends).toEqual([aliceBefore.queue[0].client_op_id]);
  expect(appendAuthorizations).toEqual(["Bearer synthetic-alice"]);
  expect(operations.get(appends[0])?.envelope).toEqual(aliceBefore.queue[0]);
  online = false;
  await page.evaluate(args => (window as any).durability.start(args), {
    endpoint: `${origin}/pim`, account: "bob", space, keyByte: 8,
  });
  await expect(page.getByText("Bob durable sentinel", { exact: true })).toBeVisible();
  const bobWhileHeld = await snapshot(page);
  expect(bobWhileHeld).toEqual(bobBefore);
  release(); holdAppend = undefined;
  await expect.poll(() => page.evaluate(async () => {
    const locks = await navigator.locks.query();
    return [...(locks.held ?? []), ...(locks.pending ?? [])]
      .filter(lock => lock.name === "kamori-web-data-plane-v1").length;
  })).toBe(0);
  const bobAfter = await snapshot(page);
  await start(page);
  const aliceAfter = await snapshot(page);
  const evidence = { heldOperation: appends[0], authorization: appendAuthorizations[0],
    aliceBefore, bobBefore, bobWhileHeld, aliceAfter, bobAfter };
  const evidencePath = info.outputPath("held-ack-snapshots.json");
  writeFileSync(evidencePath, JSON.stringify(evidence, null, 2));
  await info.attach("held-ack-snapshots", { path: evidencePath, contentType: "application/json" });
  expect(bobAfter).toEqual(bobBefore);
  expect(aliceAfter).toEqual(aliceBefore);
  await open(page); await start(page, "bob", `${origin}/pim`, 8);
  const bobReopened = await snapshot(page);
  expect(bobReopened).toEqual(bobBefore);
  await open(page); await start(page);
  const aliceReopened = await snapshot(page);
  expect(aliceReopened).toEqual(aliceBefore);
  // The remote acknowledgement succeeded, but the stale client kept its edit.
  // Reopening Alice retries exactly that operation and consumes the duplicate ack.
  online = true;
  await page.evaluate(() => (window as any).durability.requestManualSync());
  await expect.poll(async () => (await snapshot(page)).queue.length).toBe(0);
  expect(appends).toEqual([aliceBefore.queue[0].client_op_id, aliceBefore.queue[0].client_op_id]);
  expect(operations.size).toBe(1);
  const retried = await snapshot(page);
  expect(retried.state.items).toEqual(aliceBefore.state.items);
  expect(retried.state.operations).toEqual(aliceBefore.state.operations.map((op: any) => ({ ...op, spaceSeq: 1 })));
  expect(retried.state.cursors).toEqual({ [space]: 1 });
  online = false;
  await open(page); await start(page, "bob", `${origin}/pim`, 8);
  const bobAfterAliceRetry = await snapshot(page);
  expect(bobAfterAliceRetry).toEqual(bobBefore);
  const retryPath = info.outputPath("held-ack-reopen-retry.json");
  writeFileSync(retryPath, JSON.stringify({ ...evidence, bobReopened, aliceReopened, retried,
    bobAfterAliceRetry, appends, distinctOperations: operations.size }, null, 2));
  await info.attach("held-ack-reopen-retry", { path: retryPath, contentType: "application/json" });
});


for (const helper of ["storeMaterializedPimState", "commitLocalPimOperation", "removeQueuedOperationEnvelope",
  "listQueuedOperationEnvelopes", "queueOperationEnvelope", "loadMaterializedPimState", "storeSpaceKey", "quarantineOperationEnvelope",
  "listQuarantinedOperationRecords", "removeQuarantinedOperationEnvelope", "loadSpaceKey"] as const) {
  test(`vault helper ${helper} rejects a switch while its real database open is held`, async ({ page }) => {
    await open(page); await start(page, "bob", `${origin}/pim`, 8); await save(page, "Bob helper sentinel");
    const bob = await snapshot(page);
    await start(page); await save(page, "Alice helper sentinel");
    const alice = await snapshot(page);
    await page.evaluate(async ({ helper, alice, space }) => {
      const w = window as any;
      const vault = w.durability.vault;
      const envelope = alice.queue[0];
      await vault.quarantineOperationEnvelope(envelope, 1, "unresolved_pim_graph");
      const realOpen = indexedDB.open.bind(indexedDB);
      w.databaseHeld = false;
      indexedDB.open = (...args: Parameters<IDBFactory["open"]>) => {
        indexedDB.open = realOpen;
        const request = realOpen(...args);
        Object.defineProperty(request, "onsuccess", { set(callback) {
          request.addEventListener("success", event => {
            w.databaseHeld = true;
            w.releaseDatabase = () => callback.call(request, event);
          });
        } });
        return request;
      };
      const calls: Record<string, () => Promise<unknown>> = {
        storeMaterializedPimState: () => vault.storeMaterializedPimState([], [], { [space]: 99 }),
        commitLocalPimOperation: () => vault.commitLocalPimOperation(envelope, [], [], { [space]: 99 }),
        removeQueuedOperationEnvelope: () => vault.removeQueuedOperationEnvelope(space, envelope.client_op_id),
        listQueuedOperationEnvelopes: () => vault.listQueuedOperationEnvelopes(),
        queueOperationEnvelope: () => vault.queueOperationEnvelope({ ...envelope, client_op_id: crypto.randomUUID() }),
        loadMaterializedPimState: () => vault.loadMaterializedPimState(),
        storeSpaceKey: () => vault.storeSpaceKey(space, 1, new Uint8Array(32).fill(33)),
        quarantineOperationEnvelope: () => vault.quarantineOperationEnvelope(envelope, 2, "invalid_ciphertext"),
        listQuarantinedOperationRecords: () => vault.listQuarantinedOperationRecords(space),
        removeQuarantinedOperationEnvelope: () => vault.removeQuarantinedOperationEnvelope(space, envelope.client_op_id),
        loadSpaceKey: () => vault.loadSpaceKey(space, 1),
      };
      w.helperResult = calls[helper]().then(() => "resolved", (error: Error) => error.message);
    }, { helper, alice, space });
    await expect.poll(() => page.evaluate(() => (window as any).databaseHeld)).toBe(true);
    await start(page, "bob", `${origin}/pim`, 8);
    await page.evaluate(() => (window as any).releaseDatabase());
    expect(await page.evaluate(() => (window as any).helperResult)).toContain("vault changed");
    expect(await snapshot(page)).toEqual(bob);
    expect(await page.evaluate(async space => ({
      key: Array.from(await (window as any).durability.vault.loadSpaceKey(space, 1)),
      quarantined: await (window as any).durability.vault.listQuarantinedOperationRecords(space),
    }), space)).toEqual({ key: Array(32).fill(9), quarantined: [] });
    // Unlock directly so the fixture does not re-seed and conceal a key write.
    await page.evaluate(async endpoint => {
      const vault = (window as any).durability.vault;
      vault.lockWebVault();
      await vault.unlockOrCreateWebVault(endpoint, "alice", new Uint8Array(32).fill(7));
    }, `${origin}/pim`);
    expect(await snapshot(page)).toEqual(alice);
    const extra = await page.evaluate(async space => ({
      key: Array.from(await (window as any).durability.vault.loadSpaceKey(space, 1)),
      quarantined: await (window as any).durability.vault.listQuarantinedOperationRecords(space),
    }), space);
    expect(extra.key).toEqual(Array(32).fill(9));
    expect(extra.quarantined).toEqual([{ version: 1, envelope: alice.queue[0], space_seq: 1, reason_code: "unresolved_pim_graph" }]);
  });
}


test("a stale unauthorized append cannot refresh or clear the replacement account", async ({ page }) => {
  await open(page); await start(page, "bob", `${origin}/pim`, 8); await save(page, "Bob auth sentinel");
  const bob = await snapshot(page);
  await start(page); await save(page, "Alice auth sentinel");
  const alice = await snapshot(page);
  await open(page); await start(page);
  let release!: () => void;
  const gate = new Promise<void>(resolve => { release = resolve; });
  holdAppend = () => gate; appendStatus = 401; online = true;
  await page.evaluate(() => (window as any).durability.requestManualSync());
  await expect.poll(() => appends.length).toBe(1);
  expect(appends).toEqual([alice.queue[0].client_op_id]);
  online = false;
  await page.evaluate(args => (window as any).durability.start(args), {
    endpoint: `${origin}/pim`, account: "bob", space, keyByte: 8,
  });
  await expect(page.getByText("Bob auth sentinel", { exact: true })).toBeVisible();
  release(); holdAppend = undefined;
  await expect.poll(() => page.evaluate(async () => {
    const locks = await navigator.locks.query();
    return [...(locks.held ?? []), ...(locks.pending ?? [])]
      .filter(lock => lock.name === "kamori-web-data-plane-v1").length;
  })).toBe(0);
  expect(refreshes).toEqual([]);
  expect(bootstraps).toEqual([]);
  expect(await page.evaluate(() => (window as any).durability.session())).toEqual({
    username: "bob", token: "synthetic-bob", accessToken: "synthetic-bob",
  });
  expect(await snapshot(page)).toEqual(bob);
  await start(page); expect(await snapshot(page)).toEqual(alice);
});

test("master-key callbacks reject a replaced lifetime and erase their temporary key", async ({ page }) => {
  await open(page); await start(page);
  await page.evaluate(() => {
    const w = window as any;
    w.keyResult = w.durability.vault.withActiveMasterKey(async (key: Uint8Array) => {
      w.temporaryKey = key;
      await new Promise<void>(resolve => { w.releaseKey = resolve; });
      return "obsolete result";
    }).then(() => "resolved", (error: Error) => error.message);
  });
  await start(page, "bob", `${origin}/pim`, 8);
  await page.evaluate(() => (window as any).releaseKey());
  expect(await page.evaluate(() => (window as any).keyResult)).toContain("vault changed");
  expect(await page.evaluate(() => Array.from((window as any).temporaryKey))).toEqual(Array(32).fill(0));
  expect(await page.evaluate(() => (window as any).durability.vault.withActiveMasterKey((key: Uint8Array) => key[0]))).toBe(8);
});
