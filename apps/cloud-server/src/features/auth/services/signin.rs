//! Service logic for password sign-in and pre-auth/TOTP branching.

use axum::{
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    features::auth::dto::{
        SigninFinishRequest, SigninFinishResponse, SigninNextStep, SigninStartRequest,
        SigninStartResponse, SigninTotpRequest,
    },
    features::auth::{
        repositories::{
            UserRow, consume_totp_backup_code, create_refresh_token, find_user_by_username,
        },
        transport::{
            RefreshTransport, client_metadata_from_headers, generate_csrf_token,
            refresh_transport_from_headers, set_csrf_cookie, set_refresh_cookie,
        },
    },
    features::common::{ApiError, MsgPack, bad_request, internal_error, unauthenticated},
    platform::state::AppState,
    platform::{
        secret_box::decrypt_user_totp,
        security::auth::{TotpConfig, verify_totp},
    },
};

use super::support::{hash_account_recovery_code, normalize_recovery_code, normalize_username};

const TOTP_CONTINUATION_TTL: Duration = Duration::from_secs(5 * 60);
const TOTP_CONTINUATION_MAX_ATTEMPTS: u8 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TotpContinuation {
    user_id: Uuid,
    username: String,
    attempts: u8,
}

pub(crate) async fn signin_start(
    state: &AppState,
    payload: SigninStartRequest,
) -> Result<SigninStartResponse, ApiError> {
    if payload.opaque_start_request.is_empty() || payload.opaque_start_request.len() > 8 * 1024 {
        return Err(bad_request("opaque_start_request has invalid size"));
    }
    let username = normalize_username(&payload.username)
        .map_err(|_| unauthenticated("invalid credentials"))?;
    crate::platform::rate_limit::enforce_credential_attempt(
        state,
        "opaque-signin",
        username.as_bytes(),
    )
    .await?;
    let user = find_user_by_username(&state.pool, &username).await?;
    let password_file_bytes = user
        .as_ref()
        .and_then(|candidate| candidate.opaque_record.as_deref());

    let opaque = state
        .opaque
        .login_start(
            &username,
            &payload.opaque_start_request,
            password_file_bytes,
        )
        .await
        .map_err(|_| unauthenticated("invalid credentials"))?;

    Ok(SigninStartResponse {
        opaque_flow_id: opaque.flow_id,
        opaque_server_message: opaque.message,
        // Account existence and TOTP configuration remain hidden until the
        // OPAQUE proof succeeds.
        next_step: SigninNextStep::Continue,
    })
}

pub(crate) async fn signin_finish(
    state: &AppState,
    headers: &HeaderMap,
    payload: SigninFinishRequest,
) -> Result<Response, ApiError> {
    if payload.opaque_finish_request.is_empty() || payload.opaque_finish_request.len() > 8 * 1024 {
        return Err(bad_request("opaque_finish_request has invalid size"));
    }
    let refresh_transport = refresh_transport_from_headers(headers)?;
    let username = normalize_username(&payload.username)
        .map_err(|_| unauthenticated("invalid credentials"))?;
    let user = find_user_by_username(&state.pool, &username).await?;

    state
        .opaque
        .login_finish(
            &username,
            payload.opaque_flow_id,
            &payload.opaque_finish_request,
        )
        .await
        .map_err(|_| unauthenticated("invalid credentials"))?;
    let user = user.ok_or_else(|| unauthenticated("invalid credentials"))?;

    // `enable_totp` controls new enrollment only. Once an account has a TOTP
    // secret, configuration changes must never turn its second factor into an
    // authentication bypass.
    if user.totp_secret_ciphertext.is_some() {
        if let Some(code) = payload
            .totp_code
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            verify_user_totp(state, &user, code).await?;
        } else {
            let continuation_token = create_totp_continuation(state, &user).await?;
            return Ok(MsgPack(SigninFinishResponse {
                access_token: None,
                refresh_token: None,
                refresh_token_id: None,
                totp_verified: false,
                // Do not release even password-wrapped account key material
                // until the second factor succeeds. Clients retain only the
                // OPAQUE export key while completing this continuation.
                encrypted_master_key: Vec::new(),
                public_key_bundle: Vec::new(),
                totp_continuation_token: Some(continuation_token),
                device_enrollment_token: None,
                csrf_token: None,
            })
            .into_response());
        }
    }
    complete_login(state, headers, refresh_transport, user).await
}

