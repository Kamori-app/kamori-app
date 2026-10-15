//! Shared test helpers for platform-dependent feature tests.

use std::sync::Arc;
use std::time::Duration;

use sqlx::postgres::PgPoolOptions;

use crate::platform::{config::Config, state::AppState, state_store::InMemoryStore};

/// Requires a dedicated PostgreSQL database for integration tests.
///
/// Empty or whitespace-only values are treated as missing. Unit tests using
/// `test_config` or `test_state` do not need a live database.
pub fn required_test_database_url() -> String {
    let explicit = std::env::var("KAMORI_DATABASE_URL").ok();
    let fallback = std::env::var("DATABASE_URL").ok();
    resolve_test_database_url(explicit.as_deref(), fallback.as_deref())
        .expect(
            "database integration tests require a dedicated PostgreSQL database: set a nonempty \
             KAMORI_DATABASE_URL or DATABASE_URL; see apps/cloud-server/README.md",
        )
        .to_owned()
}

fn resolve_test_database_url<'a>(
    explicit: Option<&'a str>,
    fallback: Option<&'a str>,
) -> Option<&'a str> {
    explicit
        .filter(|url| !url.trim().is_empty())
        .or_else(|| fallback.filter(|url| !url.trim().is_empty()))
}

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
        opaque_server_setup_path: None,
        allow_ephemeral_opaque_setup: true,
        refresh_rotation_key: [0x63_u8; 32],
        jwt_secret: "test-secret".to_string(),
        jwt_issuer: "kamori".to_string(),
        jwt_audience: "kamori-clients".to_string(),
        access_token_ttl_seconds: 300,
        refresh_token_ttl_seconds: 2592000,
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
        session_rate_limit_units_per_minute: 20000,
        trusted_proxy_cidrs: Vec::new(),
        cors_allow_origins: vec![
            "http://localhost:4173".to_string(),
            "https://app.example.com".to_string(),
        ],
        web_cookie_origins: vec![
            "http://localhost:4173".to_string(),
            "https://app.example.com".to_string(),
        ],
        admin_cors_allow_origins: vec!["https://admin.example.com".to_string()],
        cors_allow_methods: vec!["GET".to_string(), "POST".to_string()],
        cors_allow_headers: vec!["authorization".to_string()],
        cors_allow_credentials: true,
        registration_enabled: true,
        beta_account_limit: 1000,
        max_operation_bytes: 1024 * 1024,
        max_snapshot_bytes: 4 * 1024 * 1024,
        max_operation_page_bytes: 8 * 1024 * 1024,
        space_operation_storage_bytes: 250_000_000,
        account_operation_storage_bytes: 1_000_000_000,
        max_concurrent_large_requests: 16,
        max_inflight_request_bytes: 128 * 1024 * 1024,
        max_blob_bytes: 25 * 1024 * 1024,
        account_storage_bytes: 5_000_000_000,
        owner_monthly_egress_bytes: 10_000_000_000,
        owner_rolling_24h_egress_bytes: 2_000_000_000,
        owner_concurrent_blob_downloads: 2,
        global_concurrent_blob_downloads: 64,
        blob_download_bytes_per_second: 1_250_000,
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

#[cfg(test)]
mod tests {
    use super::resolve_test_database_url;

    #[test]
    fn explicit_database_url_takes_precedence() {
        assert_eq!(
            resolve_test_database_url(
                Some("postgres://explicit/test"),
                Some("postgres://fallback/test")
            ),
            Some("postgres://explicit/test"),
        );
    }

    #[test]
    fn fallback_database_url_is_used_when_explicit_is_missing_or_blank() {
        for explicit in [None, Some(""), Some(" \t\n")] {
            assert_eq!(
                resolve_test_database_url(explicit, Some("postgres://fallback/test")),
                Some("postgres://fallback/test"),
            );
        }
    }

    #[test]
    fn explicit_database_url_does_not_require_fallback() {
        for fallback in [None, Some(""), Some(" \t\n")] {
            assert_eq!(
                resolve_test_database_url(Some("postgres://explicit/test"), fallback),
                Some("postgres://explicit/test"),
            );
        }
    }

    #[test]
    fn missing_or_blank_database_urls_are_rejected() {
        for explicit in [None, Some(""), Some(" \t\n")] {
            for fallback in [None, Some(""), Some(" \t\n")] {
                assert_eq!(resolve_test_database_url(explicit, fallback), None);
            }
        }
    }

    #[test]
    fn database_integration_test_requires_postgres_configuration() {
        let test_name = "features::admin::integration_tests::passkey_names_are_unique_rename_is_audited_and_last_key_is_retained";
        let output =
            std::process::Command::new(std::env::current_exe().expect("current test binary"))
                .args(["--exact", test_name, "--nocapture"])
                .env_remove("KAMORI_DATABASE_URL")
                .env_remove("DATABASE_URL")
                .output()
                .expect("run representative database integration test");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stdout.contains("running 1 test"),
            "integration test must actually run: {stdout} {stderr}"
        );
        assert!(
            stdout.contains(test_name),
            "expected integration test must run: {stdout} {stderr}"
        );
        assert!(
            !output.status.success(),
            "database integration test must fail without PostgreSQL configuration: {stdout} {stderr}"
        );
        assert!(
            stderr.contains("KAMORI_DATABASE_URL")
                && stderr.contains("DATABASE_URL")
                && stderr.contains("dedicated PostgreSQL"),
            "missing database diagnostic must explain the prerequisite: {stdout} {stderr}"
        );
    }
}
