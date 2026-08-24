//! Operator authentication, audited mutations, and effective runtime policy.

use std::collections::HashMap;

use axum::http::HeaderMap;
use serde_json::Value;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;
use webauthn_rs::prelude::{PublicKeyCredential, RegisterPublicKeyCredential, WebauthnError};

use crate::{
    features::{
        admin::{
            dto::{
                AdminAuditResponse, AdminAuthFinishRequest, AdminAuthFinishResponse,
                AdminAuthStartRequest, AdminAuthStartResponse, AdminDashboardResponse,
                AdminMutationResponse, AdminSecurityKeyAddFinishRequest,
                AdminSecurityKeyRegistrationFinishRequest, AdminSecurityKeyRegistrationResponse,
                AdminSecurityKeyRegistrationStartRequest, AdminSecurityKeyRemoveRequest,
                RuntimeSetting, RuntimeSettingsResponse, SuspendAccountRequest,
                UpdateRuntimeSettingRequest,
            },
            repositories::{self, AdminIdentity},
        },
        auth::transport::client_metadata_from_headers,
        common::{ApiError, bad_request, conflict, internal_error, unauthenticated, unauthorized},
    },
    platform::{
        secret_box::decrypt_admin_totp,
        security::auth::{TotpConfig, parse_bearer, verify_totp},
        state::AppState,
    },
};

const ADMIN_SESSION_MINUTES: i64 = 15;
const ADMIN_REAUTH_MINUTES: i64 = 5;
const ADMIN_KEY_ENROLLMENT_PREFIX: &str = "admin:key-enrollment:";

fn security_key_registration_failure(flow_id: Uuid, error: anyhow::Error) -> ApiError {
    let attestation_rejected = error.downcast_ref::<WebauthnError>().is_some_and(|cause| {
        matches!(
            cause,
            WebauthnError::MissingAttestationCredentialData
                | WebauthnError::AttestationNotSupported
                | WebauthnError::AttestationTrustFailure
                | WebauthnError::AttestationNotVerifiable
                | WebauthnError::AttestationUntrustedAaguid
                | WebauthnError::AttestationFormatMissingAaguid
                | WebauthnError::AttestationChainNotTrusted(_)
                | WebauthnError::TrustFailure
                | WebauthnError::CredentialMayNotBeHardwareBound
        )
    });
    tracing::warn!(
        %flow_id,
        error = ?error,
        attestation_rejected,
        "operator security-key registration rejected"
    );
    if attestation_rejected {
        unauthenticated(
            "security-key attestation was not trusted; use a supported physical YubiKey and allow the browser to share device attestation",
        )
    } else {
        unauthenticated("security-key registration failed; start a new enrollment attempt")
    }
}

fn verify_admin_totp(
    state: &AppState,
    identity: &AdminIdentity,
    code: &str,
) -> Result<(), ApiError> {
    let secret = decrypt_admin_totp(
        &state.config.admin_totp_kek,
        &identity.username,
        &identity.totp_secret_ciphertext,
    )
    .map_err(internal_error)?;
    let valid = verify_totp(
        &secret,
        code.trim(),
        OffsetDateTime::now_utc(),
        TotpConfig::default(),
    )
    .map_err(|_| unauthenticated("invalid operator second factor"))?;
    if !valid {
        return Err(unauthenticated("invalid operator second factor"));
    }
    Ok(())
}

fn token_from_headers(headers: &HeaderMap) -> Result<&str, ApiError> {
    let raw = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_bearer)
        .ok_or_else(|| unauthenticated("missing operator token"))?;
    if !raw.starts_with("kamori_admin_") {
        return Err(unauthenticated("invalid operator token"));
    }
    Ok(raw)
}

pub(crate) async fn authorize_admin(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AdminIdentity, ApiError> {
    repositories::authorize_token(&state.pool, token_from_headers(headers)?, "session")
        .await
        .map_err(internal_error)?
        .ok_or_else(|| unauthenticated("operator session expired"))
}

fn validate_operator_username(username: &str) -> Result<&str, ApiError> {
    let username = username.trim();
    if username.len() < 3
        || username.len() > 64
        || !username
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(bad_request("invalid operator username"));
    }
    Ok(username)
}

