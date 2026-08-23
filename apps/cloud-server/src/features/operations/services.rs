//! Validation and authorization for operation transport.

use axum::http::HeaderMap;
use crypto_core_lib::operation_envelope::{EnvelopeKind, OperationEnvelopeV1};

use crate::{
    features::common::{
        ApiError, authorize_session, bad_request, conflict, internal_error, unauthorized,
    },
    platform::state::AppState,
};

use super::{
    dto::{AppendOperationResponse, ListOperationsResponse},
    repositories::{self, AppendResult},
};

const DEFAULT_PAGE_LIMIT: u16 = 200;
const MAX_PAGE_LIMIT: u16 = 200;

pub(crate) async fn append(
    state: &AppState,
    headers: &HeaderMap,
    envelope: OperationEnvelopeV1,
) -> Result<AppendOperationResponse, ApiError> {
    let user_id = authorize_session(state, headers).await?;
    let limits = crate::features::admin::services::effective_u64_values(
        state,
        &[
            ("max_operation_bytes", state.config.max_operation_bytes),
            ("max_snapshot_bytes", state.config.max_snapshot_bytes),
            (
                "space_operation_storage_bytes",
                state.config.space_operation_storage_bytes,
            ),
            (
                "account_operation_storage_bytes",
                state.config.account_operation_storage_bytes,
            ),
        ],
    )
    .await?;
    let runtime_limit = |name: &str| {
        limits
            .get(name)
            .copied()
            .ok_or_else(|| internal_error("operation limit is missing"))
    };
    validate_envelope(
        &envelope,
        runtime_limit("max_operation_bytes")?,
        runtime_limit("max_snapshot_bytes")?,
    )?;

    let authorization = repositories::load_append_authorization(&state.pool, user_id, &envelope)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| unauthorized("security-space write access denied"))?;
    if !authorization.can_write {
        return Err(unauthorized("security-space is read-only for this member"));
    }
    if authorization.current_key_epoch != envelope.key_epoch {
        return Err(conflict("operation key epoch is stale"));
    }
    envelope
        .verify(&authorization.signing_public_key)
        .map_err(|_| bad_request("operation signature is invalid"))?;

    match repositories::append_operation(
        &state.pool,
        user_id,
        &envelope,
        runtime_limit("space_operation_storage_bytes")?,
        runtime_limit("account_operation_storage_bytes")?,
    )
    .await
    .map_err(internal_error)?
    {
        AppendResult::Accepted(space_seq) => Ok(AppendOperationResponse {
            accepted: true,
            duplicate: false,
            space_seq,
        }),
        AppendResult::Duplicate(space_seq) => Ok(AppendOperationResponse {
            accepted: true,
            duplicate: true,
            space_seq,
        }),
        AppendResult::ConflictingDuplicate => Err(conflict(
            "client_op_id was already used for different bytes",
        )),
        AppendResult::AccessDeniedOrStaleEpoch => {
            Err(conflict("write authorization or key epoch changed"))
        }
        AppendResult::StorageQuotaExceeded => Err(crate::features::common::quota_exceeded(
            "encrypted operation storage quota exceeded",
        )),
    }
}

pub(crate) async fn list(
    state: &AppState,
    headers: &HeaderMap,
    space_id: uuid::Uuid,
    since: u64,
    requested_limit: Option<u16>,
) -> Result<ListOperationsResponse, ApiError> {
    let user_id = authorize_session(state, headers).await?;
    if space_id.is_nil() {
        return Err(bad_request("space_id must be a non-nil UUID"));
    }
    if since > i64::MAX as u64 {
        return Err(bad_request("since exceeds the supported cursor range"));
    }
    let limit = requested_limit
        .unwrap_or(DEFAULT_PAGE_LIMIT)
        .clamp(1, MAX_PAGE_LIMIT);
    let page_bytes = crate::features::admin::services::effective_u64(
        state,
        "max_operation_page_bytes",
        state.config.max_operation_page_bytes,
    )
    .await?;
    let page =
        repositories::list_operations(&state.pool, user_id, space_id, since, limit, page_bytes)
            .await
            .map_err(internal_error)?
            .ok_or_else(|| unauthorized("security-space read access denied"))?;
    let next_cursor = page
        .operations
        .last()
        .map_or(page.effective_since, |item| item.space_seq);
    Ok(ListOperationsResponse {
        operations: page.operations,
        next_cursor,
    })
}

