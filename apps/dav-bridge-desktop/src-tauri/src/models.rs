use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// OPAQUE signin start response from cloud-server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpaqueSigninStartResponse {
    pub opaque_flow_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub opaque_server_message: Vec<u8>,
    pub next_step: String,
}

/// OPAQUE signin finish response from cloud-server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpaqueSigninFinishResponse {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub refresh_token_id: Option<Uuid>,
    pub totp_verified: bool,
    #[serde(with = "serde_bytes")]
    pub encrypted_master_key: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub public_key_bundle: Vec<u8>,
    pub totp_continuation_token: Option<String>,
    pub device_enrollment_token: Option<String>,
}

/// External-browser device authorization response from cloud-server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserLoginStartResponse {
    pub flow_id: Uuid,
    pub device_secret: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in_seconds: u64,
    pub poll_interval_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserLoginPollResponse {
    pub status: String,
    pub username: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub refresh_token_id: Option<Uuid>,
    pub device_enrollment_token: Option<String>,
    #[serde(with = "serde_bytes")]
    pub encrypted_master_key_package: Vec<u8>,
}

/// Local server runtime status for dashboard UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalServerStatus {
    pub running: bool,
    pub bind_addr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DavCollectionEndpoint {
    pub collection_id: String,
    pub name: String,
    pub calendar_url: String,
    pub address_book_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DavConnectionInfo {
    pub bind_addr: String,
    pub username: String,
    pub password: String,
    pub collections: Vec<DavCollectionEndpoint>,
}

/// Collection summary surfaced to frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionSummary {
    pub id: String,
    pub name: String,
    pub synced_items: u64,
}

/// Combined snapshot for dashboard cards.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardSnapshot {
    pub has_access_token: bool,
    pub server: LocalServerStatus,
    pub collections_total: usize,
    pub synced_items_total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogoutResult {
    pub server_session_revoked: bool,
    pub warning: Option<String>,
}