pub(crate) async fn bootstrap_start(
    state: &AppState,
    payload: AdminSecurityKeyRegistrationStartRequest,
) -> Result<AdminSecurityKeyRegistrationResponse, ApiError> {
    let username = validate_operator_username(&payload.username)?;
    let identity =
        repositories::validate_bootstrap(&state.pool, username, &payload.bootstrap_token)
            .await
            .map_err(internal_error)?
            .ok_or_else(|| unauthenticated("invalid or expired operator bootstrap"))?;
    verify_admin_totp(state, &identity, &payload.totp_code)?;
    let flow_id = Uuid::new_v4();
    let challenge = state
        .admin_passkeys
        .start_security_key_registration(flow_id, identity.id, &identity.username)
        .await
        .map_err(internal_error)?;
    Ok(AdminSecurityKeyRegistrationResponse {
        flow_id,
        public_key_credential_creation_options: challenge.public_key_credential_creation_options,
    })
}

pub(crate) async fn bootstrap_finish(
    state: &AppState,
    payload: AdminSecurityKeyRegistrationFinishRequest,
) -> Result<AdminMutationResponse, ApiError> {
    let username = validate_operator_username(&payload.username)?;
    let identity =
        repositories::validate_bootstrap(&state.pool, username, &payload.bootstrap_token)
            .await
            .map_err(internal_error)?
            .ok_or_else(|| unauthenticated("invalid or expired operator bootstrap"))?;
    verify_admin_totp(state, &identity, &payload.totp_code)?;
    let credential: RegisterPublicKeyCredential = serde_json::from_slice(&payload.credential)
        .map_err(|_| bad_request("invalid credential"))?;
    let security_key = state
        .admin_passkeys
        .finish_security_key_registration(payload.flow_id, credential)
        .await
        .map_err(|error| security_key_registration_failure(payload.flow_id, error))?;
    let changed = repositories::activate_with_security_key(
        &state.pool,
        identity.id,
        &payload.bootstrap_token,
        &security_key,
    )
    .await
    .map_err(internal_error)?;
    if !changed {
        return Err(conflict("operator bootstrap was already consumed"));
    }
    Ok(AdminMutationResponse { changed })
}

async fn auth_start_for_identity(
    state: &AppState,
    identity: &AdminIdentity,
) -> Result<AdminAuthStartResponse, ApiError> {
    if identity.security_keys.is_empty() {
        return Err(unauthenticated("operator security key is not enrolled"));
    }
    let flow_id = Uuid::new_v4();
    let challenge = state
        .admin_passkeys
        .start_security_key_authentication(flow_id, &identity.security_keys)
        .await
        .map_err(internal_error)?;
    Ok(AdminAuthStartResponse {
        flow_id,
        public_key_credential_request_options: challenge.public_key_credential_request_options,
    })
}

pub(crate) async fn login_start(
    state: &AppState,
    payload: AdminAuthStartRequest,
) -> Result<AdminAuthStartResponse, ApiError> {
    let username = validate_operator_username(&payload.username)?;
    let identity = repositories::load_active_identity(&state.pool, username)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| unauthenticated("operator authentication failed"))?;
    auth_start_for_identity(state, &identity).await
}

async fn finish_security_key_and_totp(
    state: &AppState,
    payload: &AdminAuthFinishRequest,
) -> Result<AdminIdentity, ApiError> {
    let username = validate_operator_username(&payload.username)?;
    let identity = repositories::load_active_identity(&state.pool, username)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| unauthenticated("operator authentication failed"))?;
    verify_admin_totp(state, &identity, &payload.totp_code)?;
    let credential: PublicKeyCredential = serde_json::from_slice(&payload.credential)
        .map_err(|_| bad_request("invalid credential"))?;
    let result = state
        .admin_passkeys
        .finish_security_key_authentication(payload.flow_id, credential)
        .await
        .map_err(|_| unauthenticated("operator security-key verification failed"))?;
    repositories::persist_security_key_result(&state.pool, identity.id, &result)
        .await
        .map_err(internal_error)?;
    Ok(identity)
}

