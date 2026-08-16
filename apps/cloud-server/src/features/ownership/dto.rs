//! MessagePack models for ownership transfer offers.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OwnershipResourceKind {
    Workspace,
    SecuritySpace,
}

impl OwnershipResourceKind {
    pub(crate) fn as_db(self) -> &'static str {
        match self {
            Self::Workspace => "workspace",
            Self::SecuritySpace => "security_space",
        }
    }

    pub(crate) fn from_db(value: &str) -> anyhow::Result<Self> {
        match value {
            "workspace" => Ok(Self::Workspace),
            "security_space" => Ok(Self::SecuritySpace),
            _ => anyhow::bail!("unknown ownership resource kind"),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateOwnershipTransferRequest {
    pub resource_kind: OwnershipResourceKind,
    pub resource_id: Uuid,
    pub target_user_id: Uuid,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OwnershipTransferOffer {
    pub transfer_id: Uuid,
    pub resource_kind: OwnershipResourceKind,
    pub resource_id: Uuid,
    pub current_owner_id: Uuid,
    pub current_owner_username: String,
    pub target_user_id: Uuid,
    pub expires_at_unix_ms: i64,
    pub created_at_unix_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateOwnershipTransferResponse {
    pub offer: OwnershipTransferOffer,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ListIncomingOwnershipTransfersResponse {
    pub offers: Vec<OwnershipTransferOffer>,
}

pub type ListOutgoingOwnershipTransfersResponse = ListIncomingOwnershipTransfersResponse;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OwnershipTransferResultResponse {
    pub changed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_request_msgpack_roundtrip() {
        let request = CreateOwnershipTransferRequest {
            resource_kind: OwnershipResourceKind::SecuritySpace,
            resource_id: Uuid::new_v4(),
            target_user_id: Uuid::new_v4(),
        };
        let bytes = rmp_serde::to_vec_named(&request).expect("encode");
        let decoded: CreateOwnershipTransferRequest =
            rmp_serde::from_slice(&bytes).expect("decode");
        assert_eq!(decoded.resource_kind, request.resource_kind);
        assert_eq!(decoded.resource_id, request.resource_id);
        assert_eq!(decoded.target_user_id, request.target_user_id);
    }
}
