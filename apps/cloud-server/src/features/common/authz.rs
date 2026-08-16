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
}

/// Validates a session token from Authorization headers and returns principal context.
pub async fn authorize_principal(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<Principal, ApiError> {
    let token = bearer_from_headers(headers).ok_or_else(|| unauthenticated("missing token"))?;
    let claims = state.validate_token(&token).map_err(internal_error)?;
    if claims.kind != TokenKind::Session {
        return Err(unauthenticated("invalid token"));
    }
    let username = claims
        .username
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| unauthenticated("invalid token"))?;
    if state.account_state_checks_enabled {
        let active: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM users
                WHERE id = $1 AND username = $2
                  AND deleted_at IS NULL AND suspended_at IS NULL
            )
            "#,
        )
        .bind(claims.user_id)
        .bind(&username)
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
    })
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
        assert_eq!(err.1.0.error, "missing token");
        state.pool.close().await;
    }

    #[tokio::test]
    async fn principal_rejects_non_session_kind() {
        let state = test_state();
        let user_id = Uuid::new_v4();
        let token = state
            .issue_preauth_token(user_id, "alice")
            .expect("preauth token");
        let headers = auth_headers(&token);

        let err = authorize_principal(&state, &headers)
            .await
            .expect_err("must fail");
        assert_eq!(err.0, StatusCode::UNAUTHORIZED);
        assert_eq!(err.1.0.error, "invalid token");
        state.pool.close().await;
    }

    #[tokio::test]
    async fn principal_rejects_missing_username_claim() {
        let state = test_state();
        let user_id = Uuid::new_v4();
        let token = state
            .jwt
            .issue_token(TokenKind::Session, user_id, None, 300)
            .expect("session token");
        let headers = auth_headers(&token);

        let err = authorize_principal(&state, &headers)
            .await
            .expect_err("must fail");
        assert_eq!(err.0, StatusCode::UNAUTHORIZED);
        assert_eq!(err.1.0.error, "invalid token");
        state.pool.close().await;
    }

    #[tokio::test]
    async fn principal_rejects_blank_username_claim() {
        let state = test_state();
        let user_id = Uuid::new_v4();
        let token = state
            .jwt
            .issue_token(TokenKind::Session, user_id, Some("   "), 300)
            .expect("session token");
        let headers = auth_headers(&token);

        let err = authorize_principal(&state, &headers)
            .await
            .expect_err("must fail");
        assert_eq!(err.0, StatusCode::UNAUTHORIZED);
        assert_eq!(err.1.0.error, "invalid token");
        state.pool.close().await;
    }

    #[tokio::test]
    async fn principal_accepts_valid_session_token() {
        let state = test_state();
        let user_id = Uuid::new_v4();
        let token = state
            .issue_access_token(user_id, "alice")
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
            .issue_access_token(user_id, "alice")
            .expect("access token");
        let headers = auth_headers(&token);

        let authorized_user_id = authorize_session(&state, &headers).await.expect("user id");
        assert_eq!(authorized_user_id, user_id);
        state.pool.close().await;
    }
}
