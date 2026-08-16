//! HTTP handlers for passkey registration/login/management endpoints.

use axum::{extract::State, http::HeaderMap, response::Response};

use crate::{
    features::auth::dto::{
        PasskeyAddFinishRequest, PasskeyAddFinishResponse, PasskeyAddStartRequest,
        PasskeyAddStartResponse, PasskeyDeleteRequest, PasskeyDeleteResponse, PasskeyListResponse,
        PasskeyLoginFinishRequest, PasskeyLoginStartRequest, PasskeyLoginStartResponse,
        PasskeyUpdateRequest, PasskeyUpdateResponse,
    },
    features::auth::services as auth_services,
    features::common::{ApiError, MsgPack},
    platform::state::AppState,
};

pub async fn passkey_add_start(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(_payload): MsgPack<PasskeyAddStartRequest>,
) -> Result<MsgPack<PasskeyAddStartResponse>, ApiError> {
    Ok(MsgPack(
        auth_services::passkey_add_start(&state, &headers).await?,
    ))
}

pub async fn passkey_add_finish(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<PasskeyAddFinishRequest>,
) -> Result<MsgPack<PasskeyAddFinishResponse>, ApiError> {
    Ok(MsgPack(
        auth_services::passkey_add_finish(&state, &headers, payload).await?,
    ))
}

pub async fn passkey_list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<MsgPack<PasskeyListResponse>, ApiError> {
    Ok(MsgPack(
        auth_services::passkey_list(&state, &headers).await?,
    ))
}

pub async fn passkey_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<PasskeyUpdateRequest>,
) -> Result<MsgPack<PasskeyUpdateResponse>, ApiError> {
    Ok(MsgPack(
        auth_services::passkey_update(&state, &headers, payload).await?,
    ))
}

pub async fn passkey_delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<PasskeyDeleteRequest>,
) -> Result<MsgPack<PasskeyDeleteResponse>, ApiError> {
    Ok(MsgPack(
        auth_services::passkey_delete(&state, &headers, payload).await?,
    ))
}

pub async fn passkey_login_start(
    State(state): State<AppState>,
    MsgPack(_payload): MsgPack<PasskeyLoginStartRequest>,
) -> Result<MsgPack<PasskeyLoginStartResponse>, ApiError> {
    Ok(MsgPack(auth_services::passkey_login_start(&state).await?))
}

pub async fn passkey_login_finish(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<PasskeyLoginFinishRequest>,
) -> Result<Response, ApiError> {
    auth_services::passkey_login_finish(&state, &headers, payload).await
}
