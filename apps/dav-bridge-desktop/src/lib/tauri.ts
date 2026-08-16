import { invoke } from "@tauri-apps/api/core";

export type CloseBehavior = "quit" | "hide" | "minimize";

export interface LocalServerStatus {
  running: boolean;
  bind_addr: string;
}

export interface DavCollectionEndpoint {
  collection_id: string;
  name: string;
  calendar_url: string;
  address_book_url: string;
}

export interface DavConnectionInfo {
  bind_addr: string;
  username: string;
  password: string;
  collections: DavCollectionEndpoint[];
}

export interface LogoutResult {
  server_session_revoked: boolean;
  warning?: string | null;
}

export interface DashboardSnapshot {
  has_access_token: boolean;
  server: LocalServerStatus;
  collections_total: number;
  synced_items_total: number;
}

export interface CollectionSummary {
  id: string;
  name: string;
  synced_items: number;
}

export interface OpaqueSigninFinishResponse {
  access_token?: string | null;
  refresh_token?: string | null;
  refresh_token_id?: string | null;
  totp_verified: boolean;
  encrypted_master_key: number[];
  public_key_bundle: number[];
  preauth_token?: string | null;
}

export interface PasskeyLoginStartResponse {
  flow_id: string;
  challenge: number[];
  public_key_credential_request_options: number[];
}

export interface PasskeyLoginFinishResponse {
  username: string;
  access_token: string;
  refresh_token?: string | null;
  refresh_token_id?: string | null;
}

export const api = {
  configureBackend: (cloudBaseUrl: string) =>
    invoke<void>("configure_backend", {
      cloudBaseUrl,
    }),

  applyWindowPreferences: (
    closeBehavior: CloseBehavior,
    showTrayIcon: boolean,
  ) =>
    invoke<void>("apply_window_preferences", {
      closeBehavior,
      showTrayIcon,
    }),

  passwordLogin: (username: string, password: string, totpCode?: string) =>
    invoke<OpaqueSigninFinishResponse>("password_login", {
      username,
      password,
      totpCode,
    }),

  passkeyLoginStart: () =>
    invoke<PasskeyLoginStartResponse>("passkey_login_start"),

  passkeyLoginFinish: (credential: number[], flowId: string) =>
    invoke<PasskeyLoginFinishResponse>("passkey_login_finish", {
      flowId,
      credential,
    }),

  startLocalServer: () => invoke<LocalServerStatus>("start_local_server"),
  stopLocalServer: () => invoke<LocalServerStatus>("stop_local_server"),
  localServerStatus: () => invoke<LocalServerStatus>("local_server_status"),
  davConnectionInfo: () => invoke<DavConnectionInfo>("dav_connection_info"),
  rotateDavCredentials: () =>
    invoke<DavConnectionInfo>("rotate_dav_credentials"),
  syncNow: () => invoke<number>("sync_now"),

  createCollection: (name: string) =>
    invoke<CollectionSummary>("create_collection", { name }),
  listCollections: () => invoke<CollectionSummary[]>("list_collections"),

  dashboardSnapshot: () => invoke<DashboardSnapshot>("dashboard_snapshot"),
  logout: () => invoke<LogoutResult>("logout"),
};
