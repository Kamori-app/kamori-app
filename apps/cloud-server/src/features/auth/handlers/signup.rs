//! HTTP handlers for OPAQUE sign-up endpoints.

use axum::extract::State;

use crate::{
    features::auth::dto::{
        SignupFinishRequest, SignupFinishResponse, SignupStartRequest, SignupStartResponse,
    },
    features::auth::services as auth_services,
    features::common::{ApiError, MsgPack},
    platform::state::AppState,
};

pub async fn signup_start(
    State(state): State<AppState>,
    MsgPack(payload): MsgPack<SignupStartRequest>,
) -> Result<MsgPack<SignupStartResponse>, ApiError> {
    Ok(MsgPack(auth_services::signup_start(&state, payload).await?))
}

pub async fn signup_finish(
    State(state): State<AppState>,
    MsgPack(payload): MsgPack<SignupFinishRequest>,
) -> Result<MsgPack<SignupFinishResponse>, ApiError> {
    Ok(MsgPack(
        auth_services::signup_finish(&state, payload).await?,
    ))
}
