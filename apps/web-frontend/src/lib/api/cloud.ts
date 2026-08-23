import { deleteMsgpack, getMsgpack, postMsgpack } from "./msgpack";
import {
  CSRF_HEADER,
  REFRESH_TRANSPORT_HEADER,
} from "$lib/auth/cookie-csrf.js";
import type { OperationEnvelopeV1 } from "$lib/opaqueClient";
import {
  clearRefreshAttempt,
  loadOrCreateRefreshAttempt,
} from "$lib/auth/refresh-attempt.js";

const COOKIE_REFRESH_TRANSPORT_OPTIONS = {
  headers: { [REFRESH_TRANSPORT_HEADER]: "cookie" },
  credentials: "include" as const,
};

let browserCsrfToken: string | null = null;
let browserCsrfScope: string | null = null;

const rememberBrowserCsrf = (baseUrl: string, token?: string | null) => {
  browserCsrfScope = token ? baseUrl : null;
  browserCsrfToken = token || null;
};

const browserCsrf = async (baseUrl: string): Promise<string> => {
  if (browserCsrfScope === baseUrl && browserCsrfToken) {
    return browserCsrfToken;
  }
  const response = await postMsgpack<
    Record<string, never>,
    { csrf_token: string }
  >(baseUrl, "/auth/csrf", {}, undefined, COOKIE_REFRESH_TRANSPORT_OPTIONS);
  if (!response.csrf_token) {
    throw new Error("CSRF bootstrap returned an empty token.");
  }
  rememberBrowserCsrf(baseUrl, response.csrf_token);
  return response.csrf_token;
};

const cookieRefreshWithCsrfOptions = async (baseUrl: string) => ({
  headers: {
    [REFRESH_TRANSPORT_HEADER]: "cookie",
    [CSRF_HEADER]: await browserCsrf(baseUrl),
  },
  credentials: "include" as const,
});

/**
 * Cloud API request/response contracts for MessagePack endpoints.
 *
 * Field names intentionally match backend payload names (`snake_case`).
 */
export interface OpaqueSignupStartRequest {
  username: string;
  opaque_start_request: Uint8Array;
}

export interface OpaqueSignupStartResponse {
  opaque_server_message: Uint8Array;
}

export interface OpaqueSignupFinishRequest {
  signup_request_id: string;
  username: string;
  opaque_finish_request: Uint8Array;
  encrypted_master_key: Uint8Array;
  public_key_bundle: Uint8Array;
  recovery_verifier: Uint8Array;
}

export interface OpaqueSignupFinishResponse {
  user_id: string;
}

export interface PasswordChangeStartRequest {
  opaque_start_request: Uint8Array;
}

export interface PasswordChangeStartResponse {
  opaque_server_message: Uint8Array;
}

export interface PasswordChangeFinishRequest {
  reauth_token: string;
  opaque_finish_request: Uint8Array;
  encrypted_master_key: Uint8Array;
}

export interface PasswordChangeFinishResponse {
  changed: boolean;
}

export interface AccountRecoveryStartRequest {
  username: string;
  recovery_verifier: Uint8Array;
  opaque_start_request: Uint8Array;
}

export interface AccountRecoveryStartResponse {
  opaque_server_message: Uint8Array;
  recovery_token: string;
}

export interface AccountRecoveryFinishRequest {
  recovery_token: string;
  opaque_finish_request: Uint8Array;
  encrypted_master_key: Uint8Array;
}

export interface AccountRecoveryFinishResponse {
  changed: boolean;
  totp_disabled: boolean;
  space_key_packages: Array<{
    space_id: string;
    key_epoch: number;
    encrypted_key_package: Uint8Array;
  }>;
}

export interface OpaqueSigninStartRequest {
  username: string;
  opaque_start_request: Uint8Array;
}

export type OpaqueSigninNextStep = "continue" | "totp_required";

export interface OpaqueSigninStartResponse {
  opaque_flow_id: string;
  opaque_server_message: Uint8Array;
  next_step: OpaqueSigninNextStep;
}

