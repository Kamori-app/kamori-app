//! Shared test helpers for platform-dependent feature tests.

use std::sync::Arc;
use std::time::Duration;

use sqlx::postgres::PgPoolOptions;

use crate::platform::{config::Config, state::AppState, state_store::InMemoryStore};

/// Returns a stable config baseline for unit tests.
pub fn test_config() -> Config {
    Config {
        database_url: "postgres://localhost:5432/kamori".to_string(),
        database_max_connections: 4,
        bind_addr: "127.0.0.1:3000".to_string(),
        enable_totp: false,
        webauthn_rp_id: "localhost".to_string(),
        webauthn_rp_origin: "http://localhost:3000".to_string(),
        webauthn_rp_name: "Kamori".to_string(),
        admin_webauthn_rp_origin: "http://localhost:4174".to_string(),
        admin_webauthn_rp_name: "Kamori Admin".to_string(),
        admin_totp_kek: [0x42_u8; 32],
        auth_totp_kek: [0x24_u8; 32],
        jwt_secret: "test-secret".to_string(),
        jwt_issuer: "kamori".to_string(),
        jwt_audience: "kamori-clients".to_string(),
        access_token_ttl_seconds: 300,
        refresh_token_ttl_seconds: 2592000,
        jwt_preauth_ttl_seconds: 300,
        jwt_account_recovery_ttl_seconds: 600,
        web_refresh_cookie_name: "__Host-kamori_rt".to_string(),
        web_refresh_cookie_path: "/".to_string(),
        web_refresh_cookie_domain: None,
        web_refresh_cookie_secure: true,
        web_refresh_cookie_same_site: "lax".to_string(),
        web_csrf_cookie_name: "__Host-kamori_csrf".to_string(),
        valkey_url: "valkey://127.0.0.1:6379/0".to_string(),
        valkey_key_prefix: "kamori:".to_string(),
        valkey_ttl_seconds: 300,
        auth_rate_limit_per_minute: 1000,
        api_rate_limit_per_minute: 10000,
        cors_allow_origins: vec![
            "http://localhost:4173".to_string(),
            "https://app.example.com".to_string(),
        ],
        cors_allow_methods: vec!["GET".to_string(), "POST".to_string()],
        cors_allow_headers: vec!["authorization".to_string()],
        cors_allow_credentials: true,
        registration_enabled: true,
        beta_account_limit: 1000,
        max_blob_bytes: 25 * 1024 * 1024,
        account_storage_bytes: 5_000_000_000,
        owner_monthly_egress_bytes: 10_000_000_000,
        owner_rolling_24h_egress_bytes: 2_000_000_000,
        global_nonessential_egress_stop_bytes: 16_000_000_000_000,
        global_emergency_egress_breaker_bytes: 19_000_000_000_000,
        object_store_endpoint: "memory://".to_string(),
        object_store_region: "test".to_string(),
        object_store_bucket: "test".to_string(),
        object_store_access_key_id: "test".to_string(),
        object_store_secret_access_key: "test".to_string(),
        object_store_allow_http: false,
        object_store_virtual_hosted_style: false,
        metrics_bearer_token: "test-metrics-token-at-least-32-bytes".to_string(),
    }
}

/// Builds a test application state with lazy Postgres pool and in-memory store.
pub fn test_state() -> AppState {
    let config = test_config();
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy(&config.database_url)
        .expect("lazy pool");
    let store = Arc::new(InMemoryStore::new(Duration::from_secs(
        config.valkey_ttl_seconds,
    )));
    let mut state = AppState::new(pool, config, store).expect("state");
    state.account_state_checks_enabled = false;
    state
}
