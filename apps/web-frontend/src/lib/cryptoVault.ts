import { browser } from "$app/environment";
import { decode, encode } from "@msgpack/msgpack";

import {
  decryptVaultBytes,
  encryptVaultBytes,
  generateWebDeviceIdentity,
  type WebDeviceIdentity,
} from "$lib/opaqueClient";
import type { OperationEnvelopeV1 } from "$lib/opaqueClient";
import type {
  MaterializedOperationState,
  MaterializedPimItem,
  MaterializedPimState,
} from "$lib/pim";

const DATABASE_NAME = "kamori-web-vault";
const DATABASE_VERSION = 7;
const DEVICE_STORE = "devices";
const SPACE_KEY_STORE = "space-keys";
const OUTBOX_STORE = "outbox";
const LOCAL_UNLOCK_STORE = "local-unlock";
const PIM_STATE_STORE = "pim-state";
const QUARANTINE_STORE = "quarantine";
const META_STORE = "meta";

interface StoredDevice {
  deviceId: string;
  encryptedIdentity: Uint8Array;
}

interface StoredLocalUnlock {
  key: CryptoKey;
  nonce: ArrayBuffer;
  ciphertext: ArrayBuffer;
}

interface QueuedEnvelopeRecord {
  envelope: OperationEnvelopeV1;
  queueOrder: number;
}

let activeAccountScope = "";
let activeMasterKey: Uint8Array | null = null;
let activeDevice: { deviceId: string; identity: WebDeviceIdentity } | null = null;

const DATA_RECOVERY_VERIFIER_DOMAIN = new TextEncoder().encode(
  "kamori.client.data-recovery-verifier.v1\0",
);

const ACCOUNT_SCOPE_DOMAIN = new TextEncoder().encode(
  "kamori.web-vault.account-scope.v1\0",
);

const legacyAccountScope = (cloudBaseUrl: string, username: string): string =>
  JSON.stringify([
    cloudBaseUrl.trim().toLowerCase(),
    username.trim().toLowerCase(),
  ]);

const accountScope = async (
  cloudBaseUrl: string,
  username: string,
): Promise<string> => {
  const normalizedBaseUrl = new URL(cloudBaseUrl).toString().replace(/\/$/, "");
  const identity = new TextEncoder().encode(
    JSON.stringify([normalizedBaseUrl, username.trim().toLowerCase()]),
  );
  const input = new Uint8Array(ACCOUNT_SCOPE_DOMAIN.length + identity.length);
  input.set(ACCOUNT_SCOPE_DOMAIN);
  input.set(identity, ACCOUNT_SCOPE_DOMAIN.length);
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", input));
  return Array.from(digest, (byte) => byte.toString(16).padStart(2, "0")).join("");
};

/** Derives the non-decryption credential used to authorize data-kit recovery. */
export const deriveDataRecoveryVerifier = async (
  masterKey: Uint8Array,
): Promise<Uint8Array> => {
  if (masterKey.length !== 32) {
    throw new Error("Account master key is invalid.");
  }
  const input = new Uint8Array(
    DATA_RECOVERY_VERIFIER_DOMAIN.length + masterKey.length,
  );
  input.set(DATA_RECOVERY_VERIFIER_DOMAIN);
  input.set(masterKey, DATA_RECOVERY_VERIFIER_DOMAIN.length);
  return new Uint8Array(await crypto.subtle.digest("SHA-256", input));
};