export interface OpaqueSigninFinishRequest {
  username: string;
  opaque_flow_id: string;
  opaque_finish_request: Uint8Array;
  totp_code?: string | null;
}

export interface OpaqueSigninFinishResponse {
  access_token?: string | null;
  refresh_token?: string | null;
  refresh_token_id?: string | null;
  totp_verified: boolean;
  encrypted_master_key: Uint8Array;
  public_key_bundle: Uint8Array;
  totp_continuation_token?: string | null;
  device_enrollment_token?: string | null;
  csrf_token?: string | null;
}

export interface SigninTotpRequest {
  continuation_token: string;
  totp_code: string;
}

export interface PasskeyLoginStartResponse {
  flow_id: string | Uint8Array;
  challenge: Uint8Array;
  public_key_credential_request_options: Uint8Array;
}

export interface PasskeyLoginFinishResponse {
  username: string;
  access_token: string;
  refresh_token?: string | null;
  refresh_token_id?: string | null;
  device_enrollment_token: string;
  csrf_token?: string | null;
}

export interface RefreshRequest {
  refresh_token?: string | null;
  rotation_request_id: string;
}

export interface RefreshResponse {
  access_token: string;
  username: string;
  refresh_token?: string | null;
  refresh_token_id?: string | null;
  csrf_token?: string | null;
}

export interface LogoutRequest {
  refresh_token?: string | null;
}

export interface LogoutResponse {
  revoked: boolean;
}

export interface DeviceAuthorizationApproveRequest {
  user_code: string;
  encrypted_master_key_package: Uint8Array;
}

export interface DeviceAuthorizationApproveResponse {
  approved: boolean;
}

export interface DeviceAuthorizationInspectRequest {
  user_code: string;
}

export interface DeviceAuthorizationInspectResponse {
  flow_id: string;
  hpke_public_key: Uint8Array;
}

export interface SessionSummary {
  refresh_token_id: string;
  device_id?: string | null;
  is_current: boolean;
  user_agent?: string | null;
  ip_address?: string | null;
  created_at_unix_ms: number;
  last_used_at_unix_ms?: number | null;
  expires_at_unix_ms: number;
  revoked_at_unix_ms?: number | null;
}

export interface TotpStatusResponse {
  available: boolean;
  enabled: boolean;
  recovery_codes_remaining: number;
}

export interface TotpSetupStartResponse {
  flow_id: string;
  manual_entry_key: string;
  otpauth_uri: string;
}

export interface TotpSetupFinishResponse {
  enabled: boolean;
  recovery_codes: string[];
}

export interface TotpDisableResponse {
  enabled: boolean;
}

export interface AccountRecoveryCodesRegenerateResponse {
  recovery_codes: string[];
}

export interface PasskeyMetadata {
  id: string;
  credential_id: Uint8Array;
  encrypted_name: Uint8Array;
}

export interface PasskeyAddStartResponse {
  flow_id: string | Uint8Array;
  challenge: Uint8Array;
  public_key_credential_creation_options: Uint8Array;
}

export interface PasskeyAddFinishResponse {
  passkey: PasskeyMetadata;
}

export interface PasskeyListResponse {
  passkeys: PasskeyMetadata[];
}

export interface PasskeyUpdateResponse {
  passkey: PasskeyMetadata;
}

export interface PasskeyDeleteResponse {
  deleted: boolean;
}

export type DevicePlatform = "web" | "desktop" | "android" | "ios";

export interface RegisterDeviceRequest {
  enrollment_token: string;
  device_id: string;
  signing_public_key: Uint8Array;
  hpke_public_key: Uint8Array;
  encrypted_name: Uint8Array;
  platform: DevicePlatform;
}

export interface DeviceSummary extends Omit<RegisterDeviceRequest, "enrollment_token"> {
  created_at_unix_ms: number;
  last_seen_at_unix_ms?: number | null;
}

export interface DeviceKeyPackage {
  device_id: string;
  key_epoch: number;
  encrypted_key_package: Uint8Array;
}

export interface RecoverySpaceKeyPackage {
  space_id: string;
  key_epoch: number;
  encrypted_key_package: Uint8Array;
}

