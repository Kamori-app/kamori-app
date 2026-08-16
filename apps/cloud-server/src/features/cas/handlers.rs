//! Space-scoped encrypted blob HTTP handlers.

use axum::{
    extract::{Path, State},
    http::HeaderMap,
};
use uuid::Uuid;

use crate::{
    features::{
        cas::{
            dto::{CasDownloadResponse, CasUploadRequest, CasUploadResponse},
            services,
        },
        common::{ApiError, MsgPack},
    },
    platform::state::AppState,
};

pub async fn cas_upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(space_id): Path<Uuid>,
    MsgPack(payload): MsgPack<CasUploadRequest>,
) -> Result<MsgPack<CasUploadResponse>, ApiError> {
    Ok(MsgPack(
        services::cas_upload(&state, &headers, space_id, payload).await?,
    ))
}

pub async fn cas_download(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((space_id, blob_id)): Path<(Uuid, Uuid)>,
) -> Result<MsgPack<CasDownloadResponse>, ApiError> {
    Ok(MsgPack(
        services::cas_download(&state, &headers, space_id, blob_id).await?,
    ))
}
