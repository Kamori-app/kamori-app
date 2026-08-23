//! Service logic for refresh rotation and session revocation endpoints.

use axum::{
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use time::OffsetDateTime;

use crate::{
    features::auth::dto::{
        CsrfBootstrapResponse, ListSessionsResponse, LogoutRequest, LogoutResponse, RefreshRequest,
        RefreshResponse, RevokeSessionRequest, RevokeSessionResponse,
    },
    features::auth::{
        repositories::{
            RefreshRotation, list_refresh_sessions, revoke_refresh_token_by_hash,
            revoke_refresh_token_by_id_for_user, rotate_refresh_token,
        },
        transport::{
            RefreshTransport, clear_csrf_cookie, clear_refresh_cookie,
            client_metadata_from_headers, generate_csrf_token, hash_refresh_token,
            read_csrf_cookie, read_refresh_cookie, refresh_transport_from_headers, set_csrf_cookie,
            set_refresh_cookie, validate_cookie_csrf, validate_cookie_request_origin,
        },
    },
    features::common::{ApiError, MsgPack, bad_request, internal_error, unauthenticated},
    platform::state::AppState,
};

pub(crate) async fn refresh(
    state: &AppState,
    headers: &HeaderMap,
    payload: RefreshRequest,
) -> Result<Response, ApiError> {
    let refresh_transport = refresh_transport_from_headers(headers)?;
    if matches!(refresh_transport, RefreshTransport::Cookie) {
        validate_cookie_request_origin(&state.config, headers)?;
        validate_cookie_csrf(&state.config, headers)?;
    }

    let refresh_token = match refresh_transport {
        RefreshTransport::Body => payload
            .refresh_token
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .ok_or_else(|| bad_request("refresh_token is required"))?,
        RefreshTransport::Cookie => read_refresh_cookie(&state.config, headers)
            .ok_or_else(|| unauthenticated("invalid refresh token"))?,
    };

    let token_hash = hash_refresh_token(&refresh_token)?;
    let client = client_metadata_from_headers(headers);
    let rotated = rotate_refresh_token(
        &state.pool,
        RefreshRotation {
            current_token_hash: &token_hash,
            current_token: &refresh_token,
            rotation_request_id: payload.rotation_request_id,
            rotation_key: &state.config.refresh_rotation_key,
            user_agent: client.user_agent.as_deref(),
            ip_address: client.ip_address.as_deref(),
            expires_at: OffsetDateTime::now_utc() + state.refresh_token_ttl(),
        },
    )
    .await?;

    let access_token = state
        .issue_access_token(rotated.user_id, &rotated.username, rotated.new_token_id)
        .map_err(internal_error)?;

    let next_csrf_token =
        matches!(refresh_transport, RefreshTransport::Cookie).then(generate_csrf_token);
    let mut response = MsgPack(RefreshResponse {
        access_token,
        username: rotated.username,
        refresh_token: match refresh_transport {
            RefreshTransport::Body => Some(rotated.new_token.clone()),
            RefreshTransport::Cookie => None,
        },
        refresh_token_id: Some(rotated.new_token_id),
        csrf_token: next_csrf_token.clone(),
    })
    .into_response();

    if matches!(refresh_transport, RefreshTransport::Cookie) {
        set_refresh_cookie(&state.config, &mut response, &rotated.new_token)?;
        set_csrf_cookie(
            &state.config,
            &mut response,
            next_csrf_token
                .as_deref()
                .ok_or_else(|| internal_error("missing rotated CSRF token"))?,
        )?;
    }

    Ok(response)
}

/// Returns the host-only double-submit token to an explicitly allowed browser
/// origin. This bridges the intentional `api.*` / `app.*` origin split without
/// making either authentication cookie visible to sibling subdomains.
pub(crate) async fn csrf_bootstrap(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<Response, ApiError> {
    if refresh_transport_from_headers(headers)? != RefreshTransport::Cookie {
        return Err(bad_request("CSRF bootstrap requires cookie transport"));
    }
    validate_cookie_request_origin(&state.config, headers)?;
    if read_refresh_cookie(&state.config, headers).is_none() {
        return Err(unauthenticated("refresh session is unavailable"));
    }
    let existing =
        read_csrf_cookie(&state.config, headers).filter(|token| (32..=128).contains(&token.len()));
    let should_set_cookie = existing.is_none();
    let csrf_token = existing.unwrap_or_else(generate_csrf_token);
    let mut response = MsgPack(CsrfBootstrapResponse {
        csrf_token: csrf_token.clone(),
    })
    .into_response();
    if should_set_cookie {
        set_csrf_cookie(&state.config, &mut response, &csrf_token)?;
    }
    Ok(response)
}

pub(crate) async fn logout(
    state: &AppState,
    headers: &HeaderMap,
    payload: LogoutRequest,
) -> Result<Response, ApiError> {
    let refresh_transport = refresh_transport_from_headers(headers)?;

    if matches!(refresh_transport, RefreshTransport::Cookie) {
        validate_cookie_request_origin(&state.config, headers)?;
        if read_refresh_cookie(&state.config, headers).is_some() {
            validate_cookie_csrf(&state.config, headers)?;
        }
    }

    let refresh_token = match refresh_transport {
        RefreshTransport::Body => payload
            .refresh_token
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .ok_or_else(|| bad_request("refresh_token is required"))?,
        RefreshTransport::Cookie => read_refresh_cookie(&state.config, headers).unwrap_or_default(),
    };

    let revoked = if refresh_token.is_empty() {
        false
    } else {
        let token_hash = hash_refresh_token(&refresh_token)?;
        revoke_refresh_token_by_hash(&state.pool, &token_hash).await?
    };

    let mut response = MsgPack(LogoutResponse { revoked }).into_response();
    if matches!(refresh_transport, RefreshTransport::Cookie) {
        clear_refresh_cookie(&state.config, &mut response)?;
        clear_csrf_cookie(&state.config, &mut response)?;
    }

    Ok(response)
}

pub(crate) async fn revoke(
    state: &AppState,
    headers: &HeaderMap,
    payload: RevokeSessionRequest,
) -> Result<RevokeSessionResponse, ApiError> {
    let principal = crate::features::common::authorize_principal(state, headers).await?;
    super::reauth::consume_reauth_token(
        state,
        &payload.reauth_token,
        principal.user_id,
        &principal.username,
        crate::features::auth::dto::ReauthAction::SecuritySettings,
    )
    .await?;
    let revoked = revoke_refresh_token_by_id_for_user(
        &state.pool,
        principal.user_id,
        payload.refresh_token_id,
    )
    .await?;

    Ok(RevokeSessionResponse { revoked })
}

pub(crate) async fn list_sessions(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<ListSessionsResponse, ApiError> {
    let principal = crate::features::common::authorize_principal(state, headers).await?;
    Ok(ListSessionsResponse {
        sessions: list_refresh_sessions(&state.pool, principal.user_id, principal.session_id)
            .await?,
    })
}
