//! Security-space API models.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpaceRole {
    Owner,
    Editor,
    Reader,
}

impl SpaceRole {
    pub(crate) fn from_db(value: &str) -> anyhow::Result<Self> {
        match value {
            "owner" => Ok(Self::Owner),
            "editor" => Ok(Self::Editor),
            "reader" => Ok(Self::Reader),
            _ => anyhow::bail!("unknown security-space role"),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateSpaceRequest {
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
    pub space_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub encrypted_metadata: Vec<u8>,
    pub device_key_packages: Vec<DeviceKeyPackage>,
    #[serde(with = "serde_bytes")]
    pub encrypted_recovery_key_package: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SpaceSummary {
    pub space_id: Uuid,
    pub workspace_id: Uuid,
    pub role: SpaceRole,
    pub key_epoch: u32,
    #[serde(with = "serde_bytes")]
    pub encrypted_metadata: Vec<u8>,
    pub device_key_packages: Vec<DeviceKeyPackage>,
    pub created_at_unix_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateSpaceResponse {
    pub space: SpaceSummary,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ListSpacesResponse {
    pub spaces: Vec<SpaceSummary>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SpaceMemberSummary {
    pub user_id: Uuid,
    pub username: String,
    pub role: SpaceRole,
    pub key_epoch: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ListSpaceMembersResponse {
    pub members: Vec<SpaceMemberSummary>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SpaceDeviceSummary {
    pub device_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub signing_public_key: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ListSpaceDevicesResponse {
    pub devices: Vec<SpaceDeviceSummary>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DeviceKeyPackage {
    pub device_id: Uuid,
    pub key_epoch: u32,
    #[serde(with = "serde_bytes")]
    pub encrypted_key_package: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PutDeviceKeyPackageRequest {
    pub package: DeviceKeyPackage,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PutDeviceKeyPackageResponse {
    pub stored: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PutRecoveryKeyPackageRequest {
    pub key_epoch: u32,
    #[serde(with = "serde_bytes")]
    pub encrypted_key_package: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PutRecoveryKeyPackageResponse {
    pub stored: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RecoverySpaceKeyPackage {
    pub space_id: Uuid,
    pub key_epoch: u32,
    #[serde(with = "serde_bytes")]
    pub encrypted_key_package: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RevokeSpaceMemberRequest {
    pub expected_key_epoch: u32,
    pub new_key_epoch: u32,
    pub remaining_device_packages: Vec<DeviceKeyPackage>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RevokeSpaceMemberResponse {
    pub revoked: bool,
    pub key_epoch: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SpaceLifecycleResponse {
    pub changed: bool,
}
