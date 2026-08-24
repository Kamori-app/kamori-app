import { writable } from "svelte/store";

export type SyncPhase = "idle" | "syncing" | "offline" | "error";

export interface SyncState {
  phase: SyncPhase;
  lastSuccessAt: number | null;
  pendingOperations: number;
  error: string | null;
}

const initialState: SyncState = {
  phase: "idle",
  lastSuccessAt: null,
  pendingOperations: 0,
  error: null,
};

export const syncState = writable<SyncState>(initialState);

let manualSyncHandler: (() => void) | null = null;

export function registerManualSync(handler: () => void): () => void {
  manualSyncHandler = handler;
  return () => {
    if (manualSyncHandler === handler) manualSyncHandler = null;
  };
}

export function requestManualSync(): void {
  manualSyncHandler?.();
}

export function markSyncing(): void {
  syncState.update((state) => ({ ...state, phase: "syncing", error: null }));
}

export function markSyncSuccess(pendingOperations = 0): void {
  syncState.set({
    phase: "idle",
    lastSuccessAt: Date.now(),
    pendingOperations,
    error: null,
  });
}

export function markSyncFailure(message: string, offline: boolean): void {
  syncState.update((state) => ({
    ...state,
    phase: offline ? "offline" : "error",
    error: message,
  }));
}

export function setPendingOperations(pendingOperations: number): void {
  syncState.update((state) => ({ ...state, pendingOperations }));
}

export function resetSyncState(): void {
  syncState.set(initialState);
}