fn validate_envelope(
    envelope: &OperationEnvelopeV1,
    operation_max_bytes: u64,
    snapshot_max_bytes: u64,
) -> Result<(), ApiError> {
    if envelope.space_id.is_nil()
        || envelope.stream_id.is_nil()
        || envelope.client_op_id.is_nil()
        || envelope.author_device_id.is_nil()
    {
        return Err(bad_request("operation envelope ids must be non-nil UUIDs"));
    }
    if envelope.key_epoch == 0 || envelope.key_epoch > i32::MAX as u32 {
        return Err(bad_request(
            "key_epoch must be positive and fit PostgreSQL INTEGER",
        ));
    }
    if envelope.nonce.len() != envelope.cipher_suite.nonce_len() {
        return Err(bad_request("nonce length does not match cipher suite"));
    }
    if envelope.signature.len() != 64 {
        return Err(bad_request("operation signature must be 64 bytes"));
    }
    let max_bytes = match envelope.envelope_kind {
        EnvelopeKind::Snapshot => snapshot_max_bytes,
        EnvelopeKind::Operation | EnvelopeKind::Control => operation_max_bytes,
    };
    if envelope.ciphertext.is_empty() || envelope.ciphertext.len() as u64 > max_bytes {
        return Err(bad_request("operation ciphertext has invalid size"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crypto_core_lib::operation_envelope::{EnvelopeCipherSuite, EnvelopeKind};
    use uuid::Uuid;

    use super::*;
    const TEST_OPERATION_MAX_BYTES: u64 = 1024 * 1024;
    const TEST_SNAPSHOT_MAX_BYTES: u64 = 4 * 1024 * 1024;

    fn validate(envelope: &OperationEnvelopeV1) -> Result<(), ApiError> {
        validate_envelope(envelope, TEST_OPERATION_MAX_BYTES, TEST_SNAPSHOT_MAX_BYTES)
    }

    fn valid_envelope() -> OperationEnvelopeV1 {
        OperationEnvelopeV1 {
            space_id: Uuid::new_v4(),
            stream_id: Uuid::new_v4(),
            client_op_id: Uuid::new_v4(),
            author_device_id: Uuid::new_v4(),
            key_epoch: 1,
            envelope_kind: EnvelopeKind::Operation,
            cipher_suite: EnvelopeCipherSuite::Xchacha20Poly1305,
            nonce: vec![0; 24],
            ciphertext: vec![1],
            signature: vec![2; 64],
        }
    }

    #[test]
    fn rejects_nonce_for_different_cipher_suite() {
        let mut envelope = valid_envelope();
        envelope.nonce.pop();
        assert!(validate(&envelope).is_err());
    }

    #[test]
    fn rejects_oversized_operation() {
        let mut envelope = valid_envelope();
        envelope.ciphertext = vec![0; TEST_OPERATION_MAX_BYTES as usize + 1];
        assert!(validate(&envelope).is_err());
    }

    #[test]
    fn rejects_nil_transport_ids() {
        let mut envelope = valid_envelope();
        envelope.client_op_id = Uuid::nil();
        assert!(validate(&envelope).is_err());
    }

    #[test]
    fn rejects_epoch_outside_database_range() {
        let mut envelope = valid_envelope();
        envelope.key_epoch = i32::MAX as u32 + 1;
        assert!(validate(&envelope).is_err());
    }
}
