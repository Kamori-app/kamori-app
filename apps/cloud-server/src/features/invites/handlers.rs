//! Invite-code HTTP handlers.

use axum::{extract::State, http::HeaderMap};

use crate::{
    features::{
        common::{ApiError, MsgPack, authorize_session},
        invites::{
            dto::{
                CreateInviteCodeRequest, CreateInviteCodeResponse, RedeemInviteCodeRequest,
                RedeemInviteCodeResponse,
            },
            services,
        },
    },
    platform::state::AppState,
};

pub async fn create_invite_code(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<CreateInviteCodeRequest>,
) -> Result<MsgPack<CreateInviteCodeResponse>, ApiError> {
    let actor_id = authorize_session(&state, &headers).await?;
    Ok(MsgPack(
        services::create_invite_code(&state, actor_id, payload).await?,
    ))
}

pub async fn redeem_invite_code(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<RedeemInviteCodeRequest>,
) -> Result<MsgPack<RedeemInviteCodeResponse>, ApiError> {
    let actor_id = authorize_session(&state, &headers).await?;
    Ok(MsgPack(
        services::redeem_invite_code(&state, actor_id, payload).await?,
    ))
}
