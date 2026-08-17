//! DTOs for OPAQUE sign-up transport.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// OPAQUE signup start request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignupStartRequest {
    /// User login name.
    pub username: String,
    /// OPAQUE client start message bytes.
    #[serde(with = "serde_bytes")]
    pub opaque_start_request: Vec<u8>,
}

/// OPAQUE signup start response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignupStartResponse {
    /// OPAQUE server response bytes.
    #[serde(with = "serde_bytes")]
    pub opaque_server_message: Vec<u8>,
}

/// OPAQUE signup finish request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignupFinishRequest {
    /// User login name.
    pub username: String,
    /// OPAQUE client finish message bytes.
    #[serde(with = "serde_bytes")]
    pub opaque_finish_request: Vec<u8>,
    /// Encrypted user master key.
    #[serde(with = "serde_bytes")]
    pub encrypted_master_key: Vec<u8>,
    /// Public key bundle for sharing.
    #[serde(with = "serde_bytes")]
    pub public_key_bundle: Vec<u8>,
    /// Domain-separated verifier derived from the 24-word data recovery kit.
    #[serde(with = "serde_bytes")]
    pub recovery_verifier: Vec<u8>,
}

/// OPAQUE signup finish response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignupFinishResponse {
    /// Assigned user id.
    pub user_id: Uuid,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signup_start_request_roundtrip() {
        let req = SignupStartRequest {
            username: "alice".to_string(),
            opaque_start_request: vec![1, 2, 3],
        };
        let json = serde_json::to_string(&req).expect("serialize");
        let back: SignupStartRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.username, "alice");
        assert_eq!(back.opaque_start_request, vec![1, 2, 3]);
    }
}
