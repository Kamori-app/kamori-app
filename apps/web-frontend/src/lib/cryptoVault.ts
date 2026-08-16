import { browser } from "$app/environment";
import { decode, encode } from "@msgpack/msgpack";

import {
  decryptVaultBytes,
  encryptVaultBytes,
  generateWebDeviceIdentity,
  type WebDeviceIdentity,
} from "$lib/opaqueClient";
import type { OperationEnvelopeV1 } from "$lib/opaqueClient";
import type { MaterializedPimItem } from "$lib/pim";

const DATABASE_NAME = "kamori-web-vault";
const DATABASE_VERSION = 4;
const DEVICE_STORE = "devices";
const SPACE_KEY_STORE = "space-keys";
const OUTBOX_STORE = "outbox";
const LOCAL_UNLOCK_STORE = "local-unlock";
const PIM_STATE_STORE = "pim-state";

interface StoredDevice {
  deviceId: string;
  encryptedIdentity: Uint8Array;
}

interface StoredLocalUnlock {
  key: CryptoKey;
  nonce: ArrayBuffer;
  ciphertext: ArrayBuffer;
}

let activeUsername = "";
let activeMasterKey: Uint8Array | null = null;
let activeDevice: { deviceId: string; identity: WebDeviceIdentity } | null = null;

const DATA_RECOVERY_VERIFIER_DOMAIN = new TextEncoder().encode(
  "kamori.client.data-recovery-verifier.v1\0",
);

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
    request.onupgradeneeded = () => {
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

const requireMasterKey = (): Uint8Array => {
  if (!activeMasterKey || !activeUsername) {
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

export const unlockOrCreateWebVault = async (
  username: string,
  masterKey: Uint8Array,
): Promise<{ deviceId: string; identity: WebDeviceIdentity }> => {
  if (masterKey.length !== 32) {
    throw new Error("Account master key is invalid.");
  }
  activeUsername = username;
  activeMasterKey = new Uint8Array(masterKey);

  const stored = await readValue<StoredDevice>(DEVICE_STORE, username);
  if (stored) {
    const plaintext = await decryptVaultBytes(activeMasterKey, stored.encryptedIdentity);
    activeDevice = {
      deviceId: stored.deviceId,
      identity: normalizeIdentity(decode(plaintext)),
    };
    return activeDevice;
  }

  const identity = await generateWebDeviceIdentity();
  const deviceId = crypto.randomUUID();
  const encryptedIdentity = await encryptVaultBytes(activeMasterKey, encode(identity));
  await writeValue(DEVICE_STORE, username, { deviceId, encryptedIdentity } satisfies StoredDevice);
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
 * Remembers the master key on this approved browser under a non-extractable
 * WebCrypto key. Plaintext key bytes are never written to browser storage.
 */
export const rememberMasterKeyForLocalPasskey = async (
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
  await writeValue(LOCAL_UNLOCK_STORE, username, {
    key,
    nonce: nonce.buffer as ArrayBuffer,
    ciphertext,
  } satisfies StoredLocalUnlock);
};

export const unlockWebVaultFromLocalPasskey = async (
  username: string,
): Promise<{ deviceId: string; identity: WebDeviceIdentity } | null> => {
  const stored = await readValue<StoredLocalUnlock>(LOCAL_UNLOCK_STORE, username);
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
    throw new Error("This browser's local passkey vault is damaged.");
  }
  const masterKey = new Uint8Array(plaintext);
  try {
    return await unlockOrCreateWebVault(username, masterKey);
  } finally {
    masterKey.fill(0);
  }
};

/** Drops credentials revoked by account recovery while retaining recovered data keys. */
export const resetWebCredentialsAfterRecovery = async (username: string): Promise<void> => {
  await Promise.all([
    deleteValue(DEVICE_STORE, username),
    deleteValue(LOCAL_UNLOCK_STORE, username),
  ]);
  if (activeUsername === username) {
    activeDevice = null;
  }
};

export const storeSpaceKey = async (spaceId: string, key: Uint8Array): Promise<void> => {
  if (key.length !== 32) {
    throw new Error("Space key must be 32 bytes.");
  }
  const encrypted = await encryptVaultBytes(requireMasterKey(), key);
  await writeValue(SPACE_KEY_STORE, `${activeUsername}:${spaceId}`, encrypted);
};

export const loadSpaceKey = async (spaceId: string): Promise<Uint8Array | null> => {
  const encrypted = await readValue<Uint8Array>(SPACE_KEY_STORE, `${activeUsername}:${spaceId}`);
  return encrypted ? decryptVaultBytes(requireMasterKey(), encrypted) : null;
};

export const queueOperationEnvelope = async (
  envelope: OperationEnvelopeV1,
): Promise<void> => {
  requireMasterKey();
  await writeValue(
    OUTBOX_STORE,
    `${activeUsername}:${envelope.client_op_id}`,
    envelope,
  );
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
      const operations: OperationEnvelopeV1[] = [];
      request.onsuccess = () => {
        const cursor = request.result;
        if (!cursor) {
          resolve(operations);
          return;
        }
        if (
          typeof cursor.key === "string" &&
          cursor.key.startsWith(`${activeUsername}:`)
        ) {
          operations.push(cursor.value as OperationEnvelopeV1);
        }
        cursor.continue();
      };
      request.onerror = () => reject(request.error ?? new Error("Outbox read failed."));
    });
  } finally {
    database.close();
  }
};

