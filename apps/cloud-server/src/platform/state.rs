//! Shared server state and Valkey-backed ephemeral stores.

use crate::platform::config::Config;
use crate::platform::db::assert_postgres_url;
use crate::platform::jwt::{JwtManager, TokenKind};
use crate::platform::metrics::HttpMetrics;
use crate::platform::object_storage::build_object_store;
use crate::platform::security::opaque::OpaqueServer;
use crate::platform::security::passkey::PasskeyService;
use crate::platform::state_store::{StateStore, ValkeyConfig, ValkeyStore};
use anyhow::{Result, anyhow};
use object_store::ObjectStore;
use sqlx::PgPool;
use std::sync::Arc;
use time::Duration;
use uuid::Uuid;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    /// Postgres connection pool.
    pub pool: PgPool,
    /// Runtime configuration.
    pub config: Config,
    /// JWT manager for access and preauth tokens.
    pub jwt: JwtManager,
    /// Valkey-backed store for OPAQUE and passkey states.
    pub state_store: Arc<dyn StateStore>,
    /// Primary S3-compatible ciphertext store.
    pub object_store: Arc<dyn ObjectStore>,
    /// Low-cardinality process metrics with no user or content labels.
    pub http_metrics: Arc<HttpMetrics>,
    /// OPAQUE server instance.
    pub opaque: Arc<OpaqueServer>,
    /// Passkey service instance.
    pub passkeys: Arc<PasskeyService>,
    /// Security-key verifier bound to the exact operator-console origin.
    pub admin_passkeys: Arc<PasskeyService>,
    /// Whether every access token is checked against authoritative account state.
    pub account_state_checks_enabled: bool,
}

/// Decoded and validated JWT claims used by feature-level auth helpers.
#[derive(Clone, Debug)]
pub struct ValidatedToken {
    /// Actor user id from `sub`.
    pub user_id: Uuid,
    /// Token kind discriminator.
    pub kind: TokenKind,
    /// Optional username snapshot from claims.
    pub username: Option<String>,
}

impl AppState {
    /// Builds a new AppState.
    pub fn new(pool: PgPool, config: Config, state_store: Arc<dyn StateStore>) -> Result<Self> {
        let jwt = JwtManager::new(&config)?;
        let opaque = Arc::new(OpaqueServer::new(state_store.clone())?);
        let passkeys = Arc::new(PasskeyService::new(&config, state_store.clone())?);
        let admin_passkeys = Arc::new(PasskeyService::new_admin(&config, state_store.clone())?);
        let object_store = build_object_store(&config)?;
        let http_metrics = Arc::new(HttpMetrics::default());

        Ok(Self {
            pool,
            config,
            jwt,
            state_store,
            object_store,
            http_metrics,
            opaque,
            passkeys,
            admin_passkeys,
            account_state_checks_enabled: true,
        })
    }

    /// Issues a short-lived preauth JWT.
    pub fn issue_preauth_token(&self, user_id: Uuid, username: &str) -> Result<String> {
        self.jwt.issue_token(
            TokenKind::PreAuth,
            user_id,
            Some(username),
            self.config.jwt_preauth_ttl_seconds,
        )
    }

    /// Issues a short-lived account-recovery JWT.
    pub fn issue_account_recovery_token(&self, user_id: Uuid, username: &str) -> Result<String> {
        self.jwt.issue_token(
            TokenKind::AccountRecovery,
            user_id,
            Some(username),
            self.config.jwt_account_recovery_ttl_seconds,
        )
    }

    /// Issues an access JWT.
    pub fn issue_access_token(&self, user_id: Uuid, username: &str) -> Result<String> {
        self.jwt.issue_token(
            TokenKind::Session,
            user_id,
            Some(username),
            self.config.access_token_ttl_seconds,
        )
    }

    /// Issues a five-minute token proving a fresh OPAQUE reauthentication.
    pub fn issue_reauth_token(&self, user_id: Uuid, username: &str) -> Result<String> {
        self.jwt
            .issue_token(TokenKind::Reauth, user_id, Some(username), 5 * 60)
    }

    /// Returns refresh token TTL from config.
    pub fn refresh_token_ttl(&self) -> Duration {
        Duration::seconds(self.config.refresh_token_ttl_seconds)
    }

    /// Validates a JWT and returns the user id and token kind.
    pub fn validate_token(&self, token: &str) -> Result<ValidatedToken> {
        let claims = self.jwt.validate_token(token)?;
        let user_id = Uuid::parse_str(&claims.sub).map_err(|e| anyhow!("invalid sub: {e}"))?;
        Ok(ValidatedToken {
            user_id,
            kind: claims.kind,
            username: claims.username,
        })
    }
}

/// Loads configuration, DB pool, and Valkey store.
pub async fn load_state(database_url: &str) -> Result<AppState> {
    let config = Config::load()?;
    assert_postgres_url(&config.database_url)?;

    let pool =
        crate::platform::db::create_pool(database_url, config.database_max_connections).await?;

    let valkey = ValkeyStore::new(ValkeyConfig::new(
        config.valkey_url.clone(),
        config.valkey_key_prefix.clone(),
        std::time::Duration::from_secs(config.valkey_ttl_seconds),
    ))?;

    AppState::new(pool, config, Arc::new(valkey))
}
