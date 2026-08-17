//! Operation-log HTTP handlers.

use axum::{
    extract::{Query, State},
    http::HeaderMap,
};

use crate::{
    features::common::{ApiError, MsgPack},
    platform::state::AppState,
};

use super::{
    dto::{
        AppendOperationRequest, AppendOperationResponse, ListOperationsQuery,
        ListOperationsResponse,
    },
    services,
};

pub async fn append(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(request): MsgPack<AppendOperationRequest>,
) -> Result<MsgPack<AppendOperationResponse>, ApiError> {
    Ok(MsgPack(
        services::append(&state, &headers, request.envelope).await?,
    ))
}

pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListOperationsQuery>,
) -> Result<MsgPack<ListOperationsResponse>, ApiError> {
    Ok(MsgPack(
        services::list(&state, &headers, query.space_id, query.since, query.limit).await?,
    ))
}
