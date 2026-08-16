//! OPAQUE reauthentication handlers for destructive actions.

use axum::{extract::State, http::HeaderMap};

use crate::{
    features::{
        auth::{
            dto::{
                ReauthFinishRequest, ReauthFinishResponse, ReauthStartRequest, ReauthStartResponse,
            },
            services,
        },
        common::{ApiError, MsgPack},
    },
    platform::state::AppState,
};

pub async fn start(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<ReauthStartRequest>,
) -> Result<MsgPack<ReauthStartResponse>, ApiError> {
    Ok(MsgPack(
        services::reauth_start(&state, &headers, payload).await?,
    ))
}

pub async fn finish(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<ReauthFinishRequest>,
) -> Result<MsgPack<ReauthFinishResponse>, ApiError> {
    Ok(MsgPack(
        services::reauth_finish(&state, &headers, payload).await?,
    ))
}
