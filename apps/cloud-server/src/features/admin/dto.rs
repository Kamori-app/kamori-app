//! MessagePack contracts for the operator control plane.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdminSecurityKeyRegistrationStartRequest {
    pub username: String,
    pub bootstrap_token: String,
    pub totp_code: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdminSecurityKeyRegistrationFinishRequest {
    pub username: String,
    pub bootstrap_token: String,
    pub totp_code: String,
    pub flow_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub credential: Vec<u8>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdminSecurityKeyRegistrationResponse {
    pub flow_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub public_key_credential_creation_options: Vec<u8>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdminSecurityKeyAddFinishRequest {
    pub flow_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub credential: Vec<u8>,
    pub name: String,
    pub reauth_token: String,
    pub reason: String,
    pub confirmation: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdminSecurityKeySummary {
    pub id: Uuid,
    pub name: String,
    pub created_at_unix_ms: i64,
    pub last_used_at_unix_ms: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdminSecurityKeyRemoveRequest {
    pub key_id: Uuid,
    pub reauth_token: String,
    pub reason: String,
    pub confirmation: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdminAuthStartRequest {
    pub username: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdminAuthStartResponse {
    pub flow_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub public_key_credential_request_options: Vec<u8>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdminAuthFinishRequest {
    pub username: String,
    pub flow_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub credential: Vec<u8>,
    pub totp_code: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdminAuthFinishResponse {
    pub token: String,
    pub expires_at_unix_ms: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdminDashboardResponse {
    pub active_accounts: u64,
    pub suspended_accounts: u64,
    pub total_blob_storage_bytes: u64,
    pub pending_blobs: u64,
    pub pending_object_deletions: u64,
    pub registration_enabled: bool,
    pub beta_account_limit: u64,
    pub latest_migration: Option<String>,
    pub jobs: Vec<OperatorJobStatus>,
    pub security_keys: Vec<AdminSecurityKeySummary>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OperatorJobStatus {
    pub job_name: String,
    pub status: String,
    pub details: Value,
    pub updated_at_unix_ms: i64,
    pub last_succeeded_at_unix_ms: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RuntimeSetting {
    pub key: String,
    pub value: Value,
    pub version: u64,
    pub updated_at_unix_ms: Option<i64>,
    pub overridden: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RuntimeSettingsResponse {
    pub settings: Vec<RuntimeSetting>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateRuntimeSettingRequest {
    pub key: String,
    pub value: Value,
    pub expected_version: u64,
    pub reauth_token: String,
    pub reason: String,
    pub confirmation: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SuspendAccountRequest {
    pub user_id: Uuid,
    pub suspended: bool,
    pub reauth_token: String,
    pub reason: String,
    pub confirmation: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdminMutationResponse {
    pub changed: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdminAuditEntry {
    pub id: Uuid,
    pub actor_username: Option<String>,
    pub event_kind: String,
    pub target_kind: Option<String>,
    pub target_id: Option<String>,
    pub reason: Option<String>,
    pub details: Value,
    pub created_at_unix_ms: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdminAuditResponse {
    pub entries: Vec<AdminAuditEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dangerous_setting_request_roundtrips() {
        let request = UpdateRuntimeSettingRequest {
            key: "registration_enabled".to_string(),
            value: Value::Bool(false),
            expected_version: 1,
            reauth_token: "proof".to_string(),
            reason: "incident".to_string(),
            confirmation: "SET registration_enabled".to_string(),
        };
        let encoded = rmp_serde::to_vec_named(&request).expect("encode");
        let decoded: UpdateRuntimeSettingRequest = rmp_serde::from_slice(&encoded).expect("decode");
        assert_eq!(decoded.key, request.key);
        assert_eq!(decoded.value, request.value);
    }
}
