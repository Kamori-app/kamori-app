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

const OPERATION_MAX_BYTES: usize = 1024 * 1024;
const SNAPSHOT_MAX_BYTES: usize = 25 * 1024 * 1024;
const DEFAULT_PAGE_LIMIT: u16 = 200;
const MAX_PAGE_LIMIT: u16 = 1000;

pub(crate) async fn append(
    state: &AppState,
    headers: &HeaderMap,
    envelope: OperationEnvelopeV1,
) -> Result<AppendOperationResponse, ApiError> {
    let user_id = authorize_session(state, headers).await?;
    validate_envelope(&envelope)?;

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

    match repositories::append_operation(&state.pool, user_id, &envelope)
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
    let limit = requested_limit
        .unwrap_or(DEFAULT_PAGE_LIMIT)
        .clamp(1, MAX_PAGE_LIMIT);
    let operations = repositories::list_operations(&state.pool, user_id, space_id, since, limit)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| unauthorized("security-space read access denied"))?;
    let next_cursor = operations.last().map_or(since, |item| item.space_seq);
    Ok(ListOperationsResponse {
        operations,
        next_cursor,
    })
}

fn validate_envelope(envelope: &OperationEnvelopeV1) -> Result<(), ApiError> {
    if envelope.key_epoch == 0 {
        return Err(bad_request("key_epoch must be positive"));
    }
    if envelope.nonce.len() != envelope.cipher_suite.nonce_len() {
        return Err(bad_request("nonce length does not match cipher suite"));
    }
    if envelope.signature.len() != 64 {
        return Err(bad_request("operation signature must be 64 bytes"));
    }
    let max_bytes = match envelope.envelope_kind {
        EnvelopeKind::Snapshot => SNAPSHOT_MAX_BYTES,
        EnvelopeKind::Operation | EnvelopeKind::Control => OPERATION_MAX_BYTES,
    };
    if envelope.ciphertext.is_empty() || envelope.ciphertext.len() > max_bytes {
        return Err(bad_request("operation ciphertext has invalid size"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crypto_core_lib::operation_envelope::{EnvelopeCipherSuite, EnvelopeKind};
    use uuid::Uuid;

    use super::*;

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
        assert!(validate_envelope(&envelope).is_err());
    }

    #[test]
    fn rejects_oversized_operation() {
        let mut envelope = valid_envelope();
        envelope.ciphertext = vec![0; OPERATION_MAX_BYTES + 1];
        assert!(validate_envelope(&envelope).is_err());
    }
}
