//! DTOs for refresh/logout/revoke session endpoints.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Refresh request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshRequest {
    /// Opaque refresh token (body transport mode).
    pub refresh_token: Option<String>,
    /// Idempotency key retained for every retry of this single rotation.
    pub rotation_request_id: Uuid,
}

/// Refresh response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshResponse {
    /// New short-lived access token (JWT).
    pub access_token: String,
    /// Canonical account name used to unlock the matching local encrypted vault.
    pub username: String,
    /// Rotated opaque refresh token (body transport mode only).
    pub refresh_token: Option<String>,
    /// Refresh token row id for session management.
    pub refresh_token_id: Option<Uuid>,
    /// Rotated browser double-submit token (cookie transport only).
    pub csrf_token: Option<String>,
}

/// Browser-only bootstrap response for the host-only CSRF cookie.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CsrfBootstrapResponse {
    /// Double-submit value returned only to an explicitly allowed web origin.
    pub csrf_token: String,
}

/// Logout request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogoutRequest {
    /// Opaque refresh token for current session (body transport mode).
    pub refresh_token: Option<String>,
}

/// Logout response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogoutResponse {
    /// Whether a refresh session row was targeted.
    pub revoked: bool,
}

/// Revoke another session request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevokeSessionRequest {
    /// Target refresh token row id.
    pub refresh_token_id: Uuid,
    /// One-time fresh authentication proof scoped to security settings.
    pub reauth_token: String,
}

/// Revoke another session response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevokeSessionResponse {
    /// Whether a refresh session row was targeted.
    pub revoked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub refresh_token_id: Uuid,
    pub device_id: Option<Uuid>,
    pub is_current: bool,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    pub created_at_unix_ms: i64,
    pub last_used_at_unix_ms: Option<i64>,
    pub expires_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListSessionsResponse {
    pub sessions: Vec<SessionSummary>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_and_revoke_msgpack_roundtrip() {
        let refresh = RefreshRequest {
            refresh_token: Some("opaque".to_string()),
            rotation_request_id: Uuid::new_v4(),
        };
        let refresh_bin = rmp_serde::to_vec_named(&refresh).expect("msgpack serialize");
        let refresh_back: RefreshRequest =
            rmp_serde::from_slice(&refresh_bin).expect("msgpack deserialize");
        assert_eq!(refresh_back.refresh_token, Some("opaque".to_string()));

        let revoke = RevokeSessionRequest {
            refresh_token_id: Uuid::new_v4(),
            reauth_token: "reauth".to_string(),
        };
        let revoke_bin = rmp_serde::to_vec_named(&revoke).expect("msgpack serialize");
        let revoke_back: RevokeSessionRequest =
            rmp_serde::from_slice(&revoke_bin).expect("msgpack deserialize");
        assert_eq!(revoke_back.refresh_token_id, revoke.refresh_token_id);
    }
}
