// Test-only shell around unchanged production orchestration, encryption and storage.
import { mount, unmount } from "svelte";
import AppWorkspace from "../../../apps/web-frontend/src/lib/components/app/AppWorkspace.svelte";
import { cloudApi } from "../../../apps/web-frontend/src/lib/api/cloud";
import * as vault from "../../../apps/web-frontend/src/lib/cryptoVault";
import { appState } from "../../../apps/web-frontend/src/lib/stores/app";
import { tokenStore } from "../../../apps/web-frontend/src/lib/auth/tokenStore";
import { requestManualSync, resetSyncState, syncState } from "../../../apps/web-frontend/src/lib/stores/sync";
import { notificationStore } from "../../../apps/web-frontend/src/lib/stores/notifications";
let component;
let sessionState;
appState.subscribe(state => { sessionState = state; });
window.durability = {
  cloudApi, vault, requestManualSync,
  session: () => ({ username: sessionState.currentUsername, accessToken: sessionState.accessToken, token: tokenStore.getAccessToken() }),
  async start({ endpoint, account, space, keyByte = 7 }) {
    if (component) await unmount(component);
    vault.lockWebVault();
    const device = await vault.unlockOrCreateWebVault(endpoint, account, new Uint8Array(32).fill(keyByte));
    await vault.storeSpaceKey(space, 1, new Uint8Array(32).fill(9));
    tokenStore.setAccessToken(`synthetic-${account}`);
    appState.set({ cloudBaseUrl: endpoint, currentUsername: account, accessToken: `synthetic-${account}`,
      totpContinuationToken: null, collections: [{ id: space, name: "Durability", role: "editor",
      keyAvailable: true, keyEpoch: 1, syncedItems: 0 }], syncedItemsTotal: 0, lastSyncedSeq: 0 });
    resetSyncState();
    component = mount(AppWorkspace, { target: document.body, props: { view: "tasks" } });
    return { deviceId: device.deviceId, signingPublicKey: Array.from(device.identity.signing_public_key) };
  },
};
syncState.subscribe(state => { window.durability.sync = state; });
notificationStore.subscribe(state => { window.durability.notifications = state; });