pub(crate) async fn signin_totp(
    state: &AppState,
    headers: &HeaderMap,
    payload: SigninTotpRequest,
) -> Result<Response, ApiError> {
    let token = payload.continuation_token.trim();
    let code = payload.totp_code.trim();
    if token.len() < 32 || token.len() > 256 || code.is_empty() || code.len() > 64 {
        return Err(unauthenticated("invalid two-factor continuation"));
    }
    let continuation = reserve_totp_attempt(state, token).await?;
    crate::platform::rate_limit::enforce_credential_attempt(
        state,
        "signin-totp",
        continuation.user_id.as_bytes(),
    )
    .await?;
    let user = find_user_by_username(&state.pool, &continuation.username)
        .await?
        .filter(|user| user.id == continuation.user_id)
        .ok_or_else(|| unauthenticated("account is unavailable"))?;
    verify_user_totp(state, &user, code).await?;
    let consumed = state
        .state_store
        .take(&totp_continuation_key(token))
        .await
        .map_err(internal_error)?
        .ok_or_else(|| unauthenticated("two-factor continuation was already used"))?;
    let consumed: TotpContinuation = rmp_serde::from_slice(&consumed).map_err(internal_error)?;
    if consumed.user_id != user.id || consumed.username != user.username {
        return Err(unauthenticated("invalid two-factor continuation"));
    }
    let refresh_transport = refresh_transport_from_headers(headers)?;
    complete_login(state, headers, refresh_transport, user).await
}

async fn complete_login(
    state: &AppState,
    headers: &HeaderMap,
    refresh_transport: RefreshTransport,
    user: UserRow,
) -> Result<Response, ApiError> {
    let client = client_metadata_from_headers(headers);
    let issued_refresh = create_refresh_token(
        &state.pool,
        user.id,
        client.user_agent.as_deref(),
        client.ip_address.as_deref(),
        OffsetDateTime::now_utc() + state.refresh_token_ttl(),
    )
    .await?;
    let access_token = state
        .issue_access_token(user.id, &user.username, issued_refresh.token_id)
        .map_err(internal_error)?;
    let device_enrollment_token = super::device_enrollment::issue(state, user.id).await?;
    let refresh_token = match refresh_transport {
        RefreshTransport::Body => Some(issued_refresh.token.clone()),
        RefreshTransport::Cookie => None,
    };
    let csrf_token =
        matches!(refresh_transport, RefreshTransport::Cookie).then(generate_csrf_token);
    let mut response = MsgPack(SigninFinishResponse {
        access_token: Some(access_token),
        refresh_token,
        refresh_token_id: Some(issued_refresh.token_id),
        totp_verified: true,
        encrypted_master_key: user.encrypted_master_key,
        public_key_bundle: user.public_key_bundle,
        totp_continuation_token: None,
        device_enrollment_token: Some(device_enrollment_token),
        csrf_token: csrf_token.clone(),
    })
    .into_response();

    if matches!(refresh_transport, RefreshTransport::Cookie) {
        set_refresh_cookie(&state.config, &mut response, &issued_refresh.token)?;
        set_csrf_cookie(
            &state.config,
            &mut response,
            csrf_token
                .as_deref()
                .ok_or_else(|| internal_error("missing CSRF token"))?,
        )?;
    }
    Ok(response)
}

