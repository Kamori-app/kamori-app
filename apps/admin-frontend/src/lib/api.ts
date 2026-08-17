import { decode, encode } from "@msgpack/msgpack";

export interface AuthStartResponse {
  flow_id: string;
  public_key_credential_request_options: Uint8Array;
}

export interface Dashboard {
  active_accounts: number;
  suspended_accounts: number;
  total_blob_storage_bytes: number;
  pending_blobs: number;
  pending_object_deletions: number;
  registration_enabled: boolean;
  beta_account_limit: number;
  latest_migration?: string | null;
  jobs: Array<{
    job_name: string;
    status: string;
    details: unknown;
    updated_at_unix_ms: number;
    last_succeeded_at_unix_ms?: number | null;
  }>;
  security_keys: Array<{
    id: string;
    name: string;
    created_at_unix_ms: number;
    last_used_at_unix_ms?: number | null;
  }>;
}

export interface RuntimeSetting {
  key: string;
  value: unknown;
  version: number;
  updated_at_unix_ms?: number | null;
  overridden: boolean;
}

export interface AuditEntry {
  id: string;
  actor_username?: string | null;
  event_kind: string;
  target_kind?: string | null;
  target_id?: string | null;
  reason?: string | null;
  details: unknown;
  created_at_unix_ms: number;
}

const request = async <T>(
  baseUrl: string,
  path: string,
  method: "GET" | "POST",
  payload?: unknown,
  token?: string,
): Promise<T> => {
  const response = await fetch(`${baseUrl.replace(/\/$/, "")}${path}`, {
    method,
    headers: {
      Accept: "application/msgpack",
      ...(method === "POST" ? { "Content-Type": "application/msgpack" } : {}),
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
    },
    ...(payload === undefined ? {} : { body: encode(payload) }),
  });
  const bytes = new Uint8Array(await response.arrayBuffer());
  if (!response.ok) {
    let message = `Request failed (${response.status})`;
    try {
      message = (decode(bytes) as { message?: string }).message ?? message;
    } catch {
      // Keep status fallback.
    }
    throw new Error(message);
  }
  return decode(bytes) as T;
};

export const adminApi = {
  bootstrapStart: (
    baseUrl: string,
    payload: { username: string; bootstrap_token: string; totp_code: string },
  ) =>
    request<{
      flow_id: string;
      public_key_credential_creation_options: Uint8Array;
    }>(baseUrl, "/admin-api/bootstrap/start", "POST", payload),
  bootstrapFinish: (
    baseUrl: string,
    payload: {
      username: string;
      bootstrap_token: string;
      totp_code: string;
      flow_id: string;
      credential: Uint8Array;
    },
  ) => request<{ changed: boolean }>(baseUrl, "/admin-api/bootstrap/finish", "POST", payload),
  authStart: (baseUrl: string, username: string) =>
    request<AuthStartResponse>(baseUrl, "/admin-api/auth/start", "POST", { username }),
  authFinish: (
    baseUrl: string,
    payload: { username: string; flow_id: string; credential: Uint8Array; totp_code: string },
  ) =>
    request<{ token: string; expires_at_unix_ms: number }>(
      baseUrl,
      "/admin-api/auth/finish",
      "POST",
      payload,
    ),
  reauthStart: (baseUrl: string, token: string) =>
    request<AuthStartResponse>(baseUrl, "/admin-api/auth/reauth/start", "POST", {}, token),
  reauthFinish: (
    baseUrl: string,
    token: string,
    payload: { username: string; flow_id: string; credential: Uint8Array; totp_code: string },
  ) =>
    request<{ token: string; expires_at_unix_ms: number }>(
      baseUrl,
      "/admin-api/auth/reauth/finish",
      "POST",
      payload,
      token,
    ),
  logout: (baseUrl: string, token: string) =>
    request<{ changed: boolean }>(baseUrl, "/admin-api/auth/logout", "POST", {}, token),
  addSecurityKeyStart: (baseUrl: string, token: string) =>
    request<{
      flow_id: string;
      public_key_credential_creation_options: Uint8Array;
    }>(baseUrl, "/admin-api/security-keys/add/start", "POST", {}, token),
  addSecurityKeyFinish: (baseUrl: string, token: string, payload: unknown) =>
    request<{ changed: boolean }>(
      baseUrl,
      "/admin-api/security-keys/add/finish",
      "POST",
      payload,
      token,
    ),
  removeSecurityKey: (baseUrl: string, token: string, payload: unknown) =>
    request<{ changed: boolean }>(
      baseUrl,
      "/admin-api/security-keys/remove",
      "POST",
      payload,
      token,
    ),
  dashboard: (baseUrl: string, token: string) =>
    request<Dashboard>(baseUrl, "/admin-api/dashboard", "GET", undefined, token),
  settings: (baseUrl: string, token: string) =>
    request<{ settings: RuntimeSetting[] }>(baseUrl, "/admin-api/settings", "GET", undefined, token),
  updateSetting: (baseUrl: string, token: string, payload: unknown) =>
    request<{ changed: boolean }>(baseUrl, "/admin-api/settings", "POST", payload, token),
  suspend: (baseUrl: string, token: string, payload: unknown) =>
    request<{ changed: boolean }>(baseUrl, "/admin-api/accounts/suspension", "POST", payload, token),
  audit: (baseUrl: string, token: string) =>
    request<{ entries: AuditEntry[] }>(baseUrl, "/admin-api/audit", "GET", undefined, token),
};
