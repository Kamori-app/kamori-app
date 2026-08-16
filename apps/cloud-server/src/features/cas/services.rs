//! Authorization, integrity, and strict quota policy for encrypted blobs.

use axum::http::HeaderMap;
use object_store::{ObjectStoreExt, PutPayload, path::Path};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    features::{
        cas::{
            dto::{CasDownloadResponse, CasUploadRequest, CasUploadResponse},
            repositories::{
                EgressLimits, FindDownloadResult, ReserveDownloadResult, StoreBlobResult,
                find_download_blob, mark_blob_ready, reserve_download, store_blob,
            },
        },
        common::{
            ApiError, authorize_session, bad_request, conflict, internal_error, not_found,
            quota_exceeded, unauthorized,
        },
    },
    platform::state::AppState,
};

const CHUNK_SIZE: u64 = 1024 * 1024;

pub(crate) async fn cas_upload(
    state: &AppState,
    headers: &HeaderMap,
    space_id: Uuid,
    payload: CasUploadRequest,
) -> Result<CasUploadResponse, ApiError> {
    let actor_id = authorize_session(state, headers).await?;
    let max_blob_bytes = crate::features::admin::services::effective_u64(
        state,
        "max_blob_bytes",
        state.config.max_blob_bytes,
    )
    .await?;
    if payload.blob_id.is_nil() {
        return Err(bad_request("blob_id must be a random non-nil UUID"));
    }
    if payload.size_padded == 0
        || payload.size_padded > max_blob_bytes
        || !payload.size_padded.is_multiple_of(CHUNK_SIZE)
    {
        return Err(bad_request(
            "size_padded must be positive, 1 MiB aligned, and within the blob limit",
        ));
    }
    if payload.data.len() as u64 != payload.size_padded {
        return Err(bad_request("data length must match size_padded"));
    }
    if payload.ciphertext_sha256.len() != 32
        || Sha256::digest(&payload.data).as_slice() != payload.ciphertext_sha256
    {
        return Err(bad_request(
            "ciphertext_sha256 does not match uploaded bytes",
        ));
    }

    let result = store_blob(
        &state.pool,
        actor_id,
        space_id,
        payload.blob_id,
        &payload.ciphertext_sha256,
        i64_limit(payload.size_padded)?,
        i64_limit(
            crate::features::admin::services::effective_u64(
                state,
                "account_storage_bytes",
                state.config.account_storage_bytes,
            )
            .await?,
        )?,
    )
    .await
    .map_err(internal_error)?;
    match result {
        StoreBlobResult::NeedsUpload(blob) => {
            let object_path = Path::parse(&blob.object_key).map_err(internal_error)?;
            state
                .object_store
                .put(&object_path, PutPayload::from(payload.data))
                .await
                .map_err(|error| {
                    tracing::error!(%error, blob_id = %blob.blob_id, "ciphertext object upload failed");
                    internal_error("ciphertext storage is temporarily unavailable")
                })?;
            mark_blob_ready(&state.pool, space_id, blob.blob_id, &blob.ciphertext_sha256)
                .await
                .map_err(internal_error)?;
            Ok(CasUploadResponse {
                blob_id: payload.blob_id,
                stored: true,
            })
        }
        StoreBlobResult::AlreadyStored => Ok(CasUploadResponse {
            blob_id: payload.blob_id,
            stored: false,
        }),
        StoreBlobResult::AccessDenied => Err(unauthorized("space write access denied")),
        StoreBlobResult::StorageQuotaExceeded => {
            Err(quota_exceeded("account ciphertext storage quota exceeded"))
        }
        StoreBlobResult::IdConflict => Err(conflict("blob_id already names different bytes")),
    }
}

pub(crate) async fn cas_download(
    state: &AppState,
    headers: &HeaderMap,
    space_id: Uuid,
    blob_id: Uuid,
) -> Result<CasDownloadResponse, ApiError> {
    let actor_id = authorize_session(state, headers).await?;
    let metadata = match find_download_blob(&state.pool, actor_id, space_id, blob_id)
        .await
        .map_err(internal_error)?
    {
        FindDownloadResult::Found(blob) => blob,
        FindDownloadResult::AccessDenied => {
            return Err(unauthorized("space read access denied"));
        }
        FindDownloadResult::NotFound => return Err(not_found("blob not found")),
    };
    let object_path = Path::parse(&metadata.object_key).map_err(internal_error)?;
    let data = state
        .object_store
        .get(&object_path)
        .await
        .map_err(|error| {
            tracing::error!(%error, %blob_id, "ciphertext object download failed");
            internal_error("ciphertext storage is temporarily unavailable")
        })?
        .bytes()
        .await
        .map_err(|error| {
            tracing::error!(%error, %blob_id, "ciphertext object body failed");
            internal_error("ciphertext storage is temporarily unavailable")
        })?;
    if data.len() != usize::try_from(metadata.size_padded).map_err(internal_error)?
        || Sha256::digest(&data).as_slice() != metadata.ciphertext_sha256
    {
        tracing::error!(%blob_id, "ciphertext object failed integrity verification");
        return Err(internal_error(
            "stored ciphertext failed integrity verification",
        ));
    }
    let result = reserve_download(
        &state.pool,
        actor_id,
        space_id,
        blob_id,
        EgressLimits {
            owner_monthly: i64_limit(
                crate::features::admin::services::effective_u64(
                    state,
                    "owner_monthly_egress_bytes",
                    state.config.owner_monthly_egress_bytes,
                )
                .await?,
            )?,
            owner_rolling_24h: i64_limit(
                crate::features::admin::services::effective_u64(
                    state,
                    "owner_rolling_24h_egress_bytes",
                    state.config.owner_rolling_24h_egress_bytes,
                )
                .await?,
            )?,
            global_nonessential_stop: i64_limit(
                crate::features::admin::services::effective_u64(
                    state,
                    "global_nonessential_egress_stop_bytes",
                    state.config.global_nonessential_egress_stop_bytes,
                )
                .await?,
            )?,
        },
    )
    .await
    .map_err(internal_error)?;
    match result {
        ReserveDownloadResult::Reserved(blob) => Ok(CasDownloadResponse {
            blob_id: blob.blob_id,
            ciphertext_sha256: blob.ciphertext_sha256,
            size_padded: u64::try_from(blob.size_padded).map_err(internal_error)?,
            data: data.to_vec(),
        }),
        ReserveDownloadResult::AccessDenied => Err(unauthorized("space read access denied")),
        ReserveDownloadResult::NotFound => Err(not_found("blob not found")),
        ReserveDownloadResult::OwnerQuotaExceeded => {
            Err(quota_exceeded("owner blob download quota exceeded"))
        }
        ReserveDownloadResult::GlobalQuotaExceeded => Err(quota_exceeded(
            "blob delivery is temporarily limited by the global safety budget",
        )),
    }
}

fn i64_limit(value: u64) -> Result<i64, ApiError> {
    i64::try_from(value).map_err(|_| internal_error("configured quota exceeds database range"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quota_conversion_rejects_values_outside_database_range() {
        assert!(i64_limit(i64::MAX as u64).is_ok());
        assert!(i64_limit(i64::MAX as u64 + 1).is_err());
    }
}