async fn verify_user_totp(state: &AppState, user: &UserRow, code: &str) -> Result<(), ApiError> {
    let ciphertext = user
        .totp_secret_ciphertext
        .as_deref()
        .ok_or_else(|| unauthenticated("two-factor authentication is not enabled"))?;
    let secret = decrypt_user_totp(
        &state.config.auth_totp_kek,
        &user.id.to_string(),
        ciphertext,
    )
    .map_err(internal_error)?;
    let totp_ok = verify_totp(
        &secret,
        code,
        OffsetDateTime::now_utc(),
        TotpConfig::default(),
    )
    .unwrap_or(false);
    let backup_ok = if totp_ok {
        false
    } else if let Ok(canonical) = normalize_recovery_code(code) {
        consume_totp_backup_code(
            &state.pool,
            user.id,
            &hash_account_recovery_code(&canonical),
        )
        .await?
    } else {
        false
    };
    if !totp_ok && !backup_ok {
        return Err(unauthenticated("invalid two-factor code"));
    }
    Ok(())
}

async fn create_totp_continuation(state: &AppState, user: &UserRow) -> Result<String, ApiError> {
    let continuation = TotpContinuation {
        user_id: user.id,
        username: user.username.clone(),
        attempts: 0,
    };
    let encoded = rmp_serde::to_vec_named(&continuation).map_err(internal_error)?;
    for _ in 0..3 {
        let mut secret = [0_u8; 32];
        rand::rng().fill(&mut secret);
        let token = URL_SAFE_NO_PAD.encode(secret);
        if state
            .state_store
            .put_if_absent(
                &totp_continuation_key(&token),
                &encoded,
                TOTP_CONTINUATION_TTL,
            )
            .await
            .map_err(internal_error)?
        {
            return Ok(token);
        }
    }
    Err(internal_error("failed to allocate two-factor continuation"))
}

async fn reserve_totp_attempt(state: &AppState, token: &str) -> Result<TotpContinuation, ApiError> {
    let key = totp_continuation_key(token);
    for _ in 0..3 {
        let current = state
            .state_store
            .get(&key)
            .await
            .map_err(internal_error)?
            .ok_or_else(|| unauthenticated("two-factor continuation expired"))?;
        let mut continuation: TotpContinuation =
            rmp_serde::from_slice(&current).map_err(internal_error)?;
        if continuation.attempts >= TOTP_CONTINUATION_MAX_ATTEMPTS {
            let _ = state.state_store.take(&key).await;
            return Err(unauthenticated(
                "two-factor continuation attempt limit exceeded",
            ));
        }
        continuation.attempts = continuation.attempts.saturating_add(1);
        let updated = rmp_serde::to_vec_named(&continuation).map_err(internal_error)?;
        if state
            .state_store
            .compare_and_set(&key, &current, &updated, TOTP_CONTINUATION_TTL)
            .await
            .map_err(internal_error)?
        {
            return Ok(continuation);
        }
    }
    Err(unauthenticated(
        "two-factor continuation changed concurrently; retry",
    ))
}

fn totp_continuation_key(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    format!("auth:totp-continuation:{}", URL_SAFE_NO_PAD.encode(digest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{
        secret_box::encrypt_user_totp, security::auth::generate_totp_for_test,
        test_support::test_state,
    };

    #[tokio::test]
    async fn verifies_totp_from_the_authenticated_opaque_round() {
        let mut state = test_state();
        state.config.enable_totp = true;
        let user_id = uuid::Uuid::new_v4();
        let secret = "JBSWY3DPEHPK3PXP";
        let encrypted_secret =
            encrypt_user_totp(&state.config.auth_totp_kek, &user_id.to_string(), secret)
                .expect("encrypt TOTP secret");
        let user = UserRow {
            id: user_id,
            username: "alice".to_string(),
            opaque_record: None,
            totp_secret_ciphertext: Some(encrypted_secret),
            encrypted_master_key: vec![],
            public_key_bundle: vec![],
        };
        let code = generate_totp_for_test(secret, OffsetDateTime::now_utc());

        verify_user_totp(&state, &user, &code)
            .await
            .expect("verify TOTP");
        state.pool.close().await;
    }
}
