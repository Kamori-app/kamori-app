//! DTOs for password and passkey sign-in flows.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// OPAQUE signin start request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigninStartRequest {
    /// User login name.
    pub username: String,
    /// OPAQUE client start message bytes.
    #[serde(with = "serde_bytes")]
    pub opaque_start_request: Vec<u8>,
}

/// Next step for the signin flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SigninNextStep {
    /// Proceed to finish without TOTP.
    Continue,
    /// Require TOTP before issuing an access token.
    TotpRequired,
}

/// OPAQUE signin start response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigninStartResponse {
    /// Random server-side state handle isolating concurrent login attempts.
    pub opaque_flow_id: Uuid,
    /// OPAQUE server response bytes.
    #[serde(with = "serde_bytes")]
    pub opaque_server_message: Vec<u8>,
    /// Flow control hint for TOTP.
    pub next_step: SigninNextStep,
    /// Pre-auth token if TOTP is required.
    pub preauth_token: Option<String>,
}

/// OPAQUE signin finish request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigninFinishRequest {
    /// User login name.
    pub username: String,
    /// Flow handle returned by signin start.
    pub opaque_flow_id: Uuid,
    /// OPAQUE client finish message bytes.
    #[serde(with = "serde_bytes")]
    pub opaque_finish_request: Vec<u8>,
    /// TOTP code if required.
    pub totp_code: Option<String>,
    /// Pre-auth token if TOTP is required.
    pub preauth_token: Option<String>,
}

/// OPAQUE signin finish response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigninFinishResponse {
    /// Access token (JWT) when login succeeds.
    pub access_token: Option<String>,
    /// Refresh token when login succeeds.
    pub refresh_token: Option<String>,
    /// Refresh token row id for session management.
    pub refresh_token_id: Option<Uuid>,
    /// Indicates whether TOTP was verified.
    pub totp_verified: bool,
    /// Encrypted user master key.
    #[serde(with = "serde_bytes")]
    pub encrypted_master_key: Vec<u8>,
    /// Public key bundle for sharing.
    #[serde(with = "serde_bytes")]
    pub public_key_bundle: Vec<u8>,
    /// Pre-auth token when TOTP is still pending.
    pub preauth_token: Option<String>,
}

/// Authenticated OPAQUE reauthentication start request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReauthStartRequest {
    #[serde(with = "serde_bytes")]
    pub opaque_start_request: Vec<u8>,
}

/// Authenticated OPAQUE reauthentication start response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReauthStartResponse {
    pub opaque_flow_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub opaque_server_message: Vec<u8>,
    pub totp_required: bool,
}

/// Authenticated OPAQUE reauthentication finish request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReauthFinishRequest {
    pub opaque_flow_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub opaque_finish_request: Vec<u8>,
    pub totp_code: Option<String>,
}

/// Very short-lived proof used only by destructive endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReauthFinishResponse {
    pub reauth_token: String,
}

/// Passkey login start request.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PasskeyLoginStartRequest {}

/// Passkey login start response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasskeyLoginStartResponse {
    /// Discoverable-flow identifier for finish step.
    pub flow_id: Uuid,
    /// Raw challenge bytes.
    #[serde(with = "serde_bytes")]
    pub challenge: Vec<u8>,
    /// Serialized PublicKeyCredentialRequestOptions.
    #[serde(with = "serde_bytes")]
    pub public_key_credential_request_options: Vec<u8>,
}

/// Passkey login finish request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasskeyLoginFinishRequest {
    /// Discoverable-flow identifier from login start.
    pub flow_id: Uuid,
    /// Serialized credential response.
    #[serde(with = "serde_bytes")]
    pub credential: Vec<u8>,
}

/// Passkey login finish response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasskeyLoginFinishResponse {
    /// Canonical username resolved from the discoverable credential.
    pub username: String,
    /// Access token (JWT) on success.
    pub access_token: String,
    /// Refresh token on success (body transport mode only).
    pub refresh_token: Option<String>,
    /// Refresh token row id for session management.
    pub refresh_token_id: Option<Uuid>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signin_finish_response_roundtrip() {
        let resp = SigninFinishResponse {
            access_token: Some("token".to_string()),
            refresh_token: Some("refresh".to_string()),
            refresh_token_id: Some(Uuid::new_v4()),
            totp_verified: true,
            encrypted_master_key: vec![9, 9],
            public_key_bundle: vec![8, 8],
            preauth_token: None,
        };
        let json = serde_json::to_string(&resp).expect("serialize");
        let back: SigninFinishResponse = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.access_token, Some("token".to_string()));
        assert_eq!(back.refresh_token, Some("refresh".to_string()));
        assert!(back.refresh_token_id.is_some());
        assert!(back.totp_verified);
        assert_eq!(back.encrypted_master_key, vec![9, 9]);
        assert_eq!(back.public_key_bundle, vec![8, 8]);
    }
}
