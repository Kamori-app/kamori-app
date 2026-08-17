//! Invite-code transport DTOs.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::features::spaces::dto::SpaceRole;

/// Create invite-code request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateInviteCodeRequest {
    /// Independently shared security-space id.
    pub space_id: Uuid,
    /// Role granted when the invite is redeemed. Owner cannot be invited.
    pub role: SpaceRole,
    /// Domain-separated SHA-256 lookup hash of normalized invite code (`A-Z0-9`, 16 chars).
    #[serde(with = "serde_bytes")]
    pub invite_code_hash: Vec<u8>,
    /// Collection key encrypted with invite-code-derived key.
    #[serde(with = "serde_bytes")]
    pub encrypted_key_package: Vec<u8>,
    /// Optional invite note encrypted with invite-code-derived key.
    #[serde(with = "serde_bytes")]
    pub encrypted_note: Option<Vec<u8>>,
    /// Invite lifetime in minutes.
    pub ttl_minutes: u32,
}

/// Create invite-code response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateInviteCodeResponse {
    /// Stored invite record id.
    pub id: Uuid,
}

/// Redeem invite-code request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedeemInviteCodeRequest {
    /// Domain-separated SHA-256 lookup hash of normalized invite code (`A-Z0-9`, 16 chars).
    #[serde(with = "serde_bytes")]
    pub invite_code_hash: Vec<u8>,
}

/// Redeem invite-code response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedeemInviteCodeResponse {
    /// Target security-space id.
    pub space_id: Uuid,
    /// Role granted by this invite.
    pub role: SpaceRole,
    /// Current space key epoch represented by the package.
    pub key_epoch: u32,
    /// Encrypted group key payload bound to the invite code.
    #[serde(with = "serde_bytes")]
    pub encrypted_key_package: Vec<u8>,
    /// Optional encrypted invite note payload.
    #[serde(with = "serde_bytes")]
    pub encrypted_note: Option<Vec<u8>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invite_code_requests_msgpack_roundtrip() {
        let create = CreateInviteCodeRequest {
            space_id: Uuid::new_v4(),
            role: SpaceRole::Editor,
            invite_code_hash: vec![7; 32],
            encrypted_key_package: vec![1, 2, 3],
            encrypted_note: Some(vec![4, 5, 6]),
            ttl_minutes: 60,
        };
        let create_bin = rmp_serde::to_vec_named(&create).expect("msgpack serialize");
        let create_back: CreateInviteCodeRequest =
            rmp_serde::from_slice(&create_bin).expect("msgpack deserialize");
        assert_eq!(create_back.invite_code_hash, vec![7; 32]);
        assert_eq!(create_back.encrypted_note, Some(vec![4, 5, 6]));
        assert_eq!(create_back.ttl_minutes, 60);

        let redeem = RedeemInviteCodeRequest {
            invite_code_hash: vec![9; 32],
        };
        let redeem_bin = rmp_serde::to_vec_named(&redeem).expect("msgpack serialize");
        let redeem_back: RedeemInviteCodeRequest =
            rmp_serde::from_slice(&redeem_bin).expect("msgpack deserialize");
        assert_eq!(redeem_back.invite_code_hash, vec![9; 32]);
    }

    #[test]
    fn invite_role_roundtrips() {
        let request = CreateInviteCodeRequest {
            space_id: Uuid::new_v4(),
            role: SpaceRole::Reader,
            invite_code_hash: vec![7; 32],
            encrypted_key_package: vec![1],
            encrypted_note: None,
            ttl_minutes: 10_080,
        };
        let bytes = rmp_serde::to_vec_named(&request).expect("encode");
        let decoded: CreateInviteCodeRequest = rmp_serde::from_slice(&bytes).expect("decode");
        assert_eq!(decoded.role, SpaceRole::Reader);
    }
}