export interface CreateSpaceRequest {
  workspace_id?: string | null;
  space_id: string;
  encrypted_metadata: Uint8Array;
  device_key_packages: DeviceKeyPackage[];
  encrypted_recovery_key_package: Uint8Array;
}

export interface SpaceSummary {
  space_id: string;
  workspace_id: string;
  role: "owner" | "editor" | "reader";
  key_epoch: number;
  history_start_seq: number;
  current_state_start_seq: number;
  encrypted_metadata: Uint8Array;
  device_key_packages: DeviceKeyPackage[];
  created_at_unix_ms: number;
}

export interface SpaceDeviceSummary {
  device_id: string;
  user_id: string;
  active: boolean;
  signing_public_key: Uint8Array;
  hpke_public_key: Uint8Array;
}

export interface SpaceMemberSummary {
  user_id: string;
  username: string;
  role: "owner" | "editor" | "reader";
  key_epoch: number;
  public_key_bundle: Uint8Array;
}

export interface MemberRecoveryKeyPackage {
  user_id: string;
  key_epoch: number;
  encrypted_key_package: Uint8Array;
}

export interface RotateSpaceKeyRequest {
  rotation_id: string;
  expected_key_epoch: number;
  new_key_epoch: number;
  base_space_seq: number;
  new_encrypted_metadata: Uint8Array;
  remaining_device_packages: DeviceKeyPackage[];
  remaining_recovery_packages: MemberRecoveryKeyPackage[];
  snapshots: OperationEnvelopeV1[];
  quarantined_streams: string[];
}

export type RevokeSpaceMemberRequest = RotateSpaceKeyRequest;

export interface WorkspaceSummary {
  workspace_id: string;
  kind: "personal" | "team";
  role: "owner" | "admin" | "member";
  encrypted_metadata: Uint8Array;
}

export interface WorkspaceMember {
  user_id: string;
  username: string;
  role: "owner" | "admin" | "member";
}

export type OwnershipResourceKind = "workspace" | "security_space";

export interface OwnershipTransferOffer {
  transfer_id: string;
  resource_kind: OwnershipResourceKind;
  resource_id: string;
  current_owner_id: string;
  current_owner_username: string;
  target_user_id: string;
  expires_at_unix_ms: number;
  created_at_unix_ms: number;
}

export interface DeletionStatusResponse {
  can_delete: boolean;
  shared_workspaces_owned: number;
  shared_spaces_owned: number;
}

export interface ReauthStartResponse {
  opaque_flow_id: string;
  opaque_server_message: Uint8Array;
  totp_required: boolean;
}

export type ReauthAction =
  | "change_password"
  | "delete_account"
  | "recovery_settings"
  | "security_settings";

export interface StoredOperation {
  space_seq: number;
  received_at_unix_ms: number;
  envelope: OperationEnvelopeV1;
}

export interface CreateInviteCodeRequest {
  space_id: string;
  rotation_id: string;
  role: "editor" | "reader";
  invite_code_hash: Uint8Array;
  encrypted_key_package: Uint8Array;
  encrypted_note?: Uint8Array;
  ttl_minutes: number;
}

export interface CreateInviteCodeResponse {
  id: string;
}

export interface RedeemInviteCodeResponse {
  space_id: string;
  role: "editor" | "reader";
  key_epoch: number;
  history_start_seq: number;
  current_state_start_seq: number;
  encrypted_key_package: Uint8Array;
  encrypted_note?: Uint8Array;
}

export interface ConsentSettings {
  product_analytics: boolean;
  crash_reports: boolean;
  marketing: boolean;
  policy_version: number;
  updated_at_unix_ms?: number | null;
}

export interface UpdateConsentSettingsRequest {
  product_analytics: boolean;
  crash_reports: boolean;
  marketing: boolean;
}

/**
 * Typed API surface used by web pages/components.
 */
