import { browser } from "$app/environment";
import { writable } from "svelte/store";

/**
 * Persistent collection descriptor stored in web app local state.
 */
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
 * access/preauth tokens are memory-only and must never be persisted.
 * Refresh token is cookie-bound (`HttpOnly`) and not stored in app state.
 */
export interface AppState {
  cloudBaseUrl: string;
  currentUsername: string;
  accessToken: string | null;
  preauthToken: string | null;
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
  preauthToken: null,
  collections: [],
  syncedItemsTotal: 0,
  lastSyncedSeq: 0,
  notice: "",
};

/**
 * Runtime guard for validating persisted collection entries.
 */
const isValidCollection = (value: unknown): value is CollectionEntry => {
  if (!value || typeof value !== "object") {
    return false;
  }

  const candidate = value as Partial<CollectionEntry>;
  return (
    typeof candidate.id === "string" &&
    typeof candidate.name === "string" &&
    typeof candidate.keyAvailable === "boolean" &&
    typeof candidate.keyEpoch === "number" &&
    ["owner", "editor", "reader"].includes(candidate.role ?? "") &&
    typeof candidate.syncedItems === "number"
  );
};

/**
 * Loads persisted app state from browser storage with shape validation.
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
    const collections = Array.isArray(parsed.collections)
      ? parsed.collections.filter(isValidCollection)
      : defaultState.collections;

    const currentUsername =
      typeof parsed.currentUsername === "string"
        ? parsed.currentUsername
        : defaultState.currentUsername;

    return {
      ...defaultState,
      cloudBaseUrl:
        typeof parsed.cloudBaseUrl === "string" && parsed.cloudBaseUrl
          ? parsed.cloudBaseUrl
          : defaultState.cloudBaseUrl,
      currentUsername,
      accessToken: defaultState.accessToken,
      preauthToken: defaultState.preauthToken,
      syncedItemsTotal:
        typeof parsed.syncedItemsTotal === "number"
          ? parsed.syncedItemsTotal
          : defaultState.syncedItemsTotal,
      lastSyncedSeq:
        typeof parsed.lastSyncedSeq === "number"
          ? parsed.lastSyncedSeq
          : defaultState.lastSyncedSeq,
      collections,
      notice: defaultState.notice,
    };
  } catch {
    return defaultState;
  }
};

export const appState = writable<AppState>(loadPersistedState());

if (browser) {
  // Persist only non-sensitive state. Auth fields stay memory-only.
  appState.subscribe((state) => {
    const {
      notice: _notice,
      accessToken: _accessToken,
      preauthToken: _preauthToken,
      ...persisted
    } = state;
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(persisted));
  });
}
