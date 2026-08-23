//! Authorization helpers shared by feature handlers and services.

use axum::http::HeaderMap;
use uuid::Uuid;

use crate::{
    platform::jwt::TokenKind, platform::security::auth::bearer_from_headers,
    platform::state::AppState,
};

use super::{ApiError, internal_error, unauthenticated};

/// Authenticated principal extracted from access JWT.
#[derive(Clone, Debug)]
pub struct Principal {
    /// User id from `sub`.
    pub user_id: Uuid,
    /// Username claim.
    pub username: String,
    /// Refresh session to which the access token is cryptographically bound.
    pub session_id: Uuid,
}

async fn authorize_principal_with_device_policy(
    state: &AppState,
    headers: &HeaderMap,
    require_bound_device: bool,
) -> Result<Principal, ApiError> {
    let token = bearer_from_headers(headers).ok_or_else(|| unauthenticated("missing token"))?;
    let claims = state
        .validate_token(&token)
        .map_err(|_| unauthenticated("invalid or expired token"))?;
    if claims.kind != TokenKind::Session {
        return Err(unauthenticated("invalid token"));
    }
    let username = claims
        .username
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| unauthenticated("invalid token"))?;
    let session_id = claims
        .session_id
        .filter(|value| !value.is_nil())
        .ok_or_else(|| unauthenticated("invalid session binding"))?;
    if state.account_state_checks_enabled {
        let active: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM users u
                JOIN refresh_tokens rt
                  ON rt.id = $3 AND rt.user_id = u.id
                 AND rt.revoked_at IS NULL AND rt.expires_at > now()
                LEFT JOIN devices d
                  ON d.id = rt.device_id AND d.user_id = u.id
                WHERE u.id = $1 AND u.username = $2
                  AND u.deleted_at IS NULL AND u.suspended_at IS NULL
                  AND (
                    ($4 AND rt.device_id IS NOT NULL AND d.status = 'active')
                    OR
                    (NOT $4 AND (rt.device_id IS NULL OR d.status = 'active'))
                  )
            )
            "#,
        )
        .bind(claims.user_id)
        .bind(&username)
        .bind(session_id)
        .bind(require_bound_device)
        .fetch_one(&state.pool)
        .await
        .map_err(internal_error)?;
        if !active {
            return Err(unauthenticated("account is unavailable"));
        }
    }
    Ok(Principal {
        user_id: claims.user_id,
        username,
        session_id,
    })
}

/// Validates a device-bound session token for normal authenticated endpoints.
pub async fn authorize_principal(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<Principal, ApiError> {
    authorize_principal_with_device_policy(state, headers, true).await
}

/// Validates a live session during the one-time device enrollment transition.
pub async fn authorize_enrollment_principal(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<Principal, ApiError> {
    authorize_principal_with_device_policy(state, headers, false).await
}

/// Validates a session token from Authorization headers.
pub async fn authorize_session(state: &AppState, headers: &HeaderMap) -> Result<Uuid, ApiError> {
    Ok(authorize_principal(state, headers).await?.user_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{jwt::TokenKind, test_support::test_state};
    use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
    use jsonwebtoken::{EncodingKey, Header};
    use time::OffsetDateTime;

    fn auth_headers(token: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        let value = format!("Bearer {token}");
        headers.insert(AUTHORIZATION, value.parse().expect("authorization header"));
        headers
    }

    #[tokio::test]
    async fn principal_rejects_missing_token() {
        let state = test_state();
        let headers = HeaderMap::new();

        let err = authorize_principal(&state, &headers)
            .await
            .expect_err("must fail");
        assert_eq!(err.0, StatusCode::UNAUTHORIZED);
        assert_eq!(err.1.0.message, "missing token");
        state.pool.close().await;
    }

    #[tokio::test]
    async fn principal_rejects_expired_token_as_unauthorized() {
        let state = test_state();
        let user_id = Uuid::new_v4();
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let claims = crate::platform::jwt::JwtClaims {
            sub: user_id.to_string(),
            kind: TokenKind::Session,
            iss: "kamori".to_string(),
            aud: "kamori-clients".to_string(),
            exp: usize::try_from(now - 120).expect("expired timestamp"),
            iat: usize::try_from(now - 240).expect("issued timestamp"),
            jti: Uuid::new_v4(),
            username: Some("alice".to_string()),
            session_id: Some(Uuid::new_v4()),
        };
        let token = jsonwebtoken::encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(b"test-secret"),
        )
        .expect("expired token");

        let err = authorize_principal(&state, &auth_headers(&token))
            .await
            .expect_err("must fail");
        assert_eq!(err.0, StatusCode::UNAUTHORIZED);
        assert_eq!(err.1.0.message, "invalid or expired token");
        state.pool.close().await;
    }

    #[tokio::test]
    async fn principal_rejects_non_session_kind() {
        let state = test_state();
        let user_id = Uuid::new_v4();
        let token = state
            .issue_account_recovery_token(user_id, "alice")
            .expect("account-recovery token");
        let headers = auth_headers(&token);

        let err = authorize_principal(&state, &headers)
            .await
            .expect_err("must fail");
        assert_eq!(err.0, StatusCode::UNAUTHORIZED);
        assert_eq!(err.1.0.message, "invalid token");
        state.pool.close().await;
    }

    #[tokio::test]
    async fn principal_rejects_missing_username_claim() {
        let state = test_state();
        let user_id = Uuid::new_v4();
        let token = state
            .jwt
            .issue_token(TokenKind::Session, user_id, None, Some(Uuid::new_v4()), 300)
            .expect("session token");
        let headers = auth_headers(&token);

        let err = authorize_principal(&state, &headers)
            .await
            .expect_err("must fail");
        assert_eq!(err.0, StatusCode::UNAUTHORIZED);
        assert_eq!(err.1.0.message, "invalid token");
        state.pool.close().await;
    }

    #[tokio::test]
    async fn principal_rejects_blank_username_claim() {
        let state = test_state();
        let user_id = Uuid::new_v4();
        let token = state
            .jwt
            .issue_token(
                TokenKind::Session,
                user_id,
                Some("   "),
                Some(Uuid::new_v4()),
                300,
            )
            .expect("session token");
        let headers = auth_headers(&token);

        let err = authorize_principal(&state, &headers)
            .await
            .expect_err("must fail");
        assert_eq!(err.0, StatusCode::UNAUTHORIZED);
        assert_eq!(err.1.0.message, "invalid token");
        state.pool.close().await;
    }

    #[tokio::test]
    async fn principal_accepts_valid_session_token() {
        let state = test_state();
        let user_id = Uuid::new_v4();
        let token = state
            .issue_access_token(user_id, "alice", Uuid::new_v4())
            .expect("access token");
        let headers = auth_headers(&token);

        let principal = authorize_principal(&state, &headers)
            .await
            .expect("principal");
        assert_eq!(principal.user_id, user_id);
        assert_eq!(principal.username, "alice");
        state.pool.close().await;
    }

    #[tokio::test]
    async fn authorize_session_returns_user_id() {
        let state = test_state();
        let user_id = Uuid::new_v4();
        let token = state
            .issue_access_token(user_id, "alice", Uuid::new_v4())
            .expect("access token");
        let headers = auth_headers(&token);

        let authorized_user_id = authorize_session(&state, &headers).await.expect("user id");
        assert_eq!(authorized_user_id, user_id);
        state.pool.close().await;
    }
}
