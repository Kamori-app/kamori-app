import { expect, test } from "@playwright/test";
import { decode, encode } from "@msgpack/msgpack";
import { createHash } from "node:crypto";
import { uiHost } from "./uiHost";
let host: Awaited<ReturnType<typeof uiHost>>;
test.beforeAll(async () => { host = await uiHost("invite"); });
test.afterAll(async () => { await host?.close(); });
const space = "00000000-0000-0000-0000-000000000051";
const owner = "00000000-0000-0000-0000-000000000052";

for (const minutes of [15, 10080]) {
  test(`web sharing creates an encrypted invite with the ${minutes}-minute boundary selected in the mounted UI`, async ({ page }) => {
    await page.goto(host.origin);
    await page.waitForFunction(() => Boolean((window as any).inviteUi));
    const seed = await page.evaluate(space => (window as any).inviteUi.start(space), space);
    let epoch = 1;
    let metadata = new Uint8Array(seed.metadata);
    const devicePackages = new Map<number, any>();
    const recoveryPackages = new Map<number, any>();
    const invites: any[] = [];
    const rotations: any[] = [];
    const requests: { path: string; authorization?: string; contentType?: string }[] = [];
    await page.route(`${host.origin}/**`, async route => {
      const request = route.request();
      const path = new URL(request.url()).pathname;
      if (path === "/fixture.js" || path === "/crypto_core_lib_bg.wasm" || path === "/") { await route.continue(); return; }
      const raw = request.postDataBuffer();
      const payload: any = raw ? decode(raw) : undefined;
      requests.push({ path, authorization: request.headers().authorization, contentType: request.headers()["content-type"] });
      let response: any;
      if (path === "/spaces") response = { spaces: [{ space_id: space, key_epoch: epoch, role: "owner", history_start_seq: 0, current_state_start_seq: 0, encrypted_metadata: metadata, device_key_packages: devicePackages.has(epoch) ? [devicePackages.get(epoch)] : [] }] };
      else if (path === "/spaces/recovery-key-packages") response = { packages: recoveryPackages.has(epoch) ? [{ space_id: space, key_epoch: epoch, encrypted_key_package: recoveryPackages.get(epoch) }] : [] };
      else if (path === "/spaces/trash") response = { spaces: [] };
      else if (path.endsWith("/members")) response = { members: [{ user_id: owner, username: "synthetic-owner", role: "owner", key_epoch: epoch, public_key_bundle: encode({ version: 2, account_recovery_public_key: new Uint8Array(seed.recovery) }) }] };
      else if (path.endsWith("/devices")) response = { devices: [{ device_id: seed.deviceId, user_id: owner, active: true, signing_public_key: new Uint8Array(seed.signing), hpke_public_key: new Uint8Array(seed.hpke) }] };
      else if (path.endsWith("/device-key-packages")) { devicePackages.set(payload.key_epoch, payload); response = {}; }
      else if (path.endsWith("/recovery-key-package")) { recoveryPackages.set(payload.key_epoch, payload.encrypted_key_package); response = {}; }
      else if (path === "/operations") response = { operations: [], next_cursor: 0 };
      else if (path.endsWith("/rotate-key")) {
        rotations.push(payload); epoch = payload.new_key_epoch; metadata = payload.new_encrypted_metadata;
        devicePackages.set(epoch, payload.remaining_device_packages[0]);
        recoveryPackages.set(epoch, payload.remaining_recovery_packages[0].encrypted_key_package);
        response = { rotated: true, key_epoch: epoch };
      } else if (path === "/invite-codes") { invites.push(payload); response = { created: true, expires_at_unix_ms: 2000000000000 }; }
      else throw new Error(`Unexpected sharing HTTP ${request.method()} ${path}`);
      await route.fulfill({ status: 200, contentType: "application/msgpack", body: Buffer.from(encode(response)) });
    });
    await page.evaluate(() => (window as any).inviteUi.mount());
    const expiry = page.getByLabel("Invite expiry");
    await expect(expiry).toBeVisible();
    expect(await expiry.locator("option").evaluateAll(options => options.map(option => Number((option as HTMLOptionElement).value)))).toEqual([15, 30, 60, 180, 720, 1440, 4320, 10080]);
    await expiry.selectOption(String(minutes));
    await expect(expiry).toHaveValue(String(minutes));
    await page.getByLabel("Invite role").selectOption("reader");
    await page.getByPlaceholder("Optional encrypted note for recipient").fill("Synthetic encrypted welcome");
    await page.getByRole("button", { name: "Generate Invite Code", exact: true }).click();
    const code = page.getByText(/^[A-Z0-9]{4}(?:-[A-Z0-9]{4}){3}$/, { exact: true });
    await expect(code).toBeVisible();
    expect(invites).toHaveLength(1);
    expect(rotations).toHaveLength(1);
    const payload = invites[0];
    expect(Object.keys(payload).sort()).toEqual(["encrypted_key_package", "encrypted_note", "invite_code_hash", "role", "rotation_id", "space_id", "ttl_minutes"]);
    expect(payload).toMatchObject({ space_id: space, rotation_id: rotations[0].rotation_id, role: "reader", ttl_minutes: minutes, invite_code_hash: expect.any(Uint8Array), encrypted_key_package: expect.any(Uint8Array), encrypted_note: expect.any(Uint8Array) });
    const normalized = (await code.textContent())!.replaceAll("-", "");
    const expectedHash = createHash("sha256").update("kamori:invite:lookup:v1").update(new Uint8Array([0])).update(normalized).digest();
    expect(Buffer.from(payload.invite_code_hash)).toEqual(expectedHash);
    expect(payload.encrypted_key_package.byteLength).toBe(72);
    expect(payload.encrypted_note.byteLength).toBe(67);
    expect(Buffer.from(payload.encrypted_note).includes(Buffer.from("Synthetic encrypted welcome"))).toBe(false);
    expect(requests.find(request => request.path === "/invite-codes")).toEqual({ path: "/invite-codes", authorization: "Bearer synthetic-owner-access", contentType: "application/msgpack" });
    expect(rotations[0]).toMatchObject({ expected_key_epoch: 1, new_key_epoch: 2, base_space_seq: 0 });
  });
}