export const cloudApi = {
  signupStart: (baseUrl: string, payload: OpaqueSignupStartRequest) =>
    postMsgpack<OpaqueSignupStartRequest, OpaqueSignupStartResponse>(
      baseUrl,
      "/auth/signup/start",
      payload,
    ),

  signupFinish: (baseUrl: string, payload: OpaqueSignupFinishRequest) =>
    postMsgpack<OpaqueSignupFinishRequest, OpaqueSignupFinishResponse>(
      baseUrl,
      "/auth/signup/finish",
      payload,
    ),

  passwordChangeStart: (
    baseUrl: string,
    payload: PasswordChangeStartRequest,
    accessToken: string,
  ) =>
    postMsgpack<PasswordChangeStartRequest, PasswordChangeStartResponse>(
      baseUrl,
      "/auth/password/change/start",
      payload,
      accessToken,
    ),

  passwordChangeFinish: (
    baseUrl: string,
    payload: PasswordChangeFinishRequest,
    accessToken: string,
  ) =>
    postMsgpack<PasswordChangeFinishRequest, PasswordChangeFinishResponse>(
      baseUrl,
      "/auth/password/change/finish",
      payload,
      accessToken,
    ),

  accountRecoveryStart: (
    baseUrl: string,
    payload: AccountRecoveryStartRequest,
  ) =>
    postMsgpack<AccountRecoveryStartRequest, AccountRecoveryStartResponse>(
      baseUrl,
      "/auth/account-recovery/start",
      payload,
    ),

  accountRecoveryFinish: (
    baseUrl: string,
    payload: AccountRecoveryFinishRequest,
  ) =>
    postMsgpack<AccountRecoveryFinishRequest, AccountRecoveryFinishResponse>(
      baseUrl,
      "/auth/account-recovery/finish",
      payload,
    ),

  signinStart: (baseUrl: string, payload: OpaqueSigninStartRequest) =>
    postMsgpack<OpaqueSigninStartRequest, OpaqueSigninStartResponse>(
      baseUrl,
      "/auth/signin/start",
      payload,
    ),

  signinFinish: async (baseUrl: string, payload: OpaqueSigninFinishRequest) => {
    const response = await postMsgpack<
      OpaqueSigninFinishRequest,
      OpaqueSigninFinishResponse
    >(
      baseUrl,
      "/auth/signin/finish",
      payload,
      undefined,
      COOKIE_REFRESH_TRANSPORT_OPTIONS,
    );
    rememberBrowserCsrf(baseUrl, response.csrf_token);
    return response;
  },

  signinTotp: async (baseUrl: string, payload: SigninTotpRequest) => {
    const response = await postMsgpack<
      SigninTotpRequest,
      OpaqueSigninFinishResponse
    >(
      baseUrl,
      "/auth/signin/totp",
      payload,
      undefined,
      COOKIE_REFRESH_TRANSPORT_OPTIONS,
    );
    rememberBrowserCsrf(baseUrl, response.csrf_token);
    return response;
  },

  refresh: async (baseUrl: string) => {
    const csrfToken = await browserCsrf(baseUrl);
    const attempt = await loadOrCreateRefreshAttempt(baseUrl, csrfToken);
    try {
      const response = await postMsgpack<RefreshRequest, RefreshResponse>(
        baseUrl,
        "/auth/refresh",
        { rotation_request_id: attempt.requestId },
        undefined,
        await cookieRefreshWithCsrfOptions(baseUrl),
      );
      rememberBrowserCsrf(baseUrl, response.csrf_token);
      try {
        await clearRefreshAttempt(attempt.scope);
      } catch {
        // The old scope is inert after CSRF rotation. Cleanup failure must not
        // turn an already-successful session rotation into an application error.
      }
      return response;
    } catch (error) {
      // The response headers may already have advanced the HttpOnly refresh
      // and CSRF cookies. Force a fresh bootstrap before any retry.
      rememberBrowserCsrf(baseUrl, null);
      throw error;
    }
  },

  logout: async (baseUrl: string, accessToken: string) => {
    try {
      return await postMsgpack<LogoutRequest, LogoutResponse>(
        baseUrl,
        "/auth/logout",
        {},
        accessToken,
        await cookieRefreshWithCsrfOptions(baseUrl),
      );
    } finally {
      rememberBrowserCsrf(baseUrl, null);
    }
  },

  inspectDeviceAuthorization: (
    baseUrl: string,
    userCode: string,
    accessToken: string,
  ) =>
    postMsgpack<
      DeviceAuthorizationInspectRequest,
      DeviceAuthorizationInspectResponse
    >(
      baseUrl,
      "/auth/device-authorization/inspect",
      { user_code: userCode },
      accessToken,
    ),

  approveDeviceAuthorization: (
    baseUrl: string,
    userCode: string,
    encryptedMasterKeyPackage: Uint8Array,
    accessToken: string,
  ) =>
    postMsgpack<
      DeviceAuthorizationApproveRequest,
      DeviceAuthorizationApproveResponse
    >(
      baseUrl,
      "/auth/device-authorization/approve",
      {
        user_code: userCode,
        encrypted_master_key_package: encryptedMasterKeyPackage,
      },
      accessToken,
    ),

  listSessions: (baseUrl: string, accessToken: string) =>
    getMsgpack<{ sessions: SessionSummary[] }>(
      baseUrl,
      "/auth/sessions",
      accessToken,
    ),

  revokeSession: (
    baseUrl: string,
    refreshTokenId: string,
    reauthToken: string,
    accessToken: string,
  ) =>
    postMsgpack<
      { refresh_token_id: string; reauth_token: string },
      { revoked: boolean }
    >(
      baseUrl,
      "/auth/revoke",
      { refresh_token_id: refreshTokenId, reauth_token: reauthToken },
      accessToken,
    ),

  totpStatus: (baseUrl: string, accessToken: string) =>
    postMsgpack<Record<string, never>, TotpStatusResponse>(
      baseUrl,
      "/auth/totp/status",
      {},
      accessToken,
    ),

  totpSetupStart: (
    baseUrl: string,
    reauthToken: string,
    accessToken: string,
  ) =>
    postMsgpack<{ reauth_token: string }, TotpSetupStartResponse>(
      baseUrl,
      "/auth/totp/setup/start",
      { reauth_token: reauthToken },
      accessToken,
    ),

  totpSetupFinish: (
    baseUrl: string,
    payload: { flow_id: string; code: string },
    accessToken: string,
  ) =>
    postMsgpack<
      { flow_id: string; code: string },
      TotpSetupFinishResponse
    >(baseUrl, "/auth/totp/setup/finish", payload, accessToken),

  totpDisable: (
    baseUrl: string,
    payload: { reauth_token: string; code?: string },
    accessToken: string,
  ) =>
    postMsgpack<{ reauth_token: string; code?: string }, TotpDisableResponse>(
      baseUrl,
      "/auth/totp/disable",
      payload,
      accessToken,
    ),

  accountRecoveryCodesRegenerate: (
    baseUrl: string,
    reauthToken: string,
    accessToken: string,
  ) =>
    postMsgpack<{ reauth_token: string }, AccountRecoveryCodesRegenerateResponse>(
      baseUrl,
      "/auth/account-recovery/codes/regenerate",
      { reauth_token: reauthToken },
      accessToken,
    ),

  passkeyLoginStart: (baseUrl: string) =>
    postMsgpack<Record<string, never>, PasskeyLoginStartResponse>(
      baseUrl,
      "/auth/passkey/login/start",
      {},
    ),

  passkeyLoginFinish: async (
    baseUrl: string,
    credential: Uint8Array,
    flowId: string | Uint8Array,
  ) => {
    const response = await postMsgpack<
      { flow_id: string | Uint8Array; credential: Uint8Array },
      PasskeyLoginFinishResponse
    >(
      baseUrl,
      "/auth/passkey/login/finish",
      {
        flow_id: flowId,
        credential,
      },
      undefined,
      COOKIE_REFRESH_TRANSPORT_OPTIONS,
    );
    rememberBrowserCsrf(baseUrl, response.csrf_token);
    return response;
  },

  passkeyAddStart: (baseUrl: string, reauthToken: string, accessToken: string) =>
    postMsgpack<{ reauth_token: string }, PasskeyAddStartResponse>(
      baseUrl,
      "/auth/passkey/add/start",
      { reauth_token: reauthToken },
      accessToken,
    ),

  passkeyAddFinish: (
    baseUrl: string,
    flowId: string | Uint8Array,
    credential: Uint8Array,
    encryptedName: Uint8Array,
    accessToken: string,
  ) =>
    postMsgpack<
      {
        flow_id: string | Uint8Array;
        credential: Uint8Array;
        encrypted_name: Uint8Array;
      },
      PasskeyAddFinishResponse
    >(
      baseUrl,
      "/auth/passkey/add/finish",
      {
        flow_id: flowId,
        credential,
        encrypted_name: encryptedName,
      },
      accessToken,
    ),

  listPasskeys: (baseUrl: string, accessToken: string) =>
    getMsgpack<PasskeyListResponse>(baseUrl, "/auth/passkeys", accessToken),

  updatePasskey: (
    baseUrl: string,
    passkeyId: string,
    encryptedName: Uint8Array,
    accessToken: string,
  ) =>
    postMsgpack<
      { passkey_id: string; encrypted_name: Uint8Array },
      PasskeyUpdateResponse
    >(
      baseUrl,
      "/auth/passkey/update",
      { passkey_id: passkeyId, encrypted_name: encryptedName },
      accessToken,
    ),

  deletePasskey: (
    baseUrl: string,
    passkeyId: string,
    reauthToken: string,
    accessToken: string,
  ) =>
    postMsgpack<{ passkey_id: string; reauth_token: string }, PasskeyDeleteResponse>(
      baseUrl,
      "/auth/passkey/delete",
      { passkey_id: passkeyId, reauth_token: reauthToken },
      accessToken,
    ),

  registerDevice: (
    baseUrl: string,
    payload: RegisterDeviceRequest,
    accessToken: string,
  ) =>
    postMsgpack<RegisterDeviceRequest, { device: DeviceSummary }>(
      baseUrl,
      "/devices",
      payload,
      accessToken,
    ),

  listDevices: (baseUrl: string, accessToken: string) =>
    getMsgpack<{ devices: DeviceSummary[] }>(baseUrl, "/devices", accessToken),

  revokeDevice: (
    baseUrl: string,
    deviceId: string,
    reauthToken: string,
    accessToken: string,
  ) =>
    postMsgpack<{ reauth_token: string }, { revoked: boolean }>(
      baseUrl,
      `/devices/${encodeURIComponent(deviceId)}/revoke`,
      { reauth_token: reauthToken },
      accessToken,
    ),

  createSpace: (
    baseUrl: string,
    payload: CreateSpaceRequest,
    accessToken: string,
  ) =>
    postMsgpack<CreateSpaceRequest, { space: SpaceSummary }>(
      baseUrl,
      "/spaces",
      payload,
      accessToken,
    ),

  listSpaces: (baseUrl: string, accessToken: string) =>
    getMsgpack<{ spaces: SpaceSummary[] }>(baseUrl, "/spaces", accessToken),

  listTrashedSpaces: (baseUrl: string, accessToken: string) =>
    getMsgpack<{ spaces: SpaceSummary[] }>(baseUrl, "/spaces/trash", accessToken),

  moveSpaceToTrash: (baseUrl: string, spaceId: string, accessToken: string) =>
    deleteMsgpack<{ changed: boolean }>(
      baseUrl,
      `/spaces/${encodeURIComponent(spaceId)}`,
      accessToken,
    ),

  listRecoveryKeyPackages: (baseUrl: string, accessToken: string) =>
    getMsgpack<{ packages: RecoverySpaceKeyPackage[] }>(
      baseUrl,
      "/spaces/recovery-key-packages",
      accessToken,
    ),

  restoreSpace: (baseUrl: string, spaceId: string, accessToken: string) =>
    postMsgpack<Record<string, never>, { changed: boolean }>(
      baseUrl,
      `/spaces/${encodeURIComponent(spaceId)}/restore`,
      {},
      accessToken,
    ),

  listSpaceDevices: (baseUrl: string, spaceId: string, accessToken: string) =>
    getMsgpack<{ devices: SpaceDeviceSummary[] }>(
      baseUrl,
      `/spaces/${encodeURIComponent(spaceId)}/devices`,
      accessToken,
    ),

  listSpaceMembers: (baseUrl: string, spaceId: string, accessToken: string) =>
    getMsgpack<{ members: SpaceMemberSummary[] }>(
      baseUrl,
      `/spaces/${encodeURIComponent(spaceId)}/members`,
      accessToken,
    ),

  revokeSpaceMember: (
    baseUrl: string,
    spaceId: string,
    userId: string,
    payload: RevokeSpaceMemberRequest,
    accessToken: string,
  ) =>
    postMsgpack<
      RevokeSpaceMemberRequest,
      { revoked: boolean; key_epoch: number }
    >(
      baseUrl,
      `/spaces/${encodeURIComponent(spaceId)}/members/${encodeURIComponent(userId)}/revoke`,
      payload,
      accessToken,
    ),

  rotateSpaceKey: (
    baseUrl: string,
    spaceId: string,
    payload: RotateSpaceKeyRequest,
    accessToken: string,
  ) =>
    postMsgpack<
      RotateSpaceKeyRequest,
      { rotated: boolean; key_epoch: number }
    >(
      baseUrl,
      `/spaces/${encodeURIComponent(spaceId)}/rotate-key`,
      payload,
      accessToken,
    ),

  listWorkspaces: (baseUrl: string, accessToken: string) =>
    postMsgpack<Record<string, never>, { workspaces: WorkspaceSummary[] }>(
      baseUrl,
      "/workspaces/list",
      {},
      accessToken,
    ),

  listWorkspaceMembers: (
    baseUrl: string,
    workspaceId: string,
    accessToken: string,
  ) =>
    postMsgpack<{ workspace_id: string }, { members: WorkspaceMember[] }>(
      baseUrl,
      "/workspaces/members",
      { workspace_id: workspaceId },
      accessToken,
    ),

  createOwnershipTransfer: (
    baseUrl: string,
    resourceKind: OwnershipResourceKind,
    resourceId: string,
    targetUserId: string,
    accessToken: string,
  ) =>
    postMsgpack<
      {
        resource_kind: OwnershipResourceKind;
        resource_id: string;
        target_user_id: string;
      },
      { offer: OwnershipTransferOffer }
    >(
      baseUrl,
      "/ownership-transfers",
      {
        resource_kind: resourceKind,
        resource_id: resourceId,
        target_user_id: targetUserId,
      },
      accessToken,
    ),

  listIncomingOwnershipTransfers: (baseUrl: string, accessToken: string) =>
    getMsgpack<{ offers: OwnershipTransferOffer[] }>(
      baseUrl,
      "/ownership-transfers/incoming",
      accessToken,
    ),

  listOutgoingOwnershipTransfers: (baseUrl: string, accessToken: string) =>
    getMsgpack<{ offers: OwnershipTransferOffer[] }>(
      baseUrl,
      "/ownership-transfers/outgoing",
      accessToken,
    ),

  acceptOwnershipTransfer: (
    baseUrl: string,
    transferId: string,
    accessToken: string,
  ) =>
    postMsgpack<Record<string, never>, { changed: boolean }>(
      baseUrl,
      `/ownership-transfers/${encodeURIComponent(transferId)}/accept`,
      {},
      accessToken,
    ),

  cancelOwnershipTransfer: (
    baseUrl: string,
    transferId: string,
    accessToken: string,
  ) =>
    deleteMsgpack<{ changed: boolean }>(
      baseUrl,
      `/ownership-transfers/${encodeURIComponent(transferId)}`,
      accessToken,
    ),

  reauthStart: (
    baseUrl: string,
    opaqueStartRequest: Uint8Array,
    action: ReauthAction,
    accessToken: string,
  ) =>
    postMsgpack<
      { opaque_start_request: Uint8Array; action: ReauthAction },
      ReauthStartResponse
    >(
      baseUrl,
      "/auth/reauth/start",
      { opaque_start_request: opaqueStartRequest, action },
      accessToken,
    ),

  reauthFinish: (
    baseUrl: string,
    opaqueFlowId: string,
    opaqueFinishRequest: Uint8Array,
    totpCode: string | null,
    action: ReauthAction,
    accessToken: string,
  ) =>
    postMsgpack<
      { opaque_flow_id: string; opaque_finish_request: Uint8Array; totp_code: string | null; action: ReauthAction },
      { reauth_token: string }
    >(
      baseUrl,
      "/auth/reauth/finish",
      {
        opaque_flow_id: opaqueFlowId,
        opaque_finish_request: opaqueFinishRequest,
        totp_code: totpCode,
        action,
      },
      accessToken,
    ),

  getDeletionStatus: (baseUrl: string, accessToken: string) =>
    getMsgpack<DeletionStatusResponse>(
      baseUrl,
      "/users/me/deletion-status",
      accessToken,
    ),

  deleteAccount: (
    baseUrl: string,
    reauthToken: string,
    confirmation: string,
    accessToken: string,
  ) =>
    postMsgpack<
      { reauth_token: string; confirmation: string },
      { deleted: boolean }
    >(
      baseUrl,
      "/users/me/delete",
      { reauth_token: reauthToken, confirmation },
      accessToken,
    ),

  putDeviceKeyPackage: (
    baseUrl: string,
    spaceId: string,
    packagePayload: DeviceKeyPackage,
    accessToken: string,
  ) =>
    postMsgpack<{ package: DeviceKeyPackage }, { stored: boolean }>(
      baseUrl,
      `/spaces/${encodeURIComponent(spaceId)}/device-key-packages`,
      { package: packagePayload },
      accessToken,
    ),

  putRecoveryKeyPackage: (
    baseUrl: string,
    spaceId: string,
    keyEpoch: number,
    encryptedKeyPackage: Uint8Array,
    accessToken: string,
  ) =>
    postMsgpack<
      { key_epoch: number; encrypted_key_package: Uint8Array },
      { stored: boolean }
    >(
      baseUrl,
      `/spaces/${encodeURIComponent(spaceId)}/recovery-key-package`,
      {
        key_epoch: keyEpoch,
        encrypted_key_package: encryptedKeyPackage,
      },
      accessToken,
    ),

  appendOperation: (
    baseUrl: string,
    envelope: OperationEnvelopeV1,
    accessToken: string,
  ) =>
    postMsgpack<
      { envelope: OperationEnvelopeV1 },
      { accepted: boolean; duplicate: boolean; space_seq: number }
    >(baseUrl, "/operations", { envelope }, accessToken),

  listOperations: (
    baseUrl: string,
    spaceId: string,
    since: number,
    accessToken: string,
  ) =>
    getMsgpack<{ operations: StoredOperation[]; next_cursor: number }>(
      baseUrl,
      `/operations?space_id=${encodeURIComponent(spaceId)}&since=${encodeURIComponent(String(since))}`,
      accessToken,
    ),

  createInviteCode: (
    baseUrl: string,
    payload: CreateInviteCodeRequest,
    accessToken: string,
  ) =>
    postMsgpack<CreateInviteCodeRequest, CreateInviteCodeResponse>(
      baseUrl,
      "/invite-codes",
      payload,
      accessToken,
    ),

  redeemInviteCode: (
    baseUrl: string,
    inviteCodeHash: Uint8Array,
    accessToken: string,
  ) =>
    postMsgpack<{ invite_code_hash: Uint8Array }, RedeemInviteCodeResponse>(
      baseUrl,
      "/invite-codes/redeem",
      { invite_code_hash: inviteCodeHash },
      accessToken,
    ),

  getConsentSettings: (baseUrl: string, accessToken: string) =>
    getMsgpack<ConsentSettings>(
      baseUrl,
      "/users/me/consents",
      accessToken,
    ),

  updateConsentSettings: (
    baseUrl: string,
    payload: UpdateConsentSettingsRequest,
    accessToken: string,
  ) =>
    postMsgpack<UpdateConsentSettingsRequest, ConsentSettings>(
      baseUrl,
      "/users/me/consents",
      payload,
      accessToken,
    ),
};
