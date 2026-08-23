//! Authorization, integrity, and strict quota policy for encrypted blobs.

use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use axum::{
    body::{Body, Bytes},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::Response,
};
use futures_util::Stream;
use object_store::{ObjectStoreExt, PutPayload, path::Path};
use sha2::{Digest, Sha256};
use tokio::sync::OwnedSemaphorePermit;
use uuid::Uuid;

use crate::{
    features::{
        cas::{
            dto::{CasUploadRequest, CasUploadResponse},
            repositories::{
                EgressLimits, ReserveDownloadResult, StoreBlobResult,
                finalize_download_reservation, mark_blob_ready, reserve_download, store_blob,
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
    let runtime_limits = crate::features::admin::services::effective_u64_values(
        state,
        &[
            ("max_blob_bytes", state.config.max_blob_bytes),
            ("account_storage_bytes", state.config.account_storage_bytes),
        ],
    )
    .await?;
    let max_blob_bytes = runtime_limit(&runtime_limits, "max_blob_bytes")?;
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
        i64_limit(runtime_limit(&runtime_limits, "account_storage_bytes")?)?,
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
) -> Result<Response, ApiError> {
    let actor_id = authorize_session(state, headers).await?;
    let runtime_limits = crate::features::admin::services::effective_u64_values(
        state,
        &[
            (
                "owner_monthly_egress_bytes",
                state.config.owner_monthly_egress_bytes,
            ),
            (
                "owner_rolling_24h_egress_bytes",
                state.config.owner_rolling_24h_egress_bytes,
            ),
            (
                "global_nonessential_egress_stop_bytes",
                state.config.global_nonessential_egress_stop_bytes,
            ),
            (
                "global_emergency_egress_breaker_bytes",
                state.config.global_emergency_egress_breaker_bytes,
            ),
            (
                "owner_concurrent_blob_downloads",
                state.config.owner_concurrent_blob_downloads,
            ),
            (
                "global_concurrent_blob_downloads",
                state.config.global_concurrent_blob_downloads,
            ),
            (
                "blob_download_bytes_per_second",
                state.config.blob_download_bytes_per_second,
            ),
        ],
    )
    .await?;
    let stream_permit = state
        .blob_download_semaphore
        .clone()
        .try_acquire_owned()
        .map_err(|_| quota_exceeded("blob gateway concurrency limit exceeded"))?;
    let result = reserve_download(
        &state.pool,
        actor_id,
        space_id,
        blob_id,
        EgressLimits {
            owner_monthly: i64_limit(runtime_limit(
                &runtime_limits,
                "owner_monthly_egress_bytes",
            )?)?,
            owner_rolling_24h: i64_limit(runtime_limit(
                &runtime_limits,
                "owner_rolling_24h_egress_bytes",
            )?)?,
            global_nonessential_stop: i64_limit(runtime_limit(
                &runtime_limits,
                "global_nonessential_egress_stop_bytes",
            )?)?,
            global_emergency_breaker: i64_limit(runtime_limit(
                &runtime_limits,
                "global_emergency_egress_breaker_bytes",
            )?)?,
            owner_concurrent_downloads: i64_limit(runtime_limit(
                &runtime_limits,
                "owner_concurrent_blob_downloads",
            )?)?,
            global_concurrent_downloads: i64_limit(runtime_limit(
                &runtime_limits,
                "global_concurrent_blob_downloads",
            )?)?,
        },
    )
    .await
    .map_err(internal_error)?;
    match result {
        ReserveDownloadResult::Reserved(reservation) => {
            let object_path = Path::parse(&reservation.blob.object_key).map_err(internal_error)?;
            let object = match state.object_store.get(&object_path).await {
                Ok(object) => object,
                Err(error) => {
                    finalize_download_reservation(&state.pool, reservation.id, 0)
                        .await
                        .map_err(internal_error)?;
                    tracing::error!(%error, %blob_id, "ciphertext object download failed");
                    return Err(internal_error(
                        "ciphertext storage is temporarily unavailable",
                    ));
                }
            };
            let size = u64::try_from(reservation.blob.size_padded).map_err(internal_error)?;
            let hash = reservation.blob.ciphertext_sha256.clone();
            let expected_hash: [u8; 32] = hash
                .as_slice()
                .try_into()
                .map_err(|_| internal_error("stored ciphertext hash metadata is invalid"))?;
            let stream = MeteredObjectStream::new(
                object.into_stream(),
                size,
                expected_hash,
                state.pool.clone(),
                reservation.id,
                runtime_limit(&runtime_limits, "blob_download_bytes_per_second")?,
                stream_permit,
            );
            let mut response = Response::new(Body::from_stream(stream));
            *response.status_mut() = StatusCode::OK;
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            );
            response.headers_mut().insert(
                header::CONTENT_LENGTH,
                HeaderValue::from_str(&size.to_string()).map_err(internal_error)?,
            );
            response.headers_mut().insert(
                "x-kamori-ciphertext-sha256",
                HeaderValue::from_str(&hex::encode(hash)).map_err(internal_error)?,
            );
            Ok(response)
        }
        ReserveDownloadResult::AccessDenied => Err(unauthorized("space read access denied")),
        ReserveDownloadResult::NotFound => Err(not_found("blob not found")),
        ReserveDownloadResult::OwnerQuotaExceeded => {
            Err(quota_exceeded("owner blob download quota exceeded"))
        }
        ReserveDownloadResult::ConcurrentLimitExceeded => Err(quota_exceeded(
            "owner concurrent blob download limit exceeded",
        )),
        ReserveDownloadResult::GlobalConcurrentLimitExceeded => Err(quota_exceeded(
            "global concurrent blob download limit exceeded",
        )),
        ReserveDownloadResult::GlobalQuotaExceeded => Err(quota_exceeded(
            "blob delivery is temporarily limited by the global safety budget",
        )),
        ReserveDownloadResult::EmergencyBreakerExceeded => Err(quota_exceeded(
            "blob delivery is disabled by the global emergency budget",
        )),
    }
}

struct MeteredObjectStream {
    source: Pin<Box<dyn Stream<Item = object_store::Result<Bytes>> + Send>>,
    expected_size: u64,
    expected_hash: [u8; 32],
    observed: u64,
    hasher: Sha256,
    pool: sqlx::PgPool,
    reservation_id: Uuid,
    delivered: u64,
    finalized: bool,
    ended: bool,
    bytes_per_second: u64,
    pending_chunk: Option<(Bytes, Pin<Box<tokio::time::Sleep>>)>,
    _permit: OwnedSemaphorePermit,
}

impl MeteredObjectStream {
    fn new(
        source: Pin<Box<dyn Stream<Item = object_store::Result<Bytes>> + Send>>,
        expected_size: u64,
        expected_hash: [u8; 32],
        pool: sqlx::PgPool,
        reservation_id: Uuid,
        bytes_per_second: u64,
        permit: OwnedSemaphorePermit,
    ) -> Self {
        Self {
            source,
            expected_size,
            expected_hash,
            observed: 0,
            hasher: Sha256::new(),
            pool,
            reservation_id,
            delivered: 0,
            finalized: false,
            ended: false,
            bytes_per_second,
            pending_chunk: None,
            _permit: permit,
        }
    }

    fn finalize_reservation(&mut self) {
        if self.finalized {
            return;
        }
        self.finalized = true;
        let pool = self.pool.clone();
        let reservation_id = self.reservation_id;
        // Origin bytes are billable even when the client disconnects while a
        // throttled chunk is pending. Charging the larger counter prevents an
        // attacker from repeatedly forcing unmetered B2 reads by aborting.
        let billable = self.delivered.max(self.observed).min(self.expected_size);
        let delivered = i64::try_from(billable).unwrap_or(i64::MAX);
        tokio::spawn(async move {
            if let Err(error) =
                finalize_download_reservation(&pool, reservation_id, delivered).await
            {
                tracing::error!(%error, %reservation_id, "download reservation reconciliation failed");
            }
        });
    }
}

impl Stream for MeteredObjectStream {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.ended {
            return Poll::Ready(None);
        }
        if let Some((_, delay)) = self.pending_chunk.as_mut() {
            if delay.as_mut().poll(cx).is_pending() {
                return Poll::Pending;
            }
            let Some((bytes, _)) = self.pending_chunk.take() else {
                self.ended = true;
                self.finalize_reservation();
                return Poll::Ready(Some(Err(std::io::Error::other(
                    "download throttle state was lost",
                ))));
            };
            self.delivered = self.delivered.saturating_add(bytes.len() as u64);
            return Poll::Ready(Some(Ok(bytes)));
        }
        let bytes = match self.source.as_mut().poll_next(cx) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(Some(Ok(bytes))) if bytes.is_empty() => return self.poll_next(cx),
            Poll::Ready(Some(Ok(bytes))) => bytes,
            Poll::Ready(Some(Err(error))) => {
                self.ended = true;
                self.finalize_reservation();
                return Poll::Ready(Some(Err(std::io::Error::other(error))));
            }
            Poll::Ready(None) => {
                self.ended = true;
                let integrity_ok = self.observed == self.expected_size
                    && self.hasher.clone().finalize().as_slice() == self.expected_hash;
                self.finalize_reservation();
                if integrity_ok {
                    return Poll::Ready(None);
                }
                return Poll::Ready(Some(Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "stored ciphertext failed streaming integrity verification",
                ))));
            }
        };
        self.observed = self.observed.saturating_add(bytes.len() as u64);
        if self.observed > self.expected_size {
            self.ended = true;
            self.finalize_reservation();
            return Poll::Ready(Some(Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "stored ciphertext exceeds its declared size",
            ))));
        }
        self.hasher.update(&bytes);
        let nanos = (bytes.len() as u128)
            .saturating_mul(1_000_000_000)
            .checked_div(u128::from(self.bytes_per_second.max(1)))
            .unwrap_or(0)
            .min(u128::from(u64::MAX));
        self.pending_chunk = Some((
            bytes,
            Box::pin(tokio::time::sleep(Duration::from_nanos(nanos as u64))),
        ));
        self.poll_next(cx)
    }
}

impl Drop for MeteredObjectStream {
    fn drop(&mut self) {
        self.finalize_reservation();
    }
}

fn i64_limit(value: u64) -> Result<i64, ApiError> {
    i64::try_from(value).map_err(|_| internal_error("configured quota exceeds database range"))
}

fn runtime_limit(values: &HashMap<String, u64>, key: &str) -> Result<u64, ApiError> {
    values
        .get(key)
        .copied()
        .ok_or_else(|| internal_error(format!("effective runtime setting {key} is missing")))
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
