//! Operation-log API models.

use crypto_core_lib::operation_envelope::OperationEnvelopeV1;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AppendOperationRequest {
    pub envelope: OperationEnvelopeV1,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AppendOperationResponse {
    pub accepted: bool,
    pub duplicate: bool,
    pub space_seq: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StoredOperation {
    pub space_seq: u64,
    pub received_at_unix_ms: i64,
    pub envelope: OperationEnvelopeV1,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ListOperationsResponse {
    pub operations: Vec<StoredOperation>,
    pub next_cursor: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ListOperationsQuery {
    pub space_id: Uuid,
    #[serde(default)]
    pub since: u64,
    pub limit: Option<u16>,
}
