//! Service logic for password sign-in and pre-auth/TOTP branching.

use axum::{
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    features::auth::dto::{
        SigninFinishRequest, SigninFinishResponse, SigninNextStep, SigninStartRequest,
        SigninStartResponse,
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
        jwt::TokenKind,
        secret_box::decrypt_user_totp,
        security::auth::{TotpConfig, verify_totp},
    },
};

use super::support::{hash_account_recovery_code, normalize_recovery_code, normalize_username};

pub(crate) async fn signin_start(
    state: &AppState,
    payload: SigninStartRequest,
) -> Result<SigninStartResponse, ApiError> {
    if payload.opaque_start_request.is_empty() || payload.opaque_start_request.len() > 8 * 1024 {
        return Err(bad_request("opaque_start_request has invalid size"));
    }
    let username = normalize_username(&payload.username)
        .map_err(|_| unauthenticated("invalid credentials"))?;
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
        .map_err(internal_error)?;

    // Do not disclose account existence or TOTP configuration before password proof.
    let totp_required = false;
    let preauth_token = None;

    let next_step = if totp_required {
        SigninNextStep::TotpRequired
    } else {
        SigninNextStep::Continue
    };

    Ok(SigninStartResponse {
        opaque_flow_id: opaque.flow_id,
        opaque_server_message: opaque.message,
        next_step,
        preauth_token,
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

    let (access_token, totp_verified, preauth_token) = issue_login_tokens(
        state,
        &user,
        payload.totp_code.as_deref(),
        payload.preauth_token.as_deref(),
    )
    .await?;

    let issued_refresh = if access_token.is_some() {
        let client = client_metadata_from_headers(headers);
        Some(
            create_refresh_token(
                &state.pool,
                user.id,
                client.user_agent.as_deref(),
                client.ip_address.as_deref(),
                OffsetDateTime::now_utc() + state.refresh_token_ttl(),
            )
            .await?,
        )
    } else {
        None
    };

    let refresh_token = match refresh_transport {
        RefreshTransport::Body => issued_refresh.as_ref().map(|issued| issued.token.clone()),
        RefreshTransport::Cookie => None,
    };
    let refresh_token_id = issued_refresh.as_ref().map(|issued| issued.token_id);

    let mut response = MsgPack(SigninFinishResponse {
        access_token,
        refresh_token,
        refresh_token_id,
        totp_verified,
        encrypted_master_key: user.encrypted_master_key,
        public_key_bundle: user.public_key_bundle,
        preauth_token,
    })
    .into_response();

    if matches!(refresh_transport, RefreshTransport::Cookie)
        && let Some(issued) = issued_refresh.as_ref()
    {
        set_refresh_cookie(&state.config, &mut response, &issued.token)?;
        let csrf_token = generate_csrf_token();
        set_csrf_cookie(&state.config, &mut response, &csrf_token)?;
    }

    Ok(response)
}

async fn issue_login_tokens(
    state: &AppState,
    user: &UserRow,
    totp_code: Option<&str>,
    preauth_token: Option<&str>,
) -> Result<(Option<String>, bool, Option<String>), ApiError> {
    let mut totp_verified = !state.config.enable_totp || user.totp_secret_ciphertext.is_none();
    let mut pending_preauth_token: Option<String> = None;
    let mut access_token: Option<String> = None;

    if state.config.enable_totp
        && let Some(ciphertext) = &user.totp_secret_ciphertext
    {
        let secret = decrypt_user_totp(
            &state.config.auth_totp_kek,
            &user.id.to_string(),
            ciphertext,
        )
        .map_err(internal_error)?;
        let totp_code = totp_code.map(str::trim).filter(|value| !value.is_empty());

        match (totp_code, preauth_token) {
            (Some(code), Some(preauth)) => {
                validate_preauth_token_for_user(state, user.id, preauth)?;

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
                totp_verified = true;
            }
            _ => {
                pending_preauth_token = Some(
                    state
                        .issue_preauth_token(user.id, &user.username)
                        .map_err(internal_error)?,
                );
                totp_verified = false;
            }
        }
    }

    if totp_verified {
        access_token = Some(
            state
                .issue_access_token(user.id, &user.username)
                .map_err(internal_error)?,
        );
    }

    Ok((access_token, totp_verified, pending_preauth_token))
}

fn validate_preauth_token_for_user(
    state: &AppState,
    user_id: Uuid,
    preauth: &str,
) -> Result<(), ApiError> {
    let claims = state.validate_token(preauth).map_err(internal_error)?;
    if claims.kind != TokenKind::PreAuth {
        return Err(unauthenticated("invalid preauth token"));
    }
    if claims.user_id != user_id {
        return Err(unauthenticated("preauth token mismatch"));
    }
    Ok(())
}