fn token_response(token: repositories::IssuedAdminToken) -> AdminAuthFinishResponse {
    AdminAuthFinishResponse {
        token: token.token,
        expires_at_unix_ms: token.expires_at.unix_timestamp_nanos() as i64 / 1_000_000,
    }
}

pub(crate) async fn login_finish(
    state: &AppState,
    headers: &HeaderMap,
    payload: AdminAuthFinishRequest,
) -> Result<AdminAuthFinishResponse, ApiError> {
    let identity = finish_security_key_and_totp(state, &payload).await?;
    let client = client_metadata_from_headers(headers);
    let token = repositories::issue_token(
        &state.pool,
        identity.id,
        "session",
        Duration::minutes(ADMIN_SESSION_MINUTES),
        client.user_agent.as_deref(),
        client.ip_address.as_deref(),
    )
    .await
    .map_err(internal_error)?;
    Ok(token_response(token))
}

pub(crate) async fn reauth_start(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AdminAuthStartResponse, ApiError> {
    let session = authorize_admin(state, headers).await?;
    let identity = repositories::load_active_identity(&state.pool, &session.username)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| unauthenticated("operator authentication failed"))?;
    auth_start_for_identity(state, &identity).await
}

pub(crate) async fn reauth_finish(
    state: &AppState,
    headers: &HeaderMap,
    payload: AdminAuthFinishRequest,
) -> Result<AdminAuthFinishResponse, ApiError> {
    let session = authorize_admin(state, headers).await?;
    if session.username != payload.username.trim() {
        return Err(unauthenticated("operator identity mismatch"));
    }
    let identity = finish_security_key_and_totp(state, &payload).await?;
    let client = client_metadata_from_headers(headers);
    let token = repositories::issue_token(
        &state.pool,
        identity.id,
        "reauth",
        Duration::minutes(ADMIN_REAUTH_MINUTES),
        client.user_agent.as_deref(),
        client.ip_address.as_deref(),
    )
    .await
    .map_err(internal_error)?;
    Ok(token_response(token))
}

