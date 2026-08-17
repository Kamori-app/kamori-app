use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct MobileSigninStartRequest {
    pub(super) username: String,
    #[serde(with = "serde_bytes")]
    pub(super) opaque_start_request: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct MobileSigninStartResponse {
    pub(super) opaque_flow_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub(super) opaque_server_message: Vec<u8>,
    pub(super) preauth_token: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct MobileSigninFinishRequest {
    pub(super) username: String,
    pub(super) opaque_flow_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub(super) opaque_finish_request: Vec<u8>,
    pub(super) totp_code: Option<String>,
    pub(super) preauth_token: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct MobileSigninFinishResponse {
    pub(super) access_token: Option<String>,
    pub(super) refresh_token: Option<String>,
    pub(super) totp_verified: bool,
    pub(super) preauth_token: Option<String>,
    #[serde(with = "serde_bytes")]
    pub(super) encrypted_master_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub(super) public_key_bundle: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct MobileCreateInviteCodeRequest {
    pub(super) space_id: Uuid,
    pub(super) role: String,
    #[serde(with = "serde_bytes")]
    pub(super) invite_code_hash: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub(super) encrypted_key_package: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub(super) encrypted_note: Option<Vec<u8>>,
    pub(super) ttl_minutes: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct MobileCreateInviteCodeResponse {
    pub(super) id: Uuid,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct MobileRedeemInviteCodeRequest {
    #[serde(with = "serde_bytes")]
    pub(super) invite_code_hash: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct MobileRedeemInviteCodeResponse {
    pub(super) space_id: Uuid,
    pub(super) role: String,
    pub(super) key_epoch: u32,
    #[serde(with = "serde_bytes")]
    pub(super) encrypted_key_package: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub(super) encrypted_note: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MobileLoginResult {
    pub username: Option<String>,
    pub access_token: Option<String>,
    pub preauth_token: Option<String>,
    pub totp_verified: bool,
    pub account_master_key: Option<[u8; 32]>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MobileDeviceSecrets {
    pub device_id: String,
    pub signing_private_key: [u8; 32],
    pub hpke_private_key: [u8; 32],
    pub hpke_public_key: [u8; 32],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MobileCollection {
    pub collection_id: String,
    pub name: String,
    pub role: String,
    pub key_epoch: u32,
    pub collection_key: [u8; 32],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MobilePimItem {
    pub space_id: String,
    pub resource_id: String,
    pub resource_kind: String,
    pub title: String,
    pub completed: bool,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub starts_at: Option<String>,
    pub ends_at: Option<String>,
    pub conflict: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MobileProvisionResult {
    pub access_token: String,
    pub device: MobileDeviceSecrets,
    pub collections: Vec<MobileCollection>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct MobileRegisterDeviceRequest {
    pub(super) device_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub(super) signing_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub(super) hpke_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub(super) encrypted_name: Vec<u8>,
    pub(super) platform: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct MobileDeviceSummary {
    pub(super) device_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub(super) signing_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub(super) hpke_public_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub(super) encrypted_name: Vec<u8>,
    pub(super) platform: String,
    pub(super) created_at_unix_ms: i64,
    pub(super) last_seen_at_unix_ms: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct MobileRegisterDeviceResponse {
    pub(super) device: MobileDeviceSummary,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct MobileDeviceKeyPackage {
    pub(super) device_id: Uuid,
    pub(super) key_epoch: u32,
    #[serde(with = "serde_bytes")]
    pub(super) encrypted_key_package: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct MobileSpaceSummary {
    pub(super) space_id: Uuid,
    pub(super) workspace_id: Uuid,
    pub(super) role: String,
    pub(super) key_epoch: u32,
    #[serde(with = "serde_bytes")]
    pub(super) encrypted_metadata: Vec<u8>,
    pub(super) device_key_packages: Vec<MobileDeviceKeyPackage>,
    pub(super) created_at_unix_ms: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct MobileListSpacesResponse {
    pub(super) spaces: Vec<MobileSpaceSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct MobileCreateSpaceRequest {
    pub(super) space_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub(super) encrypted_metadata: Vec<u8>,
    pub(super) device_key_packages: Vec<MobileDeviceKeyPackage>,
    #[serde(with = "serde_bytes")]
    pub(super) encrypted_recovery_key_package: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct MobilePutDeviceKeyPackageRequest {
    pub(super) package: MobileDeviceKeyPackage,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct MobilePutRecoveryKeyPackageRequest {
    pub(super) key_epoch: u32,
    #[serde(with = "serde_bytes")]
    pub(super) encrypted_key_package: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct MobileStoredResponse {
    pub(super) stored: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct MobileCreateSpaceResponse {
    pub(super) space: MobileSpaceSummary,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct MobileSpaceLifecycleResponse {
    pub(super) changed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MobileIssuedInviteCode {
    pub code: String,
    pub ttl_minutes: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MobileRedeemedInvite {
    pub collection_id: String,
    pub role: String,
    pub key_epoch: u32,
    pub collection_key: [u8; 32],
}

#[derive(Clone, Debug)]
pub struct MobileSyncConfig {
    pub cloud_base_url: String,
    pub sqlite_path: String,
    pub access_token: String,
    pub sqlite_key: [u8; 32],
}

#[derive(Default)]
pub struct MobileSyncRuntime {
    pub last_config: Option<MobileSyncConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct MobileRefreshRequest {
    pub(super) refresh_token: String,
    pub(super) rotation_request_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct MobileRefreshResponse {
    pub(super) access_token: String,
    pub(super) refresh_token: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct MobileLogoutRequest {
    pub(super) refresh_token: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct MobileLogoutResponse {
    pub(super) revoked: bool,
}
