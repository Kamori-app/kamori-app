//! DTOs for passkey registration, login and management endpoints.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Passkey add start request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasskeyAddStartRequest {
    pub reauth_token: String,
}

/// Passkey add start response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasskeyAddStartResponse {
    /// Registration-flow identifier for finish step.
    pub flow_id: Uuid,
    /// Raw challenge bytes.
    #[serde(with = "serde_bytes")]
    pub challenge: Vec<u8>,
    /// Serialized PublicKeyCredentialCreationOptions.
    #[serde(with = "serde_bytes")]
    pub public_key_credential_creation_options: Vec<u8>,
}

/// Passkey add finish request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasskeyAddFinishRequest {
    /// Registration-flow identifier from add-start.
    pub flow_id: Uuid,
    /// Serialized registration credential response.
    #[serde(with = "serde_bytes")]
    pub credential: Vec<u8>,
    /// Client-encrypted passkey label.
    #[serde(with = "serde_bytes")]
    pub encrypted_name: Vec<u8>,
}

/// Stored passkey metadata returned by management endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasskeyMetadata {
    /// Stored passkey row id.
    pub id: Uuid,
    /// WebAuthn credential id.
    #[serde(with = "serde_bytes")]
    pub credential_id: Vec<u8>,
    /// Client-encrypted passkey label.
    #[serde(with = "serde_bytes")]
    pub encrypted_name: Vec<u8>,
}

/// Passkey add finish response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasskeyAddFinishResponse {
    /// Created or updated passkey metadata.
    pub passkey: PasskeyMetadata,
}

/// List passkeys response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasskeyListResponse {
    /// User passkeys.
    pub passkeys: Vec<PasskeyMetadata>,
}

/// Passkey update request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasskeyUpdateRequest {
    /// Passkey row id.
    pub passkey_id: Uuid,
    /// Client-encrypted passkey label.
    #[serde(with = "serde_bytes")]
    pub encrypted_name: Vec<u8>,
}

/// Passkey update response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasskeyUpdateResponse {
    /// Updated passkey metadata.
    pub passkey: PasskeyMetadata,
}

/// Passkey delete request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasskeyDeleteRequest {
    /// Passkey row id.
    pub passkey_id: Uuid,
    pub reauth_token: String,
}

/// Passkey delete response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasskeyDeleteResponse {
    /// Deletion status.
    pub deleted: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passkey_management_msgpack_roundtrip() {
        let update = PasskeyUpdateRequest {
            passkey_id: Uuid::new_v4(),
            encrypted_name: vec![1, 2, 3, 4],
        };
        let update_bin = rmp_serde::to_vec_named(&update).expect("msgpack serialize");
        let update_back: PasskeyUpdateRequest =
            rmp_serde::from_slice(&update_bin).expect("msgpack deserialize");
        assert_eq!(update_back.encrypted_name, vec![1, 2, 3, 4]);

        let list = PasskeyListResponse {
            passkeys: vec![PasskeyMetadata {
                id: Uuid::new_v4(),
                credential_id: vec![9, 8, 7],
                encrypted_name: vec![6, 5, 4],
            }],
        };
        let list_bin = rmp_serde::to_vec_named(&list).expect("msgpack serialize");
        let list_back: PasskeyListResponse =
            rmp_serde::from_slice(&list_bin).expect("msgpack deserialize");
        assert_eq!(list_back.passkeys.len(), 1);
        assert_eq!(list_back.passkeys[0].credential_id, vec![9, 8, 7]);
        assert_eq!(list_back.passkeys[0].encrypted_name, vec![6, 5, 4]);
    }
}
