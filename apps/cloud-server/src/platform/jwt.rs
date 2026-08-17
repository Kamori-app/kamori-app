//! JWT utilities for access, preauth, and account-recovery tokens.

use crate::platform::config::Config;
use anyhow::{Result, anyhow};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

/// Token type used for auth flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenKind {
    /// Pre-auth token used for TOTP.
    PreAuth,
    /// Session token used for authenticated requests.
    Session,
    /// Account-recovery token used for password reset finish.
    AccountRecovery,
    /// Very short-lived proof of recent password and second-factor verification.
    Reauth,
}

/// JWT claims used by the server.
#[derive(Debug, Serialize, Deserialize)]
pub struct JwtClaims {
    /// Subject (user id as string).
    pub sub: String,
    /// Token type.
    pub kind: TokenKind,
    /// Issuer.
    pub iss: String,
    /// Audience.
    pub aud: String,
    /// Expiration time (unix seconds).
    pub exp: usize,
    /// Issued-at time (unix seconds).
    pub iat: usize,
    /// Optional username snapshot embedded into token for hot-path auth flows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
}

/// JWT encoder/decoder for server tokens.
#[derive(Clone)]
pub struct JwtManager {
    encoding: EncodingKey,
    decoding: DecodingKey,
    issuer: String,
    audience: String,
}

impl JwtManager {
    /// Builds a JWT manager from config.
    pub fn new(config: &Config) -> Result<Self> {
        let secret = config.jwt_secret.as_bytes();
        Ok(Self {
            encoding: EncodingKey::from_secret(secret),
            decoding: DecodingKey::from_secret(secret),
            issuer: config.jwt_issuer.clone(),
            audience: config.jwt_audience.clone(),
        })
    }

    /// Issues a JWT for a user with a TTL.
    pub fn issue_token(
        &self,
        kind: TokenKind,
        user_id: Uuid,
        username: Option<&str>,
        ttl_seconds: i64,
    ) -> Result<String> {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let exp = (now + ttl_seconds).max(now + 1) as usize;
        let claims = JwtClaims {
            sub: user_id.to_string(),
            kind,
            iss: self.issuer.clone(),
            aud: self.audience.clone(),
            iat: now as usize,
            exp,
            username: username.map(ToOwned::to_owned),
        };

        let token = jsonwebtoken::encode(&Header::default(), &claims, &self.encoding)
            .map_err(|e| anyhow!("jwt encode: {e}"))?;
        Ok(token)
    }

    /// Validates a JWT and returns its claims.
    pub fn validate_token(&self, token: &str) -> Result<JwtClaims> {
        let mut validation = Validation::new(jsonwebtoken::Algorithm::HS256);
        validation.set_issuer(std::slice::from_ref(&self.issuer));
        validation.set_audience(std::slice::from_ref(&self.audience));

        let data = jsonwebtoken::decode::<JwtClaims>(token, &self.decoding, &validation)
            .map_err(|e| anyhow!("jwt decode: {e}"))?;
        Ok(data.claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jwt_roundtrip() {
        let mut cfg = Config::load().expect("config");
        cfg.jwt_secret = "secret".to_string();
        cfg.jwt_issuer = "issuer".to_string();
        cfg.jwt_audience = "aud".to_string();

        let jwt = JwtManager::new(&cfg).expect("jwt");
        let user_id = Uuid::new_v4();
        let token = jwt
            .issue_token(TokenKind::Session, user_id, Some("alice"), 60)
            .expect("issue");

        let claims = jwt.validate_token(&token).expect("validate");
        assert_eq!(claims.sub, user_id.to_string());
        assert_eq!(claims.kind, TokenKind::Session);
        assert_eq!(claims.iss, "issuer");
        assert_eq!(claims.aud, "aud");
        assert_eq!(claims.username.as_deref(), Some("alice"));
    }

    #[test]
    fn jwt_account_recovery_roundtrip() {
        let mut cfg = Config::load().expect("config");
        cfg.jwt_secret = "secret".to_string();
        cfg.jwt_issuer = "issuer".to_string();
        cfg.jwt_audience = "aud".to_string();

        let jwt = JwtManager::new(&cfg).expect("jwt");
        let user_id = Uuid::new_v4();
        let token = jwt
            .issue_token(TokenKind::AccountRecovery, user_id, Some("alice"), 120)
            .expect("issue");

        let claims = jwt.validate_token(&token).expect("validate");
        assert_eq!(claims.sub, user_id.to_string());
        assert_eq!(claims.kind, TokenKind::AccountRecovery);
        assert_eq!(claims.iss, "issuer");
        assert_eq!(claims.aud, "aud");
        assert_eq!(claims.username.as_deref(), Some("alice"));
    }

    #[test]
    fn jwt_without_username_remains_valid() {
        let mut cfg = Config::load().expect("config");
        cfg.jwt_secret = "secret".to_string();
        cfg.jwt_issuer = "issuer".to_string();
        cfg.jwt_audience = "aud".to_string();

        let jwt = JwtManager::new(&cfg).expect("jwt");
        let user_id = Uuid::new_v4();
        let token = jwt
            .issue_token(TokenKind::Session, user_id, None, 60)
            .expect("issue");

        let claims = jwt.validate_token(&token).expect("validate");
        assert_eq!(claims.username, None);
    }
}
