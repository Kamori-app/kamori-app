use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// OPAQUE signin start response from cloud-server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpaqueSigninStartResponse {
    pub opaque_flow_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub opaque_server_message: Vec<u8>,
    pub next_step: String,
    pub preauth_token: Option<String>,
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
    pub preauth_token: Option<String>,
}

/// Passkey login start response from cloud-server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasskeyLoginStartResponse {
    pub flow_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub challenge: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub public_key_credential_request_options: Vec<u8>,
}

/// Passkey login finish response from cloud-server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasskeyLoginFinishResponse {
    pub username: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub refresh_token_id: Option<Uuid>,
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