pub(crate) async fn logout(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AdminMutationResponse, ApiError> {
    let token = token_from_headers(headers)?;
    let changed = repositories::revoke_session_token(&state.pool, token)
        .await
        .map_err(internal_error)?;
    Ok(AdminMutationResponse { changed })
}

pub(crate) async fn add_security_key_start(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AdminSecurityKeyRegistrationResponse, ApiError> {
    let actor = authorize_admin(state, headers).await?;
    let flow_id = Uuid::new_v4();
    let challenge = state
        .admin_passkeys
        .start_security_key_registration(flow_id, actor.id, &actor.username)
        .await
        .map_err(internal_error)?;
    state
        .state_store
        .put(
            &format!("{ADMIN_KEY_ENROLLMENT_PREFIX}{flow_id}"),
            actor.id.as_bytes(),
            std::time::Duration::from_secs(state.config.valkey_ttl_seconds.min(300)),
        )
        .await
        .map_err(internal_error)?;
    Ok(AdminSecurityKeyRegistrationResponse {
        flow_id,
        public_key_credential_creation_options: challenge.public_key_credential_creation_options,
    })
}

pub(crate) async fn add_security_key_finish(
    state: &AppState,
    headers: &HeaderMap,
    payload: AdminSecurityKeyAddFinishRequest,
) -> Result<AdminMutationResponse, ApiError> {
    let actor = authorize_admin(state, headers).await?;
    let name = payload.name.trim();
    if !(3..=64).contains(&name.len()) {
        return Err(bad_request(
            "security-key name must contain 3 to 64 characters",
        ));
    }
    let reason = validate_reason(&payload.reason)?;
    if payload.confirmation != "ADD SECURITY KEY" {
        return Err(bad_request(
            "typed security-key confirmation does not match",
        ));
    }
    let binding_key = format!("{ADMIN_KEY_ENROLLMENT_PREFIX}{}", payload.flow_id);
    consume_reauth(state, actor.id, &payload.reauth_token).await?;
    let binding = state
        .state_store
        .take(&binding_key)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| unauthenticated("security-key enrollment expired"))?;
    if binding.as_slice() != actor.id.as_bytes() {
        return Err(unauthenticated("security-key enrollment identity mismatch"));
    }
    let credential: RegisterPublicKeyCredential = serde_json::from_slice(&payload.credential)
        .map_err(|_| bad_request("invalid credential"))?;
    let security_key = state
        .admin_passkeys
        .finish_security_key_registration(payload.flow_id, credential)
        .await
        .map_err(|error| security_key_registration_failure(payload.flow_id, error))?;
    let client = client_metadata_from_headers(headers);
    repositories::add_security_key(
        &state.pool,
        actor.id,
        name,
        &security_key,
        reason,
        client.ip_address.as_deref(),
    )
    .await
    .map_err(internal_error)?;
    Ok(AdminMutationResponse { changed: true })
}

pub(crate) async fn remove_security_key(
    state: &AppState,
    headers: &HeaderMap,
    payload: AdminSecurityKeyRemoveRequest,
) -> Result<AdminMutationResponse, ApiError> {
    let actor = authorize_admin(state, headers).await?;
    let reason = validate_reason(&payload.reason)?;
    if payload.confirmation != format!("REMOVE SECURITY KEY {}", payload.key_id) {
        return Err(bad_request(
            "typed security-key confirmation does not match",
        ));
    }
    consume_reauth(state, actor.id, &payload.reauth_token).await?;
    let client = client_metadata_from_headers(headers);
    match repositories::remove_security_key(
        &state.pool,
        actor.id,
        payload.key_id,
        reason,
        client.ip_address.as_deref(),
        state.config.registration_enabled,
    )
    .await
    .map_err(internal_error)?
    {
        repositories::RemoveSecurityKeyResult::Removed => {
            Ok(AdminMutationResponse { changed: true })
        }
        repositories::RemoveSecurityKeyResult::NotFound => {
            Err(bad_request("operator security key not found"))
        }
        repositories::RemoveSecurityKeyResult::WouldViolateMinimum { required } => {
            Err(bad_request(if required == 2 {
                "close registration or enroll a replacement before removing this key"
            } else {
                "the last operator security key cannot be removed"
            }))
        }
    }
}

pub(crate) async fn dashboard(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AdminDashboardResponse, ApiError> {
    let actor = authorize_admin(state, headers).await?;
    let counts = repositories::dashboard_counts(&state.pool, actor.id)
        .await
        .map_err(internal_error)?;
    Ok(AdminDashboardResponse {
        active_accounts: u64::try_from(counts.active_accounts).map_err(internal_error)?,
        suspended_accounts: u64::try_from(counts.suspended_accounts).map_err(internal_error)?,
        total_blob_storage_bytes: u64::try_from(counts.total_blob_storage_bytes)
            .map_err(internal_error)?,
        pending_blobs: u64::try_from(counts.pending_blobs).map_err(internal_error)?,
        pending_object_deletions: u64::try_from(counts.pending_object_deletions)
            .map_err(internal_error)?,
        registration_enabled: effective_bool(
            state,
            "registration_enabled",
            state.config.registration_enabled,
        )
        .await?,
        beta_account_limit: effective_u64(
            state,
            "beta_account_limit",
            state.config.beta_account_limit,
        )
        .await?,
        latest_migration: counts.latest_migration,
        jobs: counts.jobs,
        security_keys: counts.security_keys,
    })
}

fn setting_defaults(state: &AppState) -> [(&'static str, Value); 11] {
    [
        (
            "registration_enabled",
            Value::Bool(state.config.registration_enabled),
        ),
        (
            "beta_account_limit",
            Value::from(state.config.beta_account_limit),
        ),
        ("max_blob_bytes", Value::from(state.config.max_blob_bytes)),
        (
            "account_storage_bytes",
            Value::from(state.config.account_storage_bytes),
        ),
        (
            "owner_monthly_egress_bytes",
            Value::from(state.config.owner_monthly_egress_bytes),
        ),
        (
            "owner_rolling_24h_egress_bytes",
            Value::from(state.config.owner_rolling_24h_egress_bytes),
        ),
        (
            "owner_concurrent_blob_downloads",
            Value::from(state.config.owner_concurrent_blob_downloads),
        ),
        (
            "global_concurrent_blob_downloads",
            Value::from(state.config.global_concurrent_blob_downloads),
        ),
        (
            "blob_download_bytes_per_second",
            Value::from(state.config.blob_download_bytes_per_second),
        ),
        (
            "global_nonessential_egress_stop_bytes",
            Value::from(state.config.global_nonessential_egress_stop_bytes),
        ),
        (
            "global_emergency_egress_breaker_bytes",
            Value::from(state.config.global_emergency_egress_breaker_bytes),
        ),
    ]
}

pub(crate) async fn settings(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<RuntimeSettingsResponse, ApiError> {
    authorize_admin(state, headers).await?;
    let stored = repositories::list_runtime_settings(&state.pool)
        .await
        .map_err(internal_error)?
        .into_iter()
        .map(|setting| (setting.key.clone(), setting))
        .collect::<HashMap<_, _>>();
    let settings = setting_defaults(state)
        .into_iter()
        .map(|(key, default)| {
            if let Some(value) = stored.get(key) {
                Ok(RuntimeSetting {
                    key: key.to_string(),
                    value: value.value.clone(),
                    version: u64::try_from(value.version)?,
                    updated_at_unix_ms: Some(
                        value.updated_at.unix_timestamp_nanos() as i64 / 1_000_000,
                    ),
                    overridden: true,
                })
            } else {
                Ok(RuntimeSetting {
                    key: key.to_string(),
                    value: default,
                    version: 0,
                    updated_at_unix_ms: None,
                    overridden: false,
                })
            }
        })
        .collect::<anyhow::Result<Vec<_>>>()
        .map_err(internal_error)?;
    Ok(RuntimeSettingsResponse { settings })
}

fn validate_reason(reason: &str) -> Result<&str, ApiError> {
    let reason = reason.trim();
    if !(10..=500).contains(&reason.len()) {
        return Err(bad_request("reason must contain 10 to 500 characters"));
    }
    Ok(reason)
}

fn validate_setting(key: &str, value: &Value) -> Result<(), ApiError> {
    match key {
        "registration_enabled" if value.is_boolean() => Ok(()),
        "beta_account_limit" => match value.as_u64() {
            Some(1..=100_000) => Ok(()),
            _ => Err(bad_request(
                "beta_account_limit must be between 1 and 100000",
            )),
        },
        "max_blob_bytes" => match value.as_u64() {
            Some(value)
                if (1024 * 1024..=25 * 1024 * 1024).contains(&value)
                    && value.is_multiple_of(1024 * 1024) =>
            {
                Ok(())
            }
            _ => Err(bad_request(
                "max_blob_bytes must be 1 MiB aligned and at most 25 MiB",
            )),
        },
        "account_storage_bytes"
        | "owner_monthly_egress_bytes"
        | "owner_rolling_24h_egress_bytes"
        | "global_nonessential_egress_stop_bytes"
        | "global_emergency_egress_breaker_bytes" => match value.as_u64() {
            Some(value) if (1024 * 1024..=i64::MAX as u64).contains(&value) => Ok(()),
            _ => Err(bad_request(
                "quota setting must be at least 1 MiB and fit PostgreSQL BIGINT",
            )),
        },
        "owner_concurrent_blob_downloads" => match value.as_u64() {
            Some(1..=100) => Ok(()),
            _ => Err(bad_request(
                "owner_concurrent_blob_downloads must be between 1 and 100",
            )),
        },
        "global_concurrent_blob_downloads" => match value.as_u64() {
            Some(1..=4096) => Ok(()),
            _ => Err(bad_request(
                "global_concurrent_blob_downloads must be between 1 and 4096",
            )),
        },
        "blob_download_bytes_per_second" => match value.as_u64() {
            Some(102_400..=104_857_600) => Ok(()),
            _ => Err(bad_request(
                "blob_download_bytes_per_second must be between 100 KiB/s and 100 MiB/s",
            )),
        },
        _ => Err(bad_request("unknown or invalid runtime setting")),
    }
}

async fn validate_setting_relationships(
    state: &AppState,
    candidate_key: &str,
    candidate_value: &Value,
) -> Result<(), ApiError> {
    let mut values = setting_defaults(state)
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect::<HashMap<_, _>>();
    for setting in repositories::list_runtime_settings(&state.pool)
        .await
        .map_err(internal_error)?
    {
        values.insert(setting.key, setting.value);
    }
    values.insert(candidate_key.to_string(), candidate_value.clone());
    let number = |key: &str| {
        values
            .get(key)
            .and_then(Value::as_u64)
            .ok_or_else(|| internal_error(format!("invalid effective value for {key}")))
    };
    if number("max_blob_bytes")? > number("account_storage_bytes")? {
        return Err(bad_request(
            "max_blob_bytes cannot exceed account_storage_bytes",
        ));
    }
    if number("owner_rolling_24h_egress_bytes")? > number("owner_monthly_egress_bytes")? {
        return Err(bad_request(
            "rolling 24-hour egress cannot exceed monthly owner egress",
        ));
    }
    if number("owner_concurrent_blob_downloads")? > number("global_concurrent_blob_downloads")? {
        return Err(bad_request(
            "owner blob concurrency cannot exceed global blob concurrency",
        ));
    }
    if number("global_nonessential_egress_stop_bytes")?
        >= number("global_emergency_egress_breaker_bytes")?
    {
        return Err(bad_request(
            "global nonessential stop must remain below the emergency breaker",
        ));
    }
    Ok(())
}

async fn consume_reauth(state: &AppState, actor_id: Uuid, token: &str) -> Result<(), ApiError> {
    if !repositories::consume_reauth_token(&state.pool, token, actor_id)
        .await
        .map_err(internal_error)?
    {
        return Err(unauthenticated(
            "fresh operator reauthentication is required",
        ));
    }
    Ok(())
}

pub(crate) async fn update_setting(
    state: &AppState,
    headers: &HeaderMap,
    payload: UpdateRuntimeSettingRequest,
) -> Result<AdminMutationResponse, ApiError> {
    let actor = authorize_admin(state, headers).await?;
    validate_setting(&payload.key, &payload.value)?;
    validate_setting_relationships(state, &payload.key, &payload.value).await?;
    let reason = validate_reason(&payload.reason)?;
    if payload.confirmation != format!("SET {}", payload.key) {
        return Err(bad_request("typed setting confirmation does not match"));
    }
    let expected_version = i64::try_from(payload.expected_version)
        .map_err(|_| bad_request("expected_version exceeds the supported range"))?;
    consume_reauth(state, actor.id, &payload.reauth_token).await?;
    let client = client_metadata_from_headers(headers);
    let result = repositories::set_runtime_value(
        &state.pool,
        actor.id,
        &payload.key,
        &payload.value,
        expected_version,
        reason,
        client.ip_address.as_deref(),
    )
    .await
    .map_err(internal_error)?;
    match result {
        repositories::SetRuntimeValueResult::Changed => Ok(AdminMutationResponse { changed: true }),
        repositories::SetRuntimeValueResult::VersionConflict => Err(conflict(
            "runtime setting version changed; reload and retry",
        )),
        repositories::SetRuntimeValueResult::SecurityKeyMinimum => Err(bad_request(
            "enroll a second roaming security key before opening registration",
        )),
    }
}

pub(crate) async fn suspend(
    state: &AppState,
    headers: &HeaderMap,
    payload: SuspendAccountRequest,
) -> Result<AdminMutationResponse, ApiError> {
    let actor = authorize_admin(state, headers).await?;
    let reason = validate_reason(&payload.reason)?;
    let verb = if payload.suspended {
        "SUSPEND"
    } else {
        "UNSUSPEND"
    };
    if payload.confirmation != format!("{verb} {}", payload.user_id) {
        return Err(bad_request("typed account confirmation does not match"));
    }
    consume_reauth(state, actor.id, &payload.reauth_token).await?;
    let client = client_metadata_from_headers(headers);
    let changed = repositories::suspend_account(
        &state.pool,
        actor.id,
        payload.user_id,
        payload.suspended,
        reason,
        client.ip_address.as_deref(),
    )
    .await
    .map_err(internal_error)?;
    if !changed {
        return Err(unauthorized("account not found"));
    }
    Ok(AdminMutationResponse { changed })
}

pub(crate) async fn audit(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AdminAuditResponse, ApiError> {
    authorize_admin(state, headers).await?;
    Ok(AdminAuditResponse {
        entries: repositories::list_audit(&state.pool, 200)
            .await
            .map_err(internal_error)?,
    })
}

pub(crate) async fn effective_bool(
    state: &AppState,
    key: &str,
    default: bool,
) -> Result<bool, ApiError> {
    match repositories::get_runtime_value(&state.pool, key)
        .await
        .map_err(internal_error)?
    {
        Some(value) => value
            .as_bool()
            .ok_or_else(|| internal_error(format!("invalid stored value for {key}"))),
        None => Ok(default),
    }
}

pub(crate) async fn effective_u64(
    state: &AppState,
    key: &str,
    default: u64,
) -> Result<u64, ApiError> {
    match repositories::get_runtime_value(&state.pool, key)
        .await
        .map_err(internal_error)?
    {
        Some(value) => value
            .as_u64()
            .ok_or_else(|| internal_error(format!("invalid stored value for {key}"))),
        None => Ok(default),
    }
}

pub(crate) async fn effective_u64_values(
    state: &AppState,
    defaults: &[(&str, u64)],
) -> Result<HashMap<String, u64>, ApiError> {
    let keys = defaults
        .iter()
        .map(|(key, _)| (*key).to_string())
        .collect::<Vec<_>>();
    let stored = repositories::get_runtime_values(&state.pool, &keys)
        .await
        .map_err(internal_error)?;
    defaults
        .iter()
        .map(|(key, default)| {
            let value = match stored.get(*key) {
                Some(value) => value
                    .as_u64()
                    .ok_or_else(|| internal_error(format!("invalid stored value for {key}")))?,
                None => *default,
            };
            Ok(((*key).to_string(), value))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_setting_validation_is_strict() {
        assert!(validate_setting("registration_enabled", &Value::Bool(false)).is_ok());
        assert!(validate_setting("registration_enabled", &Value::from(1)).is_err());
        assert!(validate_setting("max_blob_bytes", &Value::from(1024 * 1024)).is_ok());
        assert!(validate_setting("max_blob_bytes", &Value::from(1024)).is_err());
        assert!(validate_setting("unknown", &Value::Bool(false)).is_err());
    }

    #[test]
    fn attestation_failure_returns_actionable_safe_message() {
        let error = security_key_registration_failure(
            Uuid::nil(),
            anyhow::Error::new(WebauthnError::AttestationTrustFailure),
        );
        assert_eq!(error.0, axum::http::StatusCode::UNAUTHORIZED);
        assert!(
            error
                .1
                .0
                .message
                .starts_with("security-key attestation was not trusted")
        );
    }
}
