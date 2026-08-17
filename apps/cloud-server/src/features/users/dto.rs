//! User lifecycle transport DTOs.

use serde::{Deserialize, Serialize};

/// Account deletion request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteMeRequest {
    /// Five-minute proof from `/auth/reauth/finish`.
    pub reauth_token: String,
    /// Must exactly equal `DELETE <username>`.
    pub confirmation: String,
}

/// Account deletion response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteMeResponse {
    /// Whether deletion was accepted.
    pub deleted: bool,
}

/// Ownership blockers that must be resolved before account deletion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletionStatusResponse {
    pub can_delete: bool,
    pub shared_workspaces_owned: u64,
    pub shared_spaces_owned: u64,
}

/// Independent, revocable user consent choices. Every category defaults to false.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConsentSettings {
    pub product_analytics: bool,
    pub crash_reports: bool,
    pub marketing: bool,
    pub policy_version: u32,
    pub updated_at_unix_ms: Option<i64>,
}

/// Explicit replacement of all current consent choices.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateConsentSettingsRequest {
    pub product_analytics: bool,
    pub crash_reports: bool,
    pub marketing: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consent_settings_msgpack_roundtrip_keeps_independent_categories() {
        let request = UpdateConsentSettingsRequest {
            product_analytics: true,
            crash_reports: false,
            marketing: true,
        };
        let encoded = rmp_serde::to_vec_named(&request).expect("encode");
        let decoded: UpdateConsentSettingsRequest =
            rmp_serde::from_slice(&encoded).expect("decode");
        assert_eq!(decoded, request);
    }
}
