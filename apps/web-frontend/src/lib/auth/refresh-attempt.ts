const DATABASE_NAME = "kamori-auth-runtime";
const STORE_NAME = "refresh-attempts";
const DATABASE_VERSION = 1;

interface RefreshAttemptRecord {
  scope: string;
  requestId: string;
  createdAt: number;
}

const requestResult = <T>(request: IDBRequest<T>): Promise<T> =>
  new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error ?? new Error("IndexedDB request failed"));
  });

const transactionComplete = (transaction: IDBTransaction): Promise<void> =>
  new Promise((resolve, reject) => {
    transaction.oncomplete = () => resolve();
    transaction.onabort = () => reject(transaction.error ?? new Error("IndexedDB transaction aborted"));
    transaction.onerror = () => reject(transaction.error ?? new Error("IndexedDB transaction failed"));
  });

const openDatabase = (): Promise<IDBDatabase> =>
  new Promise((resolve, reject) => {
    if (typeof indexedDB === "undefined") {
      reject(new Error("Persistent browser storage is required to rotate the session safely."));
      return;
    }
    const request = indexedDB.open(DATABASE_NAME, DATABASE_VERSION);
    request.onupgradeneeded = () => {
      if (!request.result.objectStoreNames.contains(STORE_NAME)) {
        request.result.createObjectStore(STORE_NAME, { keyPath: "scope" });
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error ?? new Error("Unable to open browser auth storage"));
  });

const attemptScope = async (baseUrl: string, csrfToken: string): Promise<string> => {
  const input = new TextEncoder().encode(`${baseUrl}\0${csrfToken}`);
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", input));
  return Array.from(digest, (value) => value.toString(16).padStart(2, "0")).join("");
};

export const loadOrCreateRefreshAttempt = async (
  baseUrl: string,
  csrfToken: string,
): Promise<{ scope: string; requestId: string }> => {
  const scope = await attemptScope(baseUrl, csrfToken);
  const database = await openDatabase();
  try {
    const transaction = database.transaction(STORE_NAME, "readwrite");
    const store = transaction.objectStore(STORE_NAME);
    const existing = await requestResult(
      store.get(scope) as IDBRequest<RefreshAttemptRecord | undefined>,
    );
    const requestId = existing?.requestId ?? crypto.randomUUID();
    if (!existing) {
      store.put({ scope, requestId, createdAt: Date.now() } satisfies RefreshAttemptRecord);
    }
    await transactionComplete(transaction);
    return { scope, requestId };
  } finally {
    database.close();
  }
};

export const clearRefreshAttempt = async (scope: string): Promise<void> => {
  const database = await openDatabase();
  try {
    const transaction = database.transaction(STORE_NAME, "readwrite");
    transaction.objectStore(STORE_NAME).delete(scope);
    await transactionComplete(transaction);
  } finally {
    database.close();
  }
};
