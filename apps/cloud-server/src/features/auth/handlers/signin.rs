//! HTTP handlers for password sign-in endpoints.

use axum::{extract::State, http::HeaderMap, response::Response};

use crate::{
    features::auth::dto::{SigninFinishRequest, SigninStartRequest, SigninStartResponse},
    features::auth::services as auth_services,
    features::common::{ApiError, MsgPack},
    platform::state::AppState,
};

pub async fn signin_start(
    State(state): State<AppState>,
    MsgPack(payload): MsgPack<SigninStartRequest>,
) -> Result<MsgPack<SigninStartResponse>, ApiError> {
    Ok(MsgPack(auth_services::signin_start(&state, payload).await?))
}

pub async fn signin_finish(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<SigninFinishRequest>,
) -> Result<Response, ApiError> {
    auth_services::signin_finish(&state, &headers, payload).await
}
