//! Service logic for passkey flows and passkey-based sign-in token issuance.

use axum::{
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;
use webauthn_rs::prelude::{
    AuthenticationResult, DiscoverableKey, PublicKeyCredential, RegisterPublicKeyCredential,
};

use crate::{
    features::auth::dto::{
        PasskeyAddFinishRequest, PasskeyAddFinishResponse, PasskeyAddStartRequest,
        PasskeyAddStartResponse, PasskeyDeleteRequest, PasskeyDeleteResponse, PasskeyListResponse,
        PasskeyLoginFinishRequest, PasskeyLoginFinishResponse, PasskeyLoginStartResponse,
        PasskeyUpdateRequest, PasskeyUpdateResponse,
    },
    features::auth::{
        repositories::{
            create_refresh_token, delete_passkey_for_user, get_user_and_passkey_by_credential_id,
            get_user_passkey, list_user_passkey_metadata, update_passkey_name_for_user,
            update_user_passkey, upsert_owned_user_passkey,
        },
        transport::{
            RefreshTransport, client_metadata_from_headers, generate_csrf_token,
            refresh_transport_from_headers, set_csrf_cookie, set_refresh_cookie,
        },
    },
    features::common::{
        ApiError, MsgPack, authorize_principal, authorize_session, bad_request, internal_error,
        unauthenticated, unauthorized,
    },
    platform::security::passkey::encode_passkey,
    platform::state::AppState,
};

pub(crate) async fn passkey_add_start(
    state: &AppState,
    headers: &HeaderMap,
    payload: PasskeyAddStartRequest,
) -> Result<PasskeyAddStartResponse, ApiError> {
    let principal = authorize_principal(state, headers).await?;
    super::reauth::consume_reauth_token(
        state,
        &payload.reauth_token,
        principal.user_id,
        &principal.username,
        crate::features::auth::dto::ReauthAction::SecuritySettings,
    )
    .await?;
    let user_id = principal.user_id;
    let username = principal.username;

    let flow_id = Uuid::new_v4();
    let options = state
        .passkeys
        .start_registration(flow_id, user_id, &username, &username)
        .await
        .map_err(internal_error)?;

    Ok(PasskeyAddStartResponse {
        flow_id,
        challenge: options.challenge,
        public_key_credential_creation_options: options.public_key_credential_creation_options,
    })
}

pub(crate) async fn passkey_add_finish(
    state: &AppState,
    headers: &HeaderMap,
    payload: PasskeyAddFinishRequest,
) -> Result<PasskeyAddFinishResponse, ApiError> {
    let user_id = authorize_session(state, headers).await?;
    validate_encrypted_passkey_name(&payload.encrypted_name)?;

    let credential: RegisterPublicKeyCredential = serde_json::from_slice(&payload.credential)
        .map_err(|_| bad_request("invalid credential"))?;
    let passkey = state
        .passkeys
        .finish_registration(payload.flow_id, user_id, credential)
        .await
        .map_err(|_| bad_request("passkey registration failed"))?;

    let credential_id = passkey.cred_id().as_ref().to_vec();
    let passkey_data = encode_passkey(&passkey).map_err(internal_error)?;
    let metadata = upsert_owned_user_passkey(
        &state.pool,
        user_id,
        &credential_id,
        &passkey_data,
        &payload.encrypted_name,
    )
    .await?;

    Ok(PasskeyAddFinishResponse { passkey: metadata })
}

pub(crate) async fn passkey_list(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<PasskeyListResponse, ApiError> {
    let user_id = authorize_session(state, headers).await?;
    let passkeys = list_user_passkey_metadata(&state.pool, user_id)
        .await
        .map_err(internal_error)?;
    Ok(PasskeyListResponse { passkeys })
}

pub(crate) async fn passkey_update(
    state: &AppState,
    headers: &HeaderMap,
    payload: PasskeyUpdateRequest,
) -> Result<PasskeyUpdateResponse, ApiError> {
    let user_id = authorize_session(state, headers).await?;
    validate_encrypted_passkey_name(&payload.encrypted_name)?;

    let updated = update_passkey_name_for_user(
        &state.pool,
        user_id,
        payload.passkey_id,
        &payload.encrypted_name,
    )
    .await
    .map_err(internal_error)?;

    let passkey = updated.ok_or_else(|| unauthorized("passkey not found"))?;
    Ok(PasskeyUpdateResponse { passkey })
}

pub(crate) async fn passkey_delete(
    state: &AppState,
    headers: &HeaderMap,
    payload: PasskeyDeleteRequest,
) -> Result<PasskeyDeleteResponse, ApiError> {
    let principal = authorize_principal(state, headers).await?;
    super::reauth::consume_reauth_token(
        state,
        &payload.reauth_token,
        principal.user_id,
        &principal.username,
        crate::features::auth::dto::ReauthAction::SecuritySettings,
    )
    .await?;
    let user_id = principal.user_id;

    let deleted = delete_passkey_for_user(&state.pool, user_id, payload.passkey_id)
        .await
        .map_err(internal_error)?;
    if !deleted {
        return Err(unauthorized("passkey not found"));
    }

    Ok(PasskeyDeleteResponse { deleted })
}

pub(crate) async fn passkey_login_start(
    state: &AppState,
) -> Result<PasskeyLoginStartResponse, ApiError> {
    let flow_id = Uuid::new_v4();
    let options = state
        .passkeys
        .start_discoverable_authentication(flow_id)
        .await
        .map_err(internal_error)?;

    Ok(PasskeyLoginStartResponse {
        flow_id,
        challenge: options.challenge,
        public_key_credential_request_options: options.public_key_credential_request_options,
    })
}

pub(crate) async fn passkey_login_finish(
    state: &AppState,
    headers: &HeaderMap,
    payload: PasskeyLoginFinishRequest,
) -> Result<Response, ApiError> {
    let credential: PublicKeyCredential = serde_json::from_slice(&payload.credential)
        .map_err(|_| bad_request("invalid credential"))?;

    let refresh_transport = refresh_transport_from_headers(headers)?;
    let credential_id = credential.get_credential_id().to_vec();
    crate::platform::rate_limit::enforce_credential_attempt(state, "passkey-login", &credential_id)
        .await?;

    let (user, passkey) =
        get_user_and_passkey_by_credential_id(&state.pool, &credential_id).await?;
    let discoverable_keys = vec![DiscoverableKey::from(passkey)];

    let auth_result = state
        .passkeys
        .finish_discoverable_authentication(payload.flow_id, credential, &discoverable_keys)
        .await
        .map_err(|_| unauthenticated("passkey authentication failed"))?;

    persist_passkey_auth_result(&state.pool, user.id, &auth_result).await?;

    let client = client_metadata_from_headers(headers);
    let refresh = create_refresh_token(
        &state.pool,
        user.id,
        client.user_agent.as_deref(),
        client.ip_address.as_deref(),
        OffsetDateTime::now_utc() + state.refresh_token_ttl(),
    )
    .await?;

    let access_token = state
        .issue_access_token(user.id, &user.username, refresh.token_id)
        .map_err(internal_error)?;
    let device_enrollment_token = super::device_enrollment::issue(state, user.id).await?;
    let refresh_token = match refresh_transport {
        RefreshTransport::Body => Some(refresh.token.clone()),
        RefreshTransport::Cookie => None,
    };

    let csrf_token =
        matches!(refresh_transport, RefreshTransport::Cookie).then(generate_csrf_token);
    let mut response = MsgPack(PasskeyLoginFinishResponse {
        username: user.username,
        access_token,
        refresh_token,
        refresh_token_id: Some(refresh.token_id),
        device_enrollment_token,
        csrf_token: csrf_token.clone(),
    })
    .into_response();

    if matches!(refresh_transport, RefreshTransport::Cookie) {
        set_refresh_cookie(&state.config, &mut response, &refresh.token)?;
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

async fn persist_passkey_auth_result(
    pool: &PgPool,
    user_id: Uuid,
    auth_result: &AuthenticationResult,
) -> Result<(), ApiError> {
    let credential_id = auth_result.cred_id().as_ref();
    let mut passkey = get_user_passkey(pool, user_id, credential_id).await?;

    let updated = passkey
        .update_credential(auth_result)
        .ok_or_else(|| unauthenticated("passkey credential mismatch"))?;
    if !updated {
        return Ok(());
    }

    let passkey_bytes = encode_passkey(&passkey).map_err(internal_error)?;
    update_user_passkey(pool, user_id, credential_id, &passkey_bytes)
        .await
        .map_err(internal_error)?;

    Ok(())
}

fn validate_encrypted_passkey_name(encrypted_name: &[u8]) -> Result<(), ApiError> {
    if encrypted_name.is_empty() {
        return Err(bad_request("encrypted_name must not be empty"));
    }
    if encrypted_name.len() > 4096 {
        return Err(bad_request("encrypted_name is too large"));
    }
    Ok(())
}
