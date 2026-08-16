//! DTOs for password change and account recovery flows.

use serde::{Deserialize, Serialize};

use crate::features::spaces::dto::RecoverySpaceKeyPackage;

/// Password change start request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordChangeStartRequest {
    /// OPAQUE client start message bytes for new password registration flow.
    #[serde(with = "serde_bytes")]
    pub opaque_start_request: Vec<u8>,
}

/// Password change start response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordChangeStartResponse {
    /// OPAQUE server response bytes.
    #[serde(with = "serde_bytes")]
    pub opaque_server_message: Vec<u8>,
}

/// Password change finish request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordChangeFinishRequest {
    /// OPAQUE client finish message bytes for new password registration flow.
    #[serde(with = "serde_bytes")]
    pub opaque_finish_request: Vec<u8>,
    /// Existing account master key rewrapped under the new OPAQUE export key.
    #[serde(with = "serde_bytes")]
    pub encrypted_master_key: Vec<u8>,
}

/// Password change finish response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordChangeFinishResponse {
    /// Whether password update was applied.
    pub changed: bool,
}

/// Account recovery start request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountRecoveryStartRequest {
    /// User login name.
    pub username: String,
    /// Domain-separated verifier derived locally from the 24-word data recovery kit.
    #[serde(with = "serde_bytes")]
    pub recovery_verifier: Vec<u8>,
    /// OPAQUE client start message bytes for new password registration flow.
    #[serde(with = "serde_bytes")]
    pub opaque_start_request: Vec<u8>,
}

/// Account recovery start response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountRecoveryStartResponse {
    /// OPAQUE server response bytes.
    #[serde(with = "serde_bytes")]
    pub opaque_server_message: Vec<u8>,
    /// Short-lived token authorizing account-recovery finish.
    pub recovery_token: String,
}

/// Account recovery finish request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountRecoveryFinishRequest {
    /// Token from account-recovery start.
    pub recovery_token: String,
    /// OPAQUE client finish message bytes for new password registration flow.
    #[serde(with = "serde_bytes")]
    pub opaque_finish_request: Vec<u8>,
    /// Account master key recovered from the data kit and wrapped under the new OPAQUE export key.
    #[serde(with = "serde_bytes")]
    pub encrypted_master_key: Vec<u8>,
}

/// Account recovery finish response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountRecoveryFinishResponse {
    /// Whether password update was applied.
    pub changed: bool,
    /// Whether TOTP is disabled after recovery.
    pub totp_disabled: bool,
    /// Current space keys wrapped by the account data-recovery key.
    pub space_key_packages: Vec<RecoverySpaceKeyPackage>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_change_msgpack_roundtrip() {
        let start = PasswordChangeStartRequest {
            opaque_start_request: vec![1, 2, 3, 4],
        };
        let start_bin = rmp_serde::to_vec_named(&start).expect("msgpack serialize");
        let start_back: PasswordChangeStartRequest =
            rmp_serde::from_slice(&start_bin).expect("msgpack deserialize");
        assert_eq!(start_back.opaque_start_request, vec![1, 2, 3, 4]);

        let finish = PasswordChangeFinishRequest {
            opaque_finish_request: vec![9, 8, 7, 6],
            encrypted_master_key: vec![1, 2, 3],
        };
        let finish_bin = rmp_serde::to_vec_named(&finish).expect("msgpack serialize");
        let finish_back: PasswordChangeFinishRequest =
            rmp_serde::from_slice(&finish_bin).expect("msgpack deserialize");
        assert_eq!(finish_back.opaque_finish_request, vec![9, 8, 7, 6]);
        assert_eq!(finish_back.encrypted_master_key, vec![1, 2, 3]);
    }

    #[test]
    fn account_recovery_msgpack_roundtrip() {
        let start = AccountRecoveryStartRequest {
            username: "alice".to_string(),
            recovery_verifier: vec![6; 32],
            opaque_start_request: vec![5, 4, 3],
        };
        let start_bin = rmp_serde::to_vec_named(&start).expect("msgpack serialize");
        let start_back: AccountRecoveryStartRequest =
            rmp_serde::from_slice(&start_bin).expect("msgpack deserialize");
        assert_eq!(start_back.username, "alice");
        assert_eq!(start_back.recovery_verifier, vec![6; 32]);
        assert_eq!(start_back.opaque_start_request, vec![5, 4, 3]);

        let finish = AccountRecoveryFinishRequest {
            recovery_token: "recovery.jwt".to_string(),
            opaque_finish_request: vec![7, 7, 7],
            encrypted_master_key: vec![8, 8, 8],
        };
        let finish_bin = rmp_serde::to_vec_named(&finish).expect("msgpack serialize");
        let finish_back: AccountRecoveryFinishRequest =
            rmp_serde::from_slice(&finish_bin).expect("msgpack deserialize");
        assert_eq!(finish_back.recovery_token, "recovery.jwt");
        assert_eq!(finish_back.opaque_finish_request, vec![7, 7, 7]);
        assert_eq!(finish_back.encrypted_master_key, vec![8, 8, 8]);
    }
}
