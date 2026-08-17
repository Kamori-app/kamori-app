//! DTOs for TOTP enrollment and recovery-code management.

use serde::{Deserialize, Serialize};

/// TOTP status request.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TotpStatusRequest {}

/// TOTP status response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotpStatusResponse {
    /// Whether TOTP feature is enabled in server config.
    pub available: bool,
    /// Whether TOTP is enabled for the user.
    pub enabled: bool,
    /// Number of unused one-time TOTP backup codes.
    pub recovery_codes_remaining: u32,
}

/// TOTP setup-start request.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TotpSetupStartRequest {}

/// TOTP setup-start response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotpSetupStartResponse {
    /// Base32 manual entry key for authenticator apps.
    pub manual_entry_key: String,
    /// `otpauth://` URI for authenticator import.
    pub otpauth_uri: String,
}

/// TOTP setup-finish request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotpSetupFinishRequest {
    /// Base32 manual entry key returned by setup-start.
    pub manual_entry_key: String,
    /// Current TOTP code from authenticator app.
    pub code: String,
}

/// TOTP setup-finish response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotpSetupFinishResponse {
    /// Whether TOTP is enabled after the operation.
    pub enabled: bool,
    /// Newly generated one-time TOTP backup codes.
    pub recovery_codes: Vec<String>,
}

/// TOTP disable request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotpDisableRequest {
    /// Current TOTP code (required when TOTP is enabled).
    pub code: Option<String>,
}

/// TOTP disable response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotpDisableResponse {
    /// Whether TOTP is enabled after the operation.
    pub enabled: bool,
}

/// Account recovery-code regeneration request.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountRecoveryCodesRegenerateRequest {}

/// Account recovery-code regeneration response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountRecoveryCodesRegenerateResponse {
    /// Newly generated one-time TOTP backup codes.
    pub recovery_codes: Vec<String>,
}
