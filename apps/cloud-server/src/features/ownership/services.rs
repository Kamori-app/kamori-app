//! Ownership transfer policy exposed to authenticated clients.

use uuid::Uuid;

use crate::{
    features::{
        common::{
            ApiError, bad_request, conflict, internal_error, not_found, quota_exceeded,
            unauthorized,
        },
        ownership::{
            dto::{
                CreateOwnershipTransferRequest, CreateOwnershipTransferResponse,
                ListIncomingOwnershipTransfersResponse, ListOutgoingOwnershipTransfersResponse,
                OwnershipTransferResultResponse,
            },
            repositories::{self, AcceptOfferResult, CreateOfferResult},
        },
    },
    platform::state::AppState,
};

fn storage_limit(value: u64) -> Result<i64, ApiError> {
    i64::try_from(value).map_err(|_| internal_error("account storage limit exceeds database range"))
}

pub(crate) async fn create(
    state: &AppState,
    actor_id: Uuid,
    payload: CreateOwnershipTransferRequest,
) -> Result<CreateOwnershipTransferResponse, ApiError> {
    if payload.resource_id.is_nil() || payload.target_user_id.is_nil() {
        return Err(bad_request(
            "resource_id and target_user_id must be non-nil UUIDs",
        ));
    }
    match repositories::create_offer(
        &state.pool,
        actor_id,
        payload.resource_kind,
        payload.resource_id,
        payload.target_user_id,
    )
    .await
    .map_err(internal_error)?
    {
        CreateOfferResult::Created(offer) => Ok(CreateOwnershipTransferResponse { offer }),
        CreateOfferResult::AccessDenied => Err(unauthorized("resource owner access required")),
        CreateOfferResult::InvalidTarget => Err(bad_request("target must be an active member")),
        CreateOfferResult::PersonalWorkspace => {
            Err(bad_request("personal workspaces cannot be transferred"))
        }
        CreateOfferResult::AlreadyPending => {
            Err(conflict("an ownership transfer is already pending"))
        }
    }
}

pub(crate) async fn list_incoming(
    state: &AppState,
    actor_id: Uuid,
) -> Result<ListIncomingOwnershipTransfersResponse, ApiError> {
    Ok(ListIncomingOwnershipTransfersResponse {
        offers: repositories::list_incoming(&state.pool, actor_id)
            .await
            .map_err(internal_error)?,
    })
}

pub(crate) async fn list_outgoing(
    state: &AppState,
    actor_id: Uuid,
) -> Result<ListOutgoingOwnershipTransfersResponse, ApiError> {
    Ok(ListOutgoingOwnershipTransfersResponse {
        offers: repositories::list_outgoing(&state.pool, actor_id)
            .await
            .map_err(internal_error)?,
    })
}

pub(crate) async fn accept(
    state: &AppState,
    actor_id: Uuid,
    transfer_id: Uuid,
) -> Result<OwnershipTransferResultResponse, ApiError> {
    let effective_limits = crate::features::admin::services::effective_u64_values(
        state,
        &[
            ("account_storage_bytes", state.config.account_storage_bytes),
            (
                "account_operation_storage_bytes",
                state.config.account_operation_storage_bytes,
            ),
        ],
    )
    .await?;
    let limit = |name: &str| {
        effective_limits
            .get(name)
            .copied()
            .ok_or_else(|| internal_error("ownership-transfer quota is missing"))
            .and_then(storage_limit)
    };
    match repositories::accept_offer(
        &state.pool,
        actor_id,
        transfer_id,
        limit("account_storage_bytes")?,
        limit("account_operation_storage_bytes")?,
    )
    .await
    .map_err(internal_error)?
    {
        AcceptOfferResult::Accepted => Ok(OwnershipTransferResultResponse { changed: true }),
        AcceptOfferResult::NotFound => Err(not_found("ownership transfer not found")),
        AcceptOfferResult::NoLongerValid => Err(conflict("ownership transfer is no longer valid")),
        AcceptOfferResult::BlobStorageQuotaExceeded => Err(quota_exceeded(
            "accepting this space would exceed the target account storage quota",
        )),
        AcceptOfferResult::OperationStorageQuotaExceeded => Err(quota_exceeded(
            "accepting this space would exceed the target account operation-log quota",
        )),
    }
}

pub(crate) async fn cancel(
    state: &AppState,
    actor_id: Uuid,
    transfer_id: Uuid,
) -> Result<OwnershipTransferResultResponse, ApiError> {
    let changed = repositories::cancel_offer(&state.pool, actor_id, transfer_id)
        .await
        .map_err(internal_error)?;
    if !changed {
        return Err(not_found("ownership transfer not found"));
    }
    Ok(OwnershipTransferResultResponse { changed })
}
