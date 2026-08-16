//! HTTP handlers for refresh/logout/revoke session endpoints.

use axum::{extract::State, http::HeaderMap, response::Response};

use crate::{
    features::auth::dto::{
        ListSessionsResponse, LogoutRequest, RefreshRequest, RevokeSessionRequest,
        RevokeSessionResponse,
    },
    features::auth::services as auth_services,
    features::common::{ApiError, MsgPack},
    platform::state::AppState,
};

pub async fn refresh(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<RefreshRequest>,
) -> Result<Response, ApiError> {
    auth_services::refresh(&state, &headers, payload).await
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<LogoutRequest>,
) -> Result<Response, ApiError> {
    auth_services::logout(&state, &headers, payload).await
}

pub async fn revoke(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<RevokeSessionRequest>,
) -> Result<MsgPack<RevokeSessionResponse>, ApiError> {
    Ok(MsgPack(
        auth_services::revoke(&state, &headers, payload).await?,
    ))
}

pub async fn list_sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<MsgPack<ListSessionsResponse>, ApiError> {
    Ok(MsgPack(
        auth_services::list_sessions(&state, &headers).await?,
    ))
}
