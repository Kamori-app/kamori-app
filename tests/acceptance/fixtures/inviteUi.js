// Seed a synthetic unlocked account; every sharing, key rotation and encryption operation is production.
import { mount } from "svelte";
import { encode } from "@msgpack/msgpack";
import AppWorkspace from "../../../apps/web-frontend/src/lib/components/app/AppWorkspace.svelte";
import { unlockOrCreateWebVault, storeSpaceKey } from "../../../apps/web-frontend/src/lib/cryptoVault";
import { deriveAccountRecoveryKeypair, encryptVaultBytes } from "../../../apps/web-frontend/src/lib/opaqueClient";
import { appState } from "../../../apps/web-frontend/src/lib/stores/app";
import { tokenStore } from "../../../apps/web-frontend/src/lib/auth/tokenStore";
import { notificationStore } from "../../../apps/web-frontend/src/lib/stores/notifications";
window.inviteUi = {
  async start(space) {
    const masterKey = new Uint8Array(32).fill(7);
    const spaceKey = new Uint8Array(32).fill(9);
    const device = await unlockOrCreateWebVault(location.origin, "synthetic-owner", masterKey);
    await storeSpaceKey(space, 1, spaceKey);
    const recovery = await deriveAccountRecoveryKeypair(masterKey);
    tokenStore.setAccessToken("synthetic-owner-access");
    appState.set({ cloudBaseUrl: location.origin, currentUsername: "synthetic-owner", accessToken: "synthetic-owner-access", totpContinuationToken: null,
      collections: [{ id: space, name: "Invite test space", role: "owner", keyAvailable: true, keyEpoch: 1, syncedItems: 0 }], syncedItemsTotal: 0, lastSyncedSeq: 0 });
    const seed = { deviceId: device.deviceId, hpke: Array.from(device.identity.hpke_public_key), signing: Array.from(device.identity.signing_public_key),
      recovery: Array.from(recovery.public_key), metadata: Array.from(await encryptVaultBytes(spaceKey, encode({ name: "Invite test space" }))) };
    recovery.private_key.fill(0);
    return seed;
  },
  mount() { mount(AppWorkspace, { target: document.body, props: { view: "sharing" } }); },
};
notificationStore.subscribe(value => { window.inviteUi.notifications = value; });
