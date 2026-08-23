import { browser } from "$app/environment";
import { writable } from "svelte/store";
import { normalizeCloudBaseUrl } from "$lib/endpoint";

/** In-memory collection descriptor hydrated from the encrypted vault/cloud. */
export interface CollectionEntry {
  id: string;
  name: string;
  keyAvailable: boolean;
  keyEpoch: number;
  role: "owner" | "editor" | "reader";
  syncedItems: number;
}

/**
 * Root web app state.
 *
 * Security note:
 * Access and TOTP-continuation tokens are memory-only and must never be persisted.
 * Refresh token is cookie-bound (`HttpOnly`) and not stored in app state.
 */
export interface AppState {
  cloudBaseUrl: string;
  currentUsername: string;
  accessToken: string | null;
  totpContinuationToken: string | null;
  collections: CollectionEntry[];
  syncedItemsTotal: number;
  lastSyncedSeq: number;
  notice: string;
}

const STORAGE_KEY = "kamori.web-frontend.app-state.v1";

const configuredCloudBaseUrl =
  (import.meta.env.VITE_KAMORI_API_BASE_URL as string | undefined)?.trim() ||
  "http://127.0.0.1:3000";

const defaultState: AppState = {
  cloudBaseUrl: configuredCloudBaseUrl,
  currentUsername: "",
  accessToken: null,
  totpContinuationToken: null,
  collections: [],
  syncedItemsTotal: 0,
  lastSyncedSeq: 0,
  notice: "",
};

/**
 * Loads the sole persisted connection preference from browser storage.
 */
const loadPersistedState = (): AppState => {
  if (!browser) {
    return defaultState;
  }

  const raw = window.localStorage.getItem(STORAGE_KEY);
  if (!raw) {
    return defaultState;
  }

  try {
    const parsed = JSON.parse(raw) as Partial<AppState>;
    return {
      ...defaultState,
      cloudBaseUrl: (() => {
        try {
          return typeof parsed.cloudBaseUrl === "string" && parsed.cloudBaseUrl
            ? normalizeCloudBaseUrl(parsed.cloudBaseUrl)
            : normalizeCloudBaseUrl(defaultState.cloudBaseUrl);
        } catch {
          return normalizeCloudBaseUrl(defaultState.cloudBaseUrl);
        }
      })(),
    };
  } catch {
    return defaultState;
  }
};

export const appState = writable<AppState>(loadPersistedState());

if (browser) {
  // Account identity, decrypted collection metadata, cursors, counters, and
  // auth state stay memory-only. The encrypted vault remains in IndexedDB.
  appState.subscribe((state) => {
    window.localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ cloudBaseUrl: state.cloudBaseUrl }),
    );
  });
}
