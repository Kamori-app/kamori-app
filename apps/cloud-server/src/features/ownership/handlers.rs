//! HTTP handlers for ownership transfer offers.

use axum::{
    extract::{Path, State},
    http::HeaderMap,
};
use uuid::Uuid;

use crate::{
    features::{
        common::{ApiError, MsgPack, authorize_session},
        ownership::{
            dto::{
                CreateOwnershipTransferRequest, CreateOwnershipTransferResponse,
                ListIncomingOwnershipTransfersResponse, ListOutgoingOwnershipTransfersResponse,
                OwnershipTransferResultResponse,
            },
            services,
        },
    },
    platform::state::AppState,
};

pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<CreateOwnershipTransferRequest>,
) -> Result<MsgPack<CreateOwnershipTransferResponse>, ApiError> {
    let actor_id = authorize_session(&state, &headers).await?;
    Ok(MsgPack(services::create(&state, actor_id, payload).await?))
}

pub async fn list_incoming(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<MsgPack<ListIncomingOwnershipTransfersResponse>, ApiError> {
    let actor_id = authorize_session(&state, &headers).await?;
    Ok(MsgPack(services::list_incoming(&state, actor_id).await?))
}

pub async fn list_outgoing(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<MsgPack<ListOutgoingOwnershipTransfersResponse>, ApiError> {
    let actor_id = authorize_session(&state, &headers).await?;
    Ok(MsgPack(services::list_outgoing(&state, actor_id).await?))
}

pub async fn accept(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(transfer_id): Path<Uuid>,
) -> Result<MsgPack<OwnershipTransferResultResponse>, ApiError> {
    let actor_id = authorize_session(&state, &headers).await?;
    Ok(MsgPack(
        services::accept(&state, actor_id, transfer_id).await?,
    ))
}

pub async fn cancel(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(transfer_id): Path<Uuid>,
) -> Result<MsgPack<OwnershipTransferResultResponse>, ApiError> {
    let actor_id = authorize_session(&state, &headers).await?;
    Ok(MsgPack(
        services::cancel(&state, actor_id, transfer_id).await?,
    ))
}