export const removeQueuedOperationEnvelope = async (clientOpId: string): Promise<void> => {
  requireMasterKey();
  const database = await openDatabase();
  try {
    await new Promise<void>((resolve, reject) => {
      const transaction = database.transaction(OUTBOX_STORE, "readwrite");
      transaction
        .objectStore(OUTBOX_STORE)
        .delete(`${activeUsername}:${clientOpId}`);
      transaction.oncomplete = () => resolve();
      transaction.onerror = () => reject(transaction.error ?? new Error("Outbox delete failed."));
    });
  } finally {
    database.close();
  }
};

/** Persists the decrypted PIM projection only as master-key-encrypted bytes. */
export const storeMaterializedPimItems = async (
  items: MaterializedPimItem[],
): Promise<void> => {
  const encrypted = await encryptVaultBytes(requireMasterKey(), encode(items));
  await writeValue(PIM_STATE_STORE, activeUsername, encrypted);
};

/** Restores the encrypted PIM projection for the active account. */
export const loadMaterializedPimItems = async (): Promise<MaterializedPimItem[]> => {
  const encrypted = await readValue<Uint8Array>(PIM_STATE_STORE, activeUsername);
  if (!encrypted) {
    return [];
  }
  const decoded = decode(await decryptVaultBytes(requireMasterKey(), encrypted));
  if (!Array.isArray(decoded)) {
    throw new Error("Stored PIM projection is invalid.");
  }
  return decoded as MaterializedPimItem[];
};

export const lockWebVault = (): void => {
  activeMasterKey?.fill(0);
  activeMasterKey = null;
  activeUsername = "";
  activeDevice = null;
};

/** Permanently removes this account's encrypted browser material after deletion. */
export const deleteWebVaultAccount = async (username: string): Promise<void> => {
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
        ],
        "readwrite",
      );
      transaction.objectStore(DEVICE_STORE).delete(username);
      transaction.objectStore(LOCAL_UNLOCK_STORE).delete(username);
      transaction.objectStore(PIM_STATE_STORE).delete(username);

      const outbox = transaction.objectStore(OUTBOX_STORE);
      const outboxCursor = outbox.openKeyCursor();
      outboxCursor.onsuccess = () => {
        const current = outboxCursor.result;
        if (!current) return;
        if (
          typeof current.key === "string" &&
          current.key.startsWith(`${username}:`)
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
          current.key.startsWith(`${username}:`)
        ) {
          spaceKeys.delete(current.key);
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
