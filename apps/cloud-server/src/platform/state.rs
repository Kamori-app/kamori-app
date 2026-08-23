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
use tokio::sync::Semaphore;
use uuid::Uuid;

const PROCESS_BLOB_DOWNLOAD_SAFETY_CAP: usize = 4096;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    /// Postgres connection pool.
    pub pool: PgPool,
    /// Runtime configuration.
    pub config: Config,
    /// JWT manager for access and scoped proof tokens.
    pub jwt: JwtManager,
    /// Valkey-backed store for OPAQUE and passkey states.
    pub state_store: Arc<dyn StateStore>,
    /// Primary S3-compatible ciphertext store.
    pub object_store: Arc<dyn ObjectStore>,
    /// Low-cardinality process metrics with no user or content labels.
    pub http_metrics: Arc<HttpMetrics>,
    /// Process-local memory/backpressure guard for streamed blob delivery.
    pub blob_download_semaphore: Arc<Semaphore>,
    /// Process-local concurrency cap for large authenticated request bodies.
    pub large_request_semaphore: Arc<Semaphore>,
    /// Aggregate request-body budget measured in 64 KiB permits.
    pub request_byte_semaphore: Arc<Semaphore>,
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
    /// Refresh-session id bound to a session access token.
    pub session_id: Option<Uuid>,
}

impl AppState {
    /// Builds a new AppState.
    pub fn new(pool: PgPool, config: Config, state_store: Arc<dyn StateStore>) -> Result<Self> {
        let jwt = JwtManager::new(&config)?;
        let opaque = Arc::new(OpaqueServer::load(
            state_store.clone(),
            config.opaque_server_setup_path.as_deref(),
            config.allow_ephemeral_opaque_setup,
        )?);
        let passkeys = Arc::new(PasskeyService::new(&config, state_store.clone())?);
        let admin_passkeys = Arc::new(PasskeyService::new_admin(&config, state_store.clone())?);
        let object_store = build_object_store(&config)?;
        let http_metrics = Arc::new(HttpMetrics::default());
        // PostgreSQL enforces the runtime-configurable cross-node limit. This
        // semaphore is a fixed process memory/backpressure ceiling so an
        // operator reduction takes effect without rebuilding application state.
        let blob_download_semaphore = Arc::new(Semaphore::new(PROCESS_BLOB_DOWNLOAD_SAFETY_CAP));
        let large_request_semaphore = Arc::new(Semaphore::new(usize::try_from(
            config.max_concurrent_large_requests,
        )?));
        let request_byte_semaphore = Arc::new(Semaphore::new(usize::try_from(
            config.max_inflight_request_bytes.div_ceil(64 * 1024),
        )?));

        Ok(Self {
            pool,
            config,
            jwt,
            state_store,
            object_store,
            http_metrics,
            blob_download_semaphore,
            large_request_semaphore,
            request_byte_semaphore,
            opaque,
            passkeys,
            admin_passkeys,
            account_state_checks_enabled: true,
        })
    }

    /// Issues a short-lived account-recovery JWT.
    pub fn issue_account_recovery_token(&self, user_id: Uuid, username: &str) -> Result<String> {
        self.jwt.issue_token(
            TokenKind::AccountRecovery,
            user_id,
            Some(username),
            None,
            self.config.jwt_account_recovery_ttl_seconds,
        )
    }

    /// Issues an access JWT.
    pub fn issue_access_token(
        &self,
        user_id: Uuid,
        username: &str,
        session_id: Uuid,
    ) -> Result<String> {
        self.jwt.issue_token(
            TokenKind::Session,
            user_id,
            Some(username),
            Some(session_id),
            self.config.access_token_ttl_seconds,
        )
    }

    /// Issues a five-minute token proving a fresh OPAQUE reauthentication.
    pub fn issue_reauth_token(
        &self,
        user_id: Uuid,
        username: &str,
        session_id: Uuid,
    ) -> Result<String> {
        self.jwt.issue_token(
            TokenKind::Reauth,
            user_id,
            Some(username),
            Some(session_id),
            5 * 60,
        )
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
            session_id: claims.session_id,
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

    let state = AppState::new(pool, config, Arc::new(valkey))?;
    verify_opaque_setup_fingerprint(&state.pool, &state.opaque.setup_fingerprint()).await?;
    Ok(state)
}

async fn verify_opaque_setup_fingerprint(pool: &PgPool, fingerprint: &[u8; 32]) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO server_security_config (singleton, opaque_setup_version, opaque_setup_fingerprint)
        VALUES (TRUE, 1, $1)
        ON CONFLICT (singleton) DO NOTHING
        "#,
    )
    .bind(fingerprint.as_slice())
    .execute(pool)
    .await?;

    let stored: Vec<u8> = sqlx::query_scalar(
        "SELECT opaque_setup_fingerprint FROM server_security_config WHERE singleton = TRUE",
    )
    .fetch_one(pool)
    .await?;
    if stored.as_slice() != fingerprint {
        anyhow::bail!(
            "OPAQUE server setup fingerprint does not match the setup registered by this deployment"
        );
    }
    Ok(())
}