const openDatabase = (): Promise<IDBDatabase> => {
  if (!browser || !window.indexedDB) {
    return Promise.reject(new Error("Encrypted browser storage is unavailable."));
  }
  return new Promise((resolve, reject) => {
    const request = window.indexedDB.open(DATABASE_NAME, DATABASE_VERSION);
    request.onupgradeneeded = (event) => {
      const database = request.result;
      if (!database.objectStoreNames.contains(DEVICE_STORE)) {
        database.createObjectStore(DEVICE_STORE);
      }
      if (!database.objectStoreNames.contains(SPACE_KEY_STORE)) {
        database.createObjectStore(SPACE_KEY_STORE);
      }
      if (!database.objectStoreNames.contains(OUTBOX_STORE)) {
        database.createObjectStore(OUTBOX_STORE);
      }
      if (!database.objectStoreNames.contains(LOCAL_UNLOCK_STORE)) {
        database.createObjectStore(LOCAL_UNLOCK_STORE);
      }
      if (!database.objectStoreNames.contains(PIM_STATE_STORE)) {
        database.createObjectStore(PIM_STATE_STORE);
      }
      if (!database.objectStoreNames.contains(QUARANTINE_STORE)) {
        database.createObjectStore(QUARANTINE_STORE);
      }
      if (!database.objectStoreNames.contains(META_STORE)) {
        database.createObjectStore(META_STORE);
      }
      // Earlier builds persisted local unlock material automatically after a
      // password login. Revoke that implicit grant; v7+ only stores it after
      // explicit user consent.
      if ((event as IDBVersionChangeEvent).oldVersion < 7) {
        request.transaction?.objectStore(LOCAL_UNLOCK_STORE).clear();
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error ?? new Error("Failed to open encrypted storage."));
  });
};

const readValue = async <T>(storeName: string, key: string): Promise<T | undefined> => {
  const database = await openDatabase();
  try {
    return await new Promise<T | undefined>((resolve, reject) => {
      const request = database.transaction(storeName, "readonly").objectStore(storeName).get(key);
      request.onsuccess = () => resolve(request.result as T | undefined);
      request.onerror = () => reject(request.error ?? new Error("Encrypted storage read failed."));
    });
  } finally {
    database.close();
  }
};

const writeValue = async (storeName: string, key: string, value: unknown): Promise<void> => {
  const database = await openDatabase();
  try {
    await new Promise<void>((resolve, reject) => {
      const transaction = database.transaction(storeName, "readwrite");
      transaction.objectStore(storeName).put(value, key);
      transaction.oncomplete = () => resolve();
      transaction.onerror = () => reject(transaction.error ?? new Error("Encrypted storage write failed."));
      transaction.onabort = () => reject(transaction.error ?? new Error("Encrypted storage write aborted."));
    });
  } finally {
    database.close();
  }
};

const deleteValue = async (storeName: string, key: string): Promise<void> => {
  const database = await openDatabase();
  try {
    await new Promise<void>((resolve, reject) => {
      const transaction = database.transaction(storeName, "readwrite");
      transaction.objectStore(storeName).delete(key);
      transaction.oncomplete = () => resolve();
      transaction.onerror = () => reject(transaction.error ?? new Error("Encrypted storage delete failed."));
      transaction.onabort = () => reject(transaction.error ?? new Error("Encrypted storage delete aborted."));
    });
  } finally {
    database.close();
  }
};

/**
 * Re-keys pre-v7 IndexedDB records whose keys exposed the normalized username.
 * Values remain encrypted; the migration only replaces lookup keys.
 */
const migrateLegacyAccountScope = async (
  legacyScope: string,
  nextScope: string,
): Promise<void> => {
  if (legacyScope === nextScope) return;
  const database = await openDatabase();
  try {
    await new Promise<void>((resolve, reject) => {
      const stores = [
        DEVICE_STORE,
        SPACE_KEY_STORE,
        OUTBOX_STORE,
        LOCAL_UNLOCK_STORE,
        PIM_STATE_STORE,
        QUARANTINE_STORE,
        META_STORE,
      ];
      const transaction = database.transaction(stores, "readwrite");

      for (const storeName of [DEVICE_STORE, LOCAL_UNLOCK_STORE, PIM_STATE_STORE]) {
        const store = transaction.objectStore(storeName);
        const request = store.get(legacyScope);
        request.onsuccess = () => {
          if (request.result !== undefined) {
            store.put(request.result, nextScope);
            store.delete(legacyScope);
          }
        };
      }

      const meta = transaction.objectStore(META_STORE);
      const oldCounterKey = `outbox-order:${legacyScope}`;
      const counterRequest = meta.get(oldCounterKey);
      counterRequest.onsuccess = () => {
        if (counterRequest.result !== undefined) {
          meta.put(counterRequest.result, `outbox-order:${nextScope}`);
          meta.delete(oldCounterKey);
        }
      };

      for (const storeName of [SPACE_KEY_STORE, OUTBOX_STORE, QUARANTINE_STORE]) {
        const store = transaction.objectStore(storeName);
        const cursorRequest = store.openCursor();
        cursorRequest.onsuccess = () => {
          const cursor = cursorRequest.result;
          if (!cursor) return;
          if (
            typeof cursor.key === "string" &&
            cursor.key.startsWith(`${legacyScope}:`)
          ) {
            const suffix = cursor.key.slice(legacyScope.length);
            store.put(cursor.value, `${nextScope}${suffix}`);
            cursor.delete();
          }
          cursor.continue();
        };
      }

      transaction.oncomplete = () => resolve();
      transaction.onerror = () =>
        reject(transaction.error ?? new Error("Browser storage privacy migration failed."));
      transaction.onabort = () =>
        reject(transaction.error ?? new Error("Browser storage privacy migration aborted."));
    });
  } finally {
    database.close();
  }
};

const requireMasterKey = (): Uint8Array => {
  if (!activeMasterKey || !activeAccountScope) {
    throw new Error("Unlock the encrypted vault first.");
  }
  return activeMasterKey;
};

const normalizeIdentity = (value: unknown): WebDeviceIdentity => {
  const identity = value as WebDeviceIdentity;
  for (const field of [
    identity.signing_private_key,
    identity.signing_public_key,
    identity.hpke_private_key,
    identity.hpke_public_key,
  ]) {
    if (!(field instanceof Uint8Array) || field.length !== 32) {
      throw new Error("Stored device identity is invalid.");
    }
  }
  return identity;
};

const activateWebVault = async (
  cloudBaseUrl: string,
  username: string,
  masterKey: Uint8Array,
): Promise<void> => {
  if (masterKey.length !== 32) {
    throw new Error("Account master key is invalid.");
  }
  const scope = await accountScope(cloudBaseUrl, username);
  await migrateLegacyAccountScope(
    legacyAccountScope(cloudBaseUrl, username),
    scope,
  );
  activeMasterKey?.fill(0);
  activeAccountScope = scope;
  activeMasterKey = new Uint8Array(masterKey);
  activeDevice = null;
};

/** Unlocks account data without trusting device credentials revoked by recovery. */
export const unlockWebVaultForRecovery = async (
  cloudBaseUrl: string,
  username: string,
  masterKey: Uint8Array,
): Promise<void> => activateWebVault(cloudBaseUrl, username, masterKey);

export const unlockOrCreateWebVault = async (
  cloudBaseUrl: string,
  username: string,
  masterKey: Uint8Array,
): Promise<{ deviceId: string; identity: WebDeviceIdentity }> => {
  await activateWebVault(cloudBaseUrl, username, masterKey);
  const activeKey = requireMasterKey();

  const stored = await readValue<StoredDevice>(DEVICE_STORE, activeAccountScope);
  if (stored) {
    const plaintext = await decryptVaultBytes(activeKey, stored.encryptedIdentity);
    activeDevice = {
      deviceId: stored.deviceId,
      identity: normalizeIdentity(decode(plaintext)),
    };
    return activeDevice;
  }

  const identity = await generateWebDeviceIdentity();
  const deviceId = crypto.randomUUID();
  const encryptedIdentity = await encryptVaultBytes(activeKey, encode(identity));
  await writeValue(
    DEVICE_STORE,
    activeAccountScope,
    { deviceId, encryptedIdentity } satisfies StoredDevice,
  );
  activeDevice = { deviceId, identity };
  return activeDevice;
};

export const getActiveWebDevice = (): { deviceId: string; identity: WebDeviceIdentity } => {
  if (!activeDevice) {
    throw new Error("Unlock the encrypted device identity first.");
  }
  return activeDevice;
};

/**
 * Gives a short-lived copy of the unlocked account master key to one operation
 * and clears that copy even if the operation fails.
 */
export const withActiveMasterKey = async <T>(
  operation: (masterKey: Uint8Array) => Promise<T> | T,
): Promise<T> => {
  const masterKey = new Uint8Array(requireMasterKey());
  try {
    return await operation(masterKey);
  } finally {
    masterKey.fill(0);
  }
};

/**
 * Remembers the master key for local browser unlock under a non-extractable
 * WebCrypto key. This is an at-rest browser-storage control, not hardware
 * binding: same-origin script running in this profile can request decryption.
 */
export const rememberMasterKeyForLocalUnlock = async (
  cloudBaseUrl: string,
  username: string,
  masterKey: Uint8Array,
): Promise<void> => {
  if (masterKey.length !== 32) {
    throw new Error("Account master key is invalid.");
  }
  const key = await crypto.subtle.generateKey(
    { name: "AES-GCM", length: 256 },
    false,
    ["encrypt", "decrypt"],
  );
  const nonce = crypto.getRandomValues(new Uint8Array(12));
  const ciphertext = await crypto.subtle.encrypt(
    { name: "AES-GCM", iv: nonce },
    key,
    new Uint8Array(masterKey),
  );
  await writeValue(LOCAL_UNLOCK_STORE, await accountScope(cloudBaseUrl, username), {
    key,
    nonce: nonce.buffer as ArrayBuffer,
    ciphertext,
  } satisfies StoredLocalUnlock);
};

export const unlockWebVaultFromLocalUnlock = async (
  cloudBaseUrl: string,
  username: string,
): Promise<{ deviceId: string; identity: WebDeviceIdentity } | null> => {
  const stored = await readValue<StoredLocalUnlock>(
    LOCAL_UNLOCK_STORE,
    await accountScope(cloudBaseUrl, username),
  );
  if (!stored) {
    return null;
  }
  let plaintext: ArrayBuffer;
  try {
    plaintext = await crypto.subtle.decrypt(
      { name: "AES-GCM", iv: stored.nonce },
      stored.key,
      stored.ciphertext,
    );
  } catch {
    throw new Error("This browser's local unlock record is damaged.");
  }
  const masterKey = new Uint8Array(plaintext);
  try {
    return await unlockOrCreateWebVault(cloudBaseUrl, username, masterKey);
  } finally {
    masterKey.fill(0);
  }
};

export const forgetMasterKeyForLocalUnlock = async (
  cloudBaseUrl: string,
  username: string,
): Promise<void> => {
  await deleteValue(
    LOCAL_UNLOCK_STORE,
    await accountScope(cloudBaseUrl, username),
  );
};

/** Drops credentials revoked by account recovery while retaining recovered data keys. */
export const resetWebCredentialsAfterRecovery = async (
  cloudBaseUrl: string,
  username: string,
): Promise<void> => {
  const scope = await accountScope(cloudBaseUrl, username);
  await Promise.all([
    deleteValue(DEVICE_STORE, scope),
    deleteValue(LOCAL_UNLOCK_STORE, scope),
  ]);
  if (activeAccountScope === scope) {
    activeDevice = null;
  }
};

export const storeSpaceKey = async (
  spaceId: string,
  keyEpoch: number,
  key: Uint8Array,
): Promise<void> => {
  if (key.length !== 32) {
    throw new Error("Space key must be 32 bytes.");
  }
  if (!Number.isSafeInteger(keyEpoch) || keyEpoch <= 0) {
    throw new Error("Space key epoch must be positive.");
  }
  const encrypted = await encryptVaultBytes(requireMasterKey(), key);
  await writeValue(
    SPACE_KEY_STORE,
    `${activeAccountScope}:${spaceId}:${keyEpoch}`,
    encrypted,
  );
};

export const loadSpaceKey = async (
  spaceId: string,
  keyEpoch: number,
): Promise<Uint8Array | null> => {
  const encrypted = await readValue<Uint8Array>(
    SPACE_KEY_STORE,
    `${activeAccountScope}:${spaceId}:${keyEpoch}`,
  );
  return encrypted ? decryptVaultBytes(requireMasterKey(), encrypted) : null;
};

export const queueOperationEnvelope = async (
  envelope: OperationEnvelopeV1,
): Promise<void> => {
  requireMasterKey();
  const database = await openDatabase();
  try {
    await new Promise<void>((resolve, reject) => {
      const transaction = database.transaction(
        [OUTBOX_STORE, META_STORE],
        "readwrite",
      );
      const outbox = transaction.objectStore(OUTBOX_STORE);
      const meta = transaction.objectStore(META_STORE);
      const counterKey = `outbox-order:${activeAccountScope}`;
      const outboxKey = `${activeAccountScope}:${envelope.space_id}:${envelope.client_op_id}`;
      const counterRequest = meta.get(counterKey);
      counterRequest.onsuccess = () => {
        const current = counterRequest.result;
        const existingRequest = outbox.get(outboxKey);
        existingRequest.onsuccess = () => {
          const existing = existingRequest.result as
            | QueuedEnvelopeRecord
            | undefined;
          const existingOrder = existing?.queueOrder;
          const queueOrder =
            typeof existingOrder === "number" &&
            Number.isSafeInteger(existingOrder) &&
            existingOrder > 0
              ? existingOrder
              : typeof current === "number" &&
                  Number.isSafeInteger(current) &&
                  current >= 0
                ? current + 1
                : 1;
          if (!Number.isSafeInteger(queueOrder)) {
            transaction.abort();
            return;
          }
          if (queueOrder !== existingOrder) {
            meta.put(queueOrder, counterKey);
          }
          outbox.put(
            { envelope, queueOrder } satisfies QueuedEnvelopeRecord,
            outboxKey,
          );
          // Remove the pre-space-scoping key after the replacement is queued in
          // the same transaction. This upgrades existing offline outboxes safely.
          outbox.delete(`${activeAccountScope}:${envelope.client_op_id}`);
        };
        existingRequest.onerror = () => transaction.abort();
      };
      counterRequest.onerror = () => transaction.abort();
      transaction.oncomplete = () => resolve();
      transaction.onerror = () =>
        reject(transaction.error ?? new Error("Outbox write failed."));
      transaction.onabort = () =>
        reject(transaction.error ?? new Error("Outbox write aborted."));
    });
  } finally {
    database.close();
  }
};

export const listQueuedOperationEnvelopes = async (): Promise<OperationEnvelopeV1[]> => {
  requireMasterKey();
  const database = await openDatabase();
  try {
    return await new Promise<OperationEnvelopeV1[]>((resolve, reject) => {
      const request = database
        .transaction(OUTBOX_STORE, "readonly")
        .objectStore(OUTBOX_STORE)
        .openCursor();
      const operations: Array<{
        envelope: OperationEnvelopeV1;
        queueOrder: number;
        legacyOrder: number;
      }> = [];
      request.onsuccess = () => {
        const cursor = request.result;
        if (!cursor) {
          operations.sort(
            (left, right) =>
              left.queueOrder - right.queueOrder ||
              left.legacyOrder - right.legacyOrder,
          );
          resolve(operations.map((record) => record.envelope));
          return;
        }
        if (
          typeof cursor.key === "string" &&
          cursor.key.startsWith(`${activeAccountScope}:`)
        ) {
          const value = cursor.value as
            | QueuedEnvelopeRecord
            | OperationEnvelopeV1;
          const record = value as QueuedEnvelopeRecord;
          operations.push({
            envelope:
              record.envelope && typeof record.queueOrder === "number"
                ? record.envelope
                : (value as OperationEnvelopeV1),
            queueOrder:
              Number.isSafeInteger(record.queueOrder) && record.queueOrder > 0
                ? record.queueOrder
                : 0,
            legacyOrder: operations.length,
          });
        }
        cursor.continue();
      };
      request.onerror = () => reject(request.error ?? new Error("Outbox read failed."));
    });
  } finally {
    database.close();
  }
};

export const removeQueuedOperationEnvelope = async (
  spaceId: string,
  clientOpId: string,
): Promise<void> => {
  requireMasterKey();
  const database = await openDatabase();
  try {
    await new Promise<void>((resolve, reject) => {
      const transaction = database.transaction(OUTBOX_STORE, "readwrite");
      const store = transaction.objectStore(OUTBOX_STORE);
      store.delete(`${activeAccountScope}:${spaceId}:${clientOpId}`);
      // A queued operation written by version 2 used the unscoped key.
      store.delete(`${activeAccountScope}:${clientOpId}`);
      transaction.oncomplete = () => resolve();
      transaction.onerror = () => reject(transaction.error ?? new Error("Outbox delete failed."));
    });
  } finally {
    database.close();
  }
};

/**
 * Retains a signed but unusable remote envelope for diagnostics/reprocessing.
 * The record is master-key encrypted and intentionally stores only a stable
 * reason code, never decrypted content or exception text.
 */
export const quarantineOperationEnvelope = async (
  envelope: OperationEnvelopeV1,
  spaceSeq: number,
  reasonCode: string,
): Promise<void> => {
  const encrypted = await encryptVaultBytes(
    requireMasterKey(),
    encode({ version: 1, envelope, space_seq: spaceSeq, reason_code: reasonCode }),
  );
  await writeValue(
    QUARANTINE_STORE,
    `${activeAccountScope}:${envelope.space_id}:${envelope.client_op_id}`,
    encrypted,
  );
};

export interface QuarantinedOperationRecord {
  envelope: OperationEnvelopeV1;
  space_seq: number;
  reason_code: string;
}

/** Lists encrypted quarantine records for a space without exposing other accounts. */
export const listQuarantinedOperationRecords = async (
  spaceId: string,
): Promise<QuarantinedOperationRecord[]> => {
  const masterKey = requireMasterKey();
  const database = await openDatabase();
  let encryptedRecords: Uint8Array[];
  try {
    encryptedRecords = await new Promise<Uint8Array[]>((resolve, reject) => {
      const records: Uint8Array[] = [];
      const request = database
        .transaction(QUARANTINE_STORE, "readonly")
        .objectStore(QUARANTINE_STORE)
        .openCursor();
      const prefix = `${activeAccountScope}:${spaceId}:`;
      request.onsuccess = () => {
        const cursor = request.result;
        if (!cursor) {
          resolve(records);
          return;
        }
        if (typeof cursor.key === "string" && cursor.key.startsWith(prefix)) {
          records.push(cursor.value as Uint8Array);
        }
        cursor.continue();
      };
      request.onerror = () =>
        reject(request.error ?? new Error("Quarantine read failed."));
    });
  } finally {
    database.close();
  }
  const records: QuarantinedOperationRecord[] = [];
  for (const encrypted of encryptedRecords) {
    const decoded = decode(await decryptVaultBytes(masterKey, encrypted)) as {
      version?: number;
      envelope?: OperationEnvelopeV1;
      space_seq?: number;
      reason_code?: string;
    };
    if (
      decoded.version !== 1 ||
      !decoded.envelope ||
      decoded.envelope.space_id !== spaceId ||
      typeof decoded.space_seq !== "number" ||
      !Number.isSafeInteger(decoded.space_seq) ||
      decoded.space_seq <= 0 ||
      typeof decoded.reason_code !== "string" ||
      decoded.reason_code.length === 0
    ) {
      throw new Error("Stored quarantine record is invalid.");
    }
    records.push(decoded as QuarantinedOperationRecord);
  }
  return records;
};

export const removeQuarantinedOperationEnvelope = async (
  spaceId: string,
  clientOpId: string,
): Promise<void> => {
  requireMasterKey();
  await deleteValue(
    QUARANTINE_STORE,
    `${activeAccountScope}:${spaceId}:${clientOpId}`,
  );
};

/** Persists the decrypted PIM projection only as master-key-encrypted bytes. */
export const storeMaterializedPimState = async (
  items: MaterializedPimItem[],
  operations: MaterializedOperationState[],
  cursors: Record<string, number>,
): Promise<void> => {
  const state: MaterializedPimState = { version: 5, items, operations, cursors };
  const encrypted = await encryptVaultBytes(requireMasterKey(), encode(state));
  await writeValue(PIM_STATE_STORE, activeAccountScope, encrypted);
};

/** Restores the encrypted PIM projection for the active account. */
export const loadMaterializedPimState = async (): Promise<MaterializedPimState> => {
  const encrypted = await readValue<Uint8Array>(PIM_STATE_STORE, activeAccountScope);
  if (!encrypted) {
    return { version: 5, items: [], operations: [], cursors: {} };
  }
  const decoded = decode(await decryptVaultBytes(requireMasterKey(), encrypted));
  if (Array.isArray(decoded)) {
    return {
      version: 5,
      items: [],
      operations: [],
      cursors: {},
    };
  }
  const state = decoded as {
    version?: number;
    items?: MaterializedPimItem[];
    operations?: Array<Partial<MaterializedOperationState>>;
    cursors?: Record<string, number>;
  };
  if (
    ![2, 3, 4, 5].includes(state.version ?? 0) ||
    !Array.isArray(state.items) ||
    !Array.isArray(state.operations)
  ) {
    throw new Error("Stored PIM projection is invalid.");
  }
  // Versions before v5 did not retain the complete version graph. Replaying
  // the encrypted oplog is the only safe migration: synthesizing projections
  // from selected fields would silently discard unknown iCalendar/vCard data.
  if (state.version !== 5) {
    return { version: 5, items: [], operations: [], cursors: {} };
  }
  return {
    version: 5,
    items: state.items,
    operations: state.operations.map((operation) => {
      const spaceSeq = operation.spaceSeq;
      if (typeof operation.materializedProjection !== "string") {
        throw new Error("Stored PIM projection is incomplete.");
      }
      return {
        ...(operation as MaterializedOperationState),
        spaceSeq:
          typeof spaceSeq === "number" && Number.isSafeInteger(spaceSeq) && spaceSeq >= 0
            ? spaceSeq
            : 0,
      };
    }),
    cursors:
      state.cursors && typeof state.cursors === "object"
        ? Object.fromEntries(
            Object.entries(state.cursors).filter(
              ([spaceId, cursor]) =>
                spaceId.length > 0 &&
                typeof cursor === "number" &&
                Number.isSafeInteger(cursor) &&
                cursor >= 0,
            ),
          )
        : {},
  };
};

export const lockWebVault = (): void => {
  activeMasterKey?.fill(0);
  activeMasterKey = null;
  activeAccountScope = "";
  activeDevice = null;
};

/** Permanently removes this account's encrypted browser material after deletion. */
export const deleteWebVaultAccount = async (
  cloudBaseUrl: string,
  username: string,
): Promise<void> => {
  const scope = await accountScope(cloudBaseUrl, username);
  const database = await openDatabase();
  try {
    await new Promise<void>((resolve, reject) => {
      const transaction = database.transaction(
        [
          DEVICE_STORE,
          SPACE_KEY_STORE,
          OUTBOX_STORE,
          LOCAL_UNLOCK_STORE,
          PIM_STATE_STORE,
          QUARANTINE_STORE,
          META_STORE,
        ],
        "readwrite",
      );
      transaction.objectStore(DEVICE_STORE).delete(scope);
      transaction.objectStore(LOCAL_UNLOCK_STORE).delete(scope);
      transaction.objectStore(PIM_STATE_STORE).delete(scope);
      transaction.objectStore(META_STORE).delete(`outbox-order:${scope}`);

      const outbox = transaction.objectStore(OUTBOX_STORE);
      const outboxCursor = outbox.openKeyCursor();
      outboxCursor.onsuccess = () => {
        const current = outboxCursor.result;
        if (!current) return;
        if (
          typeof current.key === "string" &&
          current.key.startsWith(`${scope}:`)
        ) {
          outbox.delete(current.key);
        }
        current.continue();
      };

      const spaceKeys = transaction.objectStore(SPACE_KEY_STORE);
      const cursor = spaceKeys.openKeyCursor();
      cursor.onsuccess = () => {
        const current = cursor.result;
        if (!current) return;
        if (
          typeof current.key === "string" &&
          current.key.startsWith(`${scope}:`)
        ) {
          spaceKeys.delete(current.key);
        }
        current.continue();
      };

      const quarantine = transaction.objectStore(QUARANTINE_STORE);
      const quarantineCursor = quarantine.openKeyCursor();
      quarantineCursor.onsuccess = () => {
        const current = quarantineCursor.result;
        if (!current) return;
        if (
          typeof current.key === "string" &&
          current.key.startsWith(`${scope}:`)
        ) {
          quarantine.delete(current.key);
        }
        current.continue();
      };
      transaction.oncomplete = () => resolve();
      transaction.onerror = () =>
        reject(transaction.error ?? new Error("Encrypted storage deletion failed."));
      transaction.onabort = () =>
        reject(transaction.error ?? new Error("Encrypted storage deletion aborted."));
    });
  } finally {
    database.close();
    lockWebVault();
  }
};
