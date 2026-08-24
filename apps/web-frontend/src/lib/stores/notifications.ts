import { writable } from "svelte/store";

export type NotificationKind = "success" | "info" | "warning" | "error";

export interface AppNotification {
  id: string;
  kind: NotificationKind;
  message: string;
  source?: string;
  actionLabel?: string;
  onAction?: () => void;
  persistent: boolean;
}

const notifications = writable<AppNotification[]>([]);

const dismissAfter = (id: string, milliseconds: number) => {
  if (typeof window === "undefined") return;
  window.setTimeout(() => dismissNotification(id), milliseconds);
};

export const notificationStore = {
  subscribe: notifications.subscribe,
};

export function notify(
  message: string,
  options: Partial<Omit<AppNotification, "id" | "message">> = {},
): string {
  const id = crypto.randomUUID();
  const entry: AppNotification = {
    id,
    kind: options.kind ?? "info",
    message,
    source: options.source,
    actionLabel: options.actionLabel,
    onAction: options.onAction,
    persistent: options.persistent ?? options.kind === "error",
  };
  notifications.update((items) => [...items.slice(-4), entry]);
  if (!entry.persistent) {
    dismissAfter(id, entry.kind === "success" ? 4_000 : 7_000);
  }
  return id;
}

export function dismissNotification(id: string): void {
  notifications.update((items) => items.filter((item) => item.id !== id));
}

export function clearNotifications(source?: string): void {
  notifications.update((items) =>
    source ? items.filter((item) => item.source !== source) : [],
  );
}
