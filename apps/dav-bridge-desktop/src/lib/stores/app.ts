import { writable } from 'svelte/store';
import type { CloseBehavior } from '../tauri';

export interface SessionState {
  hasSession: boolean;
  serverRunning: boolean;
  bindAddr: string;
  collectionsTotal: number;
  syncedItemsTotal: number;
}

export const session = writable<SessionState>({
  hasSession: false,
  serverRunning: false,
  bindAddr: '127.0.0.1:8181',
  collectionsTotal: 0,
  syncedItemsTotal: 0,
});

export const loginNotice = writable<string>('');

export interface BackendSettingsState {
  cloudBaseUrl: string;
}

const defaultBackendSettings: BackendSettingsState = {
  cloudBaseUrl: 'http://127.0.0.1:3000',
};

const backendStorageKey = 'kamori.desktop.backend-settings.v2';

const loadBackendSettings = (): BackendSettingsState => {
  if (typeof window === 'undefined') {
    return defaultBackendSettings;
  }

  const raw = window.localStorage.getItem(backendStorageKey);
  if (!raw) {
    return defaultBackendSettings;
  }

  try {
    const parsed = JSON.parse(raw) as Partial<BackendSettingsState>;
    return {
      cloudBaseUrl: parsed.cloudBaseUrl ?? defaultBackendSettings.cloudBaseUrl,
    };
  } catch {
    return defaultBackendSettings;
  }
};

export const backendSettings = writable<BackendSettingsState>(loadBackendSettings());

export const saveBackendSettings = (next: BackendSettingsState) => {
  backendSettings.set(next);
  if (typeof window !== 'undefined') {
    window.localStorage.setItem(backendStorageKey, JSON.stringify(next));
  }
};

export interface WindowPreferencesState {
  closeBehavior: CloseBehavior;
  showTrayIcon: boolean;
}

const defaultWindowPreferences: WindowPreferencesState = {
  closeBehavior: 'quit',
  showTrayIcon: false,
};

const windowPrefsStorageKey = 'kamori.desktop.window-preferences.v1';

const loadWindowPreferences = (): WindowPreferencesState => {
  if (typeof window === 'undefined') {
    return defaultWindowPreferences;
  }

  const raw = window.localStorage.getItem(windowPrefsStorageKey);
  if (!raw) {
    return defaultWindowPreferences;
  }

  try {
    const parsed = JSON.parse(raw) as Partial<WindowPreferencesState>;
    const closeBehavior =
      parsed.closeBehavior === 'hide' || parsed.closeBehavior === 'minimize'
        ? parsed.closeBehavior
        : 'quit';

    return {
      closeBehavior,
      showTrayIcon: parsed.showTrayIcon ?? defaultWindowPreferences.showTrayIcon,
    };
  } catch {
    return defaultWindowPreferences;
  }
};

export const windowPreferences = writable<WindowPreferencesState>(loadWindowPreferences());

export const saveWindowPreferences = (next: WindowPreferencesState) => {
  windowPreferences.set(next);
  if (typeof window !== 'undefined') {
    window.localStorage.setItem(windowPrefsStorageKey, JSON.stringify(next));
  }
};
