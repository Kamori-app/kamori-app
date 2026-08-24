/// Configuration loading for the cloud server.
use anyhow::{Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ipnet::IpNet;
use std::env;
use url::Host;

#[derive(Clone)]
/// Runtime configuration values.
pub struct Config {
    /// Postgres connection URL.
    pub database_url: String,
    /// Maximum size of the DB pool.
    pub database_max_connections: u32,
    /// Socket address to bind the HTTP server.
    pub bind_addr: String,
    /// Enables TOTP verification when true.
    pub enable_totp: bool,
    /// WebAuthn relying party ID.
    pub webauthn_rp_id: String,
    /// WebAuthn relying party origin.
    pub webauthn_rp_origin: String,
    /// WebAuthn relying party name.
    pub webauthn_rp_name: String,
    /// Exact WebAuthn origin of the separately deployed operator console.
    pub admin_webauthn_rp_origin: String,
    /// Display name used for operator passkey ceremonies.
    pub admin_webauthn_rp_name: String,
    /// Deployment-owned 256-bit key used only to encrypt operator TOTP seeds.
    pub admin_totp_kek: [u8; 32],
    /// Independent 256-bit key used only to encrypt consumer TOTP seeds.
    pub auth_totp_kek: [u8; 32],
    /// Path to the base64-encoded, deployment-owned OPAQUE server setup.
    pub opaque_server_setup_path: Option<String>,
    /// Explicitly permits an ephemeral OPAQUE setup outside tests.
    pub allow_ephemeral_opaque_setup: bool,
    /// Dedicated key used to derive idempotent refresh-token replacements.
    pub refresh_rotation_key: [u8; 32],
    /// JWT signing secret (HS256).
    pub jwt_secret: String,
    /// JWT issuer string.
    pub jwt_issuer: String,
    /// JWT audience string.
    pub jwt_audience: String,
    /// Access JWT TTL in seconds.
    pub access_token_ttl_seconds: i64,
    /// Refresh token TTL in seconds.
    pub refresh_token_ttl_seconds: i64,
    /// Account-recovery JWT TTL in seconds.
    pub jwt_account_recovery_ttl_seconds: i64,
    /// Web refresh cookie name.
    pub web_refresh_cookie_name: String,
    /// Web refresh cookie path.
    pub web_refresh_cookie_path: String,
    /// Optional web refresh cookie domain.
    pub web_refresh_cookie_domain: Option<String>,
    /// Whether web refresh cookie uses `Secure`.
    pub web_refresh_cookie_secure: bool,
    /// Web refresh cookie SameSite policy (`lax`, `strict`, `none`).
    pub web_refresh_cookie_same_site: String,
    /// Web CSRF cookie name.
    pub web_csrf_cookie_name: String,
    /// Valkey connection URL.
    pub valkey_url: String,
    /// Valkey key prefix for namespacing.
    pub valkey_key_prefix: String,
    /// Valkey default TTL in seconds.
    pub valkey_ttl_seconds: u64,
    /// Maximum authentication requests per source IP per minute.
    pub auth_rate_limit_per_minute: u64,
    /// Maximum total API requests per source IP per minute.
    pub api_rate_limit_per_minute: u64,
    /// Weighted authenticated request units allowed per session per minute.
    pub session_rate_limit_units_per_minute: u64,
    /// Proxy source networks allowed to supply forwarding headers.
    pub trusted_proxy_cidrs: Vec<IpNet>,
    /// Allowed CORS origins (`*` or explicit origin list).
    pub cors_allow_origins: Vec<String>,
    /// Exact browser origins allowed to use refresh/CSRF cookies.
    pub web_cookie_origins: Vec<String>,
    /// Exact operator-console origins allowed to call `/admin-api`.
    pub admin_cors_allow_origins: Vec<String>,
    /// Allowed CORS methods (`*` or explicit HTTP methods).
    pub cors_allow_methods: Vec<String>,
    /// Allowed CORS headers (`*` or explicit header names).
    pub cors_allow_headers: Vec<String>,
    /// Whether CORS credentials are allowed.
    pub cors_allow_credentials: bool,
    /// Whether public account registration is open at all.
    pub registration_enabled: bool,
    /// Strict active-account admission ceiling for the beta.
    pub beta_account_limit: u64,
    /// Maximum normal encrypted operation payload.
    pub max_operation_bytes: u64,
    /// Maximum encrypted snapshot payload.
    pub max_snapshot_bytes: u64,
    /// Maximum encoded bytes returned by one operation-log page.
    pub max_operation_page_bytes: u64,
    /// Maximum operation-log bytes charged to one security space.
    pub space_operation_storage_bytes: u64,
    /// Maximum operation-log bytes charged to one account.
    pub account_operation_storage_bytes: u64,
    /// Maximum concurrent large authenticated request bodies per process.
    pub max_concurrent_large_requests: u64,
    /// Maximum aggregate bytes admitted for large request bodies per process.
    pub max_inflight_request_bytes: u64,
    /// Maximum padded ciphertext blob size.
    pub max_blob_bytes: u64,
    /// Maximum stored ciphertext charged to an owning account.
    pub account_storage_bytes: u64,
    /// Per-owner blob egress ceiling in one calendar month.
    pub owner_monthly_egress_bytes: u64,
    /// Per-owner blob egress ceiling in a rolling 24-hour window.
    pub owner_rolling_24h_egress_bytes: u64,
    /// Maximum simultaneous blob streams charged to one owning account.
    pub owner_concurrent_blob_downloads: u64,
    /// Cross-node cap for simultaneous blob streams, independent of owner.
    pub global_concurrent_blob_downloads: u64,
    /// Per-stream delivery rate; two default streams remain below 20 Mbit/s per owner.
    pub blob_download_bytes_per_second: u64,
    /// Global monthly point where nonessential blob delivery stops.
    pub global_nonessential_egress_stop_bytes: u64,
    /// Absolute global monthly emergency breaker for blob delivery.
    pub global_emergency_egress_breaker_bytes: u64,
    /// S3-compatible endpoint for primary encrypted blob storage.
    pub object_store_endpoint: String,
    /// S3 signing region.
    pub object_store_region: String,
    /// Primary ciphertext bucket.
    pub object_store_bucket: String,
    /// S3 access key id.
    pub object_store_access_key_id: String,
    /// S3 secret access key.
    pub object_store_secret_access_key: String,
    /// Allows an HTTP endpoint for local development only.
    pub object_store_allow_http: bool,
    /// Uses virtual-hosted-style S3 requests when true.
    pub object_store_virtual_hosted_style: bool,
    /// Secret bearer token required to scrape operational metrics.
    pub metrics_bearer_token: String,
}

impl Config {
    fn validate_cookie_name(name: &str, env_name: &str) -> Result<()> {
        if name.is_empty()
            || !name.bytes().all(|byte| {
                byte.is_ascii_alphanumeric()
                    || matches!(
                        byte,
                        b'!' | b'#'
                            | b'$'
                            | b'%'
                            | b'&'
                            | b'\''
                            | b'*'
                            | b'+'
                            | b'-'
                            | b'.'
                            | b'^'
                            | b'_'
                            | b'`'
                            | b'|'
                            | b'~'
                    )
            })
        {
            bail!("{env_name} must be a non-empty HTTP cookie token");
        }
        Ok(())
    }

    fn validate_cookie_path(path: &str) -> Result<()> {
        if !path.starts_with('/')
            || path
                .bytes()
                .any(|byte| byte.is_ascii_control() || matches!(byte, b';' | b','))
        {
            bail!(
                "KAMORI_WEB_REFRESH_COOKIE_PATH must start with '/' and contain no control, comma, or semicolon characters"
            );
        }
        Ok(())
    }

    fn validate_cookie_domain(domain: &str) -> Result<()> {
        if domain.starts_with('.') || domain.ends_with('.') {
            bail!(
                "KAMORI_WEB_REFRESH_COOKIE_DOMAIN must be a canonical hostname without leading or trailing dots"
            );
        }
        match Host::parse(domain) {
            Ok(Host::Domain(_)) => Ok(()),
            _ => bail!("KAMORI_WEB_REFRESH_COOKIE_DOMAIN must be a valid hostname"),
        }
    }

    fn parse_csv_env(name: &str, default: &[&str]) -> Vec<String> {
        match env::var(name) {
            Ok(value) => value
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToOwned::to_owned)
                .collect(),
            Err(_) => default.iter().map(|item| (*item).to_owned()).collect(),
        }
    }

    fn parse_cidr_env(name: &str) -> Result<Vec<IpNet>> {
        let Some(value) = env::var(name).ok() else {
            return Ok(Vec::new());
        };
        value
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(|item| {
                item.parse::<IpNet>()
                    .map_err(|_| anyhow::anyhow!("{name} contains invalid CIDR {item:?}"))
            })
            .collect()
    }

    fn parse_positive_i64_env(name: &str, default: i64) -> Result<i64> {
        let value = match env::var(name) {
            Ok(raw) => raw
                .parse::<i64>()
                .map_err(|_| anyhow::anyhow!("{name} must be a valid integer"))?,
            Err(_) => default,
        };
        if value <= 0 {
            bail!("{name} must be > 0");
        }
        Ok(value)
    }

    fn parse_positive_u64_env(name: &str, default: u64) -> Result<u64> {
        let value = match env::var(name) {
            Ok(raw) => raw
                .parse::<u64>()
                .map_err(|_| anyhow::anyhow!("{name} must be a valid unsigned integer"))?,
            Err(_) => default,
        };
        if value == 0 {
            bail!("{name} must be > 0");
        }
        Ok(value)
    }

    fn parse_bool_env(name: &str, default: bool) -> Result<bool> {
        let Ok(value) = env::var(name) else {
            return Ok(default);
        };
        match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => bail!("{name} must be a boolean (true/false, yes/no, on/off, or 1/0)"),
        }
    }

    fn required_env(name: &str) -> Result<String> {
        let value = env::var(name).map_err(|_| anyhow::anyhow!("{name} must be set"))?;
        if value.trim().is_empty() {
            bail!("{name} must not be empty");
        }
        Ok(value)
    }

    /// Loads and validates the dedicated operator TOTP key-encryption key.
    pub(crate) fn load_admin_totp_kek() -> Result<[u8; 32]> {
        Self::load_base64_key("KAMORI_ADMIN_TOTP_KEK", 0x42)
    }

    fn load_base64_key(name: &str, test_byte: u8) -> Result<[u8; 32]> {
        let encoded = match env::var(name) {
            Ok(value) => value,
            Err(_) if cfg!(test) => STANDARD.encode([test_byte; 32]),
            Err(_) => bail!("{name} must be set"),
        };
        let decoded = STANDARD
            .decode(encoded.trim())
            .map_err(|_| anyhow::anyhow!("{name} must be standard base64"))?;
        decoded
            .try_into()
            .map_err(|_| anyhow::anyhow!("{name} must decode to exactly 32 bytes"))
    }

    fn load_base64_key_file(name: &str, path_name: &str, test_byte: u8) -> Result<[u8; 32]> {
        if let Ok(path) = env::var(path_name) {
            let encoded = std::fs::read_to_string(path.trim())
                .map_err(|error| anyhow::anyhow!("failed to read {path_name}: {error}"))?;
            let decoded = STANDARD
                .decode(encoded.trim())
                .map_err(|_| anyhow::anyhow!("{path_name} must contain standard base64"))?;
            return decoded
                .try_into()
                .map_err(|_| anyhow::anyhow!("{path_name} must contain exactly 32 decoded bytes"));
        }
        Self::load_base64_key(name, test_byte)
    }

    /// Loads configuration from environment variables.
    pub fn load() -> Result<Self> {
        let _ = dotenvy::dotenv();

        let database_url = env::var("KAMORI_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://localhost:5432/kamori".to_string());
        let database_max_connections = match env::var("KAMORI_DATABASE_MAX_CONNECTIONS") {
            Ok(value) => value.parse::<u32>().map_err(|_| {
                anyhow::anyhow!("KAMORI_DATABASE_MAX_CONNECTIONS must be a valid unsigned integer")
            })?,
            Err(_) => 10,
        };
        if database_max_connections == 0 {
            bail!("KAMORI_DATABASE_MAX_CONNECTIONS must be > 0");
        }

        let bind_addr =
            env::var("KAMORI_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());

        let enable_totp = Self::parse_bool_env("KAMORI_ENABLE_TOTP", false)?;

        let webauthn_rp_id =
            env::var("KAMORI_WEBAUTHN_RP_ID").unwrap_or_else(|_| "localhost".to_string());
        let webauthn_rp_origin = env::var("KAMORI_WEBAUTHN_RP_ORIGIN")
            .unwrap_or_else(|_| "http://localhost:8080".to_string());
        let webauthn_rp_name =
            env::var("KAMORI_WEBAUTHN_RP_NAME").unwrap_or_else(|_| "Kamori".to_string());
        let admin_webauthn_rp_origin = env::var("KAMORI_ADMIN_WEBAUTHN_RP_ORIGIN")
            .unwrap_or_else(|_| "https://admin.kamori.app".to_string());
        let admin_webauthn_rp_name = env::var("KAMORI_ADMIN_WEBAUTHN_RP_NAME")
            .unwrap_or_else(|_| "Kamori Admin".to_string());
        let admin_totp_kek = Self::load_admin_totp_kek()?;
        let auth_totp_kek = Self::load_base64_key("KAMORI_AUTH_TOTP_KEK", 0x24)?;
        let opaque_server_setup_path = env::var("KAMORI_OPAQUE_SERVER_SETUP_FILE")
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        let allow_ephemeral_opaque_setup =
            Self::parse_bool_env("KAMORI_ALLOW_EPHEMERAL_OPAQUE_SETUP", cfg!(test))?;
        if opaque_server_setup_path.is_none() && !allow_ephemeral_opaque_setup {
            bail!(
                "KAMORI_OPAQUE_SERVER_SETUP_FILE must point to a persistent setup; ephemeral OPAQUE is allowed only with KAMORI_ALLOW_EPHEMERAL_OPAQUE_SETUP=true"
            );
        }
        let refresh_rotation_key = Self::load_base64_key_file(
            "KAMORI_REFRESH_ROTATION_KEY",
            "KAMORI_REFRESH_ROTATION_KEY_FILE",
            0x63,
        )?;

        let jwt_secret = match env::var("KAMORI_JWT_SECRET") {
            Ok(value) => value,
            Err(_) if cfg!(test) => "test-jwt-secret-not-for-production".to_string(),
            Err(_) => bail!("KAMORI_JWT_SECRET must be set"),
        };
        if jwt_secret.trim().len() < 32 {
            bail!("KAMORI_JWT_SECRET must contain at least 32 bytes");
        }
        if !cfg!(test) && jwt_secret == "change-me" {
            bail!("KAMORI_JWT_SECRET uses insecure placeholder value");
        }
        let jwt_issuer = env::var("KAMORI_JWT_ISSUER").unwrap_or_else(|_| "kamori".to_string());
        let jwt_audience =
            env::var("KAMORI_JWT_AUDIENCE").unwrap_or_else(|_| "kamori-clients".to_string());
        let access_token_ttl_seconds =
            Self::parse_positive_i64_env("KAMORI_ACCESS_TOKEN_TTL_SECONDS", 5 * 60)?;
        let refresh_token_ttl_seconds =
            Self::parse_positive_i64_env("KAMORI_REFRESH_TOKEN_TTL_SECONDS", 30 * 24 * 60 * 60)?;
        let jwt_account_recovery_ttl_seconds =
            Self::parse_positive_i64_env("KAMORI_JWT_ACCOUNT_RECOVERY_TTL_SECONDS", 10 * 60)?;
        let web_refresh_cookie_name = env::var("KAMORI_WEB_REFRESH_COOKIE_NAME")
            .unwrap_or_else(|_| "__Host-kamori_rt".to_string());
        let web_refresh_cookie_path =
            env::var("KAMORI_WEB_REFRESH_COOKIE_PATH").unwrap_or_else(|_| "/".to_string());
        let web_refresh_cookie_domain = env::var("KAMORI_WEB_REFRESH_COOKIE_DOMAIN")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let web_refresh_cookie_secure =
            Self::parse_bool_env("KAMORI_WEB_REFRESH_COOKIE_SECURE", true)?;
        let web_refresh_cookie_same_site = env::var("KAMORI_WEB_REFRESH_COOKIE_SAMESITE")
            .unwrap_or_else(|_| "lax".to_string())
            .trim()
            .to_ascii_lowercase();
        let web_csrf_cookie_name = env::var("KAMORI_WEB_CSRF_COOKIE_NAME")
            .unwrap_or_else(|_| "__Host-kamori_csrf".to_string());
        Self::validate_cookie_name(&web_refresh_cookie_name, "KAMORI_WEB_REFRESH_COOKIE_NAME")?;
        Self::validate_cookie_name(&web_csrf_cookie_name, "KAMORI_WEB_CSRF_COOKIE_NAME")?;
        Self::validate_cookie_path(&web_refresh_cookie_path)?;
        if let Some(domain) = &web_refresh_cookie_domain {
            Self::validate_cookie_domain(domain)?;
        }
        if web_refresh_cookie_name == web_csrf_cookie_name {
            bail!("refresh and CSRF cookie names must be different");
        }
        match web_refresh_cookie_same_site.as_str() {
            "lax" | "strict" | "none" => {}
            _ => bail!("KAMORI_WEB_REFRESH_COOKIE_SAMESITE must be one of: lax, strict, none"),
        }
        if web_refresh_cookie_same_site == "none" && !web_refresh_cookie_secure {
            bail!("KAMORI_WEB_REFRESH_COOKIE_SAMESITE=none requires a Secure cookie");
        }
        if web_refresh_cookie_name.starts_with("__Host-")
            && (web_refresh_cookie_domain.is_some()
                || web_refresh_cookie_path != "/"
                || !web_refresh_cookie_secure)
        {
            bail!(
                "__Host- cookie requires KAMORI_WEB_REFRESH_COOKIE_DOMAIN unset, KAMORI_WEB_REFRESH_COOKIE_PATH=/, and KAMORI_WEB_REFRESH_COOKIE_SECURE=true (for local HTTP dev use non-__Host- cookie names)"
            );
        }
        if web_csrf_cookie_name.starts_with("__Host-")
            && (web_refresh_cookie_domain.is_some()
                || web_refresh_cookie_path != "/"
                || !web_refresh_cookie_secure)
        {
            bail!(
                "__Host- CSRF cookie requires KAMORI_WEB_REFRESH_COOKIE_DOMAIN unset, KAMORI_WEB_REFRESH_COOKIE_PATH=/, and KAMORI_WEB_REFRESH_COOKIE_SECURE=true (for local HTTP dev use non-__Host- cookie names)"
            );
        }

        let valkey_url = env::var("KAMORI_VALKEY_URL")
            .unwrap_or_else(|_| "valkey://127.0.0.1:6379/0".to_string());
        let valkey_key_prefix =
            env::var("KAMORI_VALKEY_KEY_PREFIX").unwrap_or_else(|_| "kamori:".to_string());
        let valkey_ttl_seconds = Self::parse_positive_u64_env("KAMORI_VALKEY_TTL_SECONDS", 300)?;
        let auth_rate_limit_per_minute =
            Self::parse_positive_u64_env("KAMORI_AUTH_RATE_LIMIT_PER_MINUTE", 30)?;
        let api_rate_limit_per_minute =
            Self::parse_positive_u64_env("KAMORI_API_RATE_LIMIT_PER_MINUTE", 1200)?;
        let session_rate_limit_units_per_minute =
            Self::parse_positive_u64_env("KAMORI_SESSION_RATE_LIMIT_UNITS_PER_MINUTE", 6000)?;
        let trusted_proxy_cidrs = Self::parse_cidr_env("KAMORI_TRUSTED_PROXY_CIDRS")?;

        let cors_allow_origins = Self::parse_csv_env(
            "KAMORI_CORS_ALLOW_ORIGINS",
            &[
                "http://localhost:4173",
                "http://127.0.0.1:4173",
                "http://localhost:1420",
                "http://127.0.0.1:1420",
                "tauri://localhost",
            ],
        );
        let web_cookie_origins = Self::parse_csv_env(
            "KAMORI_WEB_COOKIE_ORIGINS",
            &["http://localhost:4173", "http://127.0.0.1:4173"],
        );
        let admin_cors_allow_origins = Self::parse_csv_env(
            "KAMORI_ADMIN_CORS_ALLOW_ORIGINS",
            &["https://admin.kamori.app"],
        );
        let cors_allow_methods = Self::parse_csv_env(
            "KAMORI_CORS_ALLOW_METHODS",
            &["GET", "POST", "DELETE", "OPTIONS"],
        );
        let cors_allow_headers = Self::parse_csv_env(
            "KAMORI_CORS_ALLOW_HEADERS",
            &[
                "authorization",
                "content-type",
                "accept",
                "x-kamori-refresh-transport",
                "x-kamori-csrf-token",
            ],
        );
        let cors_allow_credentials = Self::parse_bool_env("KAMORI_CORS_ALLOW_CREDENTIALS", true)?;
        let registration_enabled = Self::parse_bool_env("KAMORI_REGISTRATION_ENABLED", false)?;
        let beta_account_limit = Self::parse_positive_u64_env("KAMORI_BETA_ACCOUNT_LIMIT", 1_000)?;
        let max_operation_bytes =
            Self::parse_positive_u64_env("KAMORI_MAX_OPERATION_BYTES", 1024 * 1024)?;
        let max_snapshot_bytes =
            Self::parse_positive_u64_env("KAMORI_MAX_SNAPSHOT_BYTES", 4 * 1024 * 1024)?;
        let max_operation_page_bytes =
            Self::parse_positive_u64_env("KAMORI_MAX_OPERATION_PAGE_BYTES", 8 * 1024 * 1024)?;
        let space_operation_storage_bytes =
            Self::parse_positive_u64_env("KAMORI_SPACE_OPERATION_STORAGE_BYTES", 250_000_000)?;
        let account_operation_storage_bytes =
            Self::parse_positive_u64_env("KAMORI_ACCOUNT_OPERATION_STORAGE_BYTES", 1_000_000_000)?;
        if max_operation_bytes > max_snapshot_bytes
            || max_snapshot_bytes > max_operation_page_bytes
            || space_operation_storage_bytes > account_operation_storage_bytes
        {
            bail!(
                "operation, snapshot, page, space, and account limits must be monotonically increasing"
            );
        }
        let max_concurrent_large_requests =
            Self::parse_positive_u64_env("KAMORI_MAX_CONCURRENT_LARGE_REQUESTS", 16)?;
        let max_inflight_request_bytes =
            Self::parse_positive_u64_env("KAMORI_MAX_INFLIGHT_REQUEST_BYTES", 128 * 1024 * 1024)?;
        if max_concurrent_large_requests > 1024
            || max_inflight_request_bytes > 4 * 1024 * 1024 * 1024
        {
            bail!("large request process limits exceed their safety ceilings");
        }
        let max_blob_bytes =
            Self::parse_positive_u64_env("KAMORI_MAX_BLOB_BYTES", 25 * 1024 * 1024)?;
        let account_storage_bytes =
            Self::parse_positive_u64_env("KAMORI_ACCOUNT_STORAGE_BYTES", 5_000_000_000)?;
        let owner_monthly_egress_bytes =
            Self::parse_positive_u64_env("KAMORI_OWNER_MONTHLY_EGRESS_BYTES", 10_000_000_000)?;
        let owner_rolling_24h_egress_bytes =
            Self::parse_positive_u64_env("KAMORI_OWNER_ROLLING_24H_EGRESS_BYTES", 2_000_000_000)?;
        let owner_concurrent_blob_downloads =
            Self::parse_positive_u64_env("KAMORI_OWNER_CONCURRENT_BLOB_DOWNLOADS", 2)?;
        let global_concurrent_blob_downloads =
            Self::parse_positive_u64_env("KAMORI_GLOBAL_CONCURRENT_BLOB_DOWNLOADS", 64)?;
        if owner_concurrent_blob_downloads > 100 {
            bail!("KAMORI_OWNER_CONCURRENT_BLOB_DOWNLOADS must not exceed 100");
        }
        if global_concurrent_blob_downloads > 4096 {
            bail!("KAMORI_GLOBAL_CONCURRENT_BLOB_DOWNLOADS must not exceed 4096");
        }
        if owner_concurrent_blob_downloads > global_concurrent_blob_downloads {
            bail!(
                "KAMORI_OWNER_CONCURRENT_BLOB_DOWNLOADS cannot exceed KAMORI_GLOBAL_CONCURRENT_BLOB_DOWNLOADS"
            );
        }
        let blob_download_bytes_per_second =
            Self::parse_positive_u64_env("KAMORI_BLOB_DOWNLOAD_BYTES_PER_SECOND", 1_250_000)?;
        if !(102_400..=104_857_600).contains(&blob_download_bytes_per_second) {
            bail!("KAMORI_BLOB_DOWNLOAD_BYTES_PER_SECOND must be between 100 KiB/s and 100 MiB/s");
        }
        let global_nonessential_egress_stop_bytes = Self::parse_positive_u64_env(
            "KAMORI_GLOBAL_NONESSENTIAL_EGRESS_STOP_BYTES",
            16_000_000_000_000,
        )?;
        let global_emergency_egress_breaker_bytes = Self::parse_positive_u64_env(
            "KAMORI_GLOBAL_EMERGENCY_EGRESS_BREAKER_BYTES",
            19_000_000_000_000,
        )?;
        if !(1024 * 1024..=25 * 1024 * 1024).contains(&max_blob_bytes)
            || !max_blob_bytes.is_multiple_of(1024 * 1024)
        {
            bail!("KAMORI_MAX_BLOB_BYTES must be 1 MiB aligned and at most 25 MiB");
        }
        if max_blob_bytes > account_storage_bytes {
            bail!("KAMORI_MAX_BLOB_BYTES cannot exceed KAMORI_ACCOUNT_STORAGE_BYTES");
        }
        if owner_rolling_24h_egress_bytes > owner_monthly_egress_bytes {
            bail!(
                "KAMORI_OWNER_ROLLING_24H_EGRESS_BYTES cannot exceed KAMORI_OWNER_MONTHLY_EGRESS_BYTES"
            );
        }
        for (name, value) in [
            ("KAMORI_MAX_BLOB_BYTES", max_blob_bytes),
            ("KAMORI_ACCOUNT_STORAGE_BYTES", account_storage_bytes),
            (
                "KAMORI_OWNER_MONTHLY_EGRESS_BYTES",
                owner_monthly_egress_bytes,
            ),
            (
                "KAMORI_OWNER_ROLLING_24H_EGRESS_BYTES",
                owner_rolling_24h_egress_bytes,
            ),
            (
                "KAMORI_GLOBAL_NONESSENTIAL_EGRESS_STOP_BYTES",
                global_nonessential_egress_stop_bytes,
            ),
            (
                "KAMORI_GLOBAL_EMERGENCY_EGRESS_BREAKER_BYTES",
                global_emergency_egress_breaker_bytes,
            ),
        ] {
            if value > i64::MAX as u64 {
                bail!("{name} exceeds the PostgreSQL BIGINT range");
            }
        }
        if global_nonessential_egress_stop_bytes >= global_emergency_egress_breaker_bytes {
            bail!("global nonessential egress stop must be below the emergency egress breaker");
        }
        let (
            object_store_endpoint,
            object_store_region,
            object_store_bucket,
            object_store_access_key_id,
            object_store_secret_access_key,
        ) = if cfg!(test) && env::var("KAMORI_OBJECT_STORE_ENDPOINT").is_err() {
            (
                "memory://".to_string(),
                "test".to_string(),
                "test".to_string(),
                "test".to_string(),
                "test".to_string(),
            )
        } else {
            (
                Self::required_env("KAMORI_OBJECT_STORE_ENDPOINT")?,
                Self::required_env("KAMORI_OBJECT_STORE_REGION")?,
                Self::required_env("KAMORI_OBJECT_STORE_BUCKET")?,
                Self::required_env("KAMORI_OBJECT_STORE_ACCESS_KEY_ID")?,
                Self::required_env("KAMORI_OBJECT_STORE_SECRET_ACCESS_KEY")?,
            )
        };
        let object_store_allow_http =
            Self::parse_bool_env("KAMORI_OBJECT_STORE_ALLOW_HTTP", false)?;
        let object_store_virtual_hosted_style =
            Self::parse_bool_env("KAMORI_OBJECT_STORE_VIRTUAL_HOSTED_STYLE", false)?;
        if !cfg!(test) && object_store_endpoint == "memory://" {
            bail!("memory object storage is test-only");
        }
        if object_store_endpoint.starts_with("http://") && !object_store_allow_http {
            bail!("HTTP object store endpoint requires KAMORI_OBJECT_STORE_ALLOW_HTTP=true");
        }
        let metrics_bearer_token = if cfg!(test) && env::var("KAMORI_METRICS_BEARER_TOKEN").is_err()
        {
            "test-metrics-token-at-least-32-bytes".to_string()
        } else {
            Self::required_env("KAMORI_METRICS_BEARER_TOKEN")?
        };
        if metrics_bearer_token.len() < 32 {
            bail!("KAMORI_METRICS_BEARER_TOKEN must contain at least 32 bytes");
        }

        Ok(Self {
            database_url,
            database_max_connections,
            bind_addr,
            enable_totp,
            webauthn_rp_id,
            webauthn_rp_origin,
            webauthn_rp_name,
            admin_webauthn_rp_origin,
            admin_webauthn_rp_name,
            admin_totp_kek,
            auth_totp_kek,
            opaque_server_setup_path,
            allow_ephemeral_opaque_setup,
            refresh_rotation_key,
            jwt_secret,
            jwt_issuer,
            jwt_audience,
            access_token_ttl_seconds,
            refresh_token_ttl_seconds,
            jwt_account_recovery_ttl_seconds,
            web_refresh_cookie_name,
            web_refresh_cookie_path,
            web_refresh_cookie_domain,
            web_refresh_cookie_secure,
            web_refresh_cookie_same_site,
            web_csrf_cookie_name,
            valkey_url,
            valkey_key_prefix,
            valkey_ttl_seconds,
            auth_rate_limit_per_minute,
            api_rate_limit_per_minute,
            session_rate_limit_units_per_minute,
            trusted_proxy_cidrs,
            cors_allow_origins,
            web_cookie_origins,
            admin_cors_allow_origins,
            cors_allow_methods,
            cors_allow_headers,
            cors_allow_credentials,
            registration_enabled,
            beta_account_limit,
            max_operation_bytes,
            max_snapshot_bytes,
            max_operation_page_bytes,
            space_operation_storage_bytes,
            account_operation_storage_bytes,
            max_concurrent_large_requests,
            max_inflight_request_bytes,
            max_blob_bytes,
            account_storage_bytes,
            owner_monthly_egress_bytes,
            owner_rolling_24h_egress_bytes,
            owner_concurrent_blob_downloads,
            global_concurrent_blob_downloads,
            blob_download_bytes_per_second,
            global_nonessential_egress_stop_bytes,
            global_emergency_egress_breaker_bytes,
            object_store_endpoint,
            object_store_region,
            object_store_bucket,
            object_store_access_key_id,
            object_store_secret_access_key,
            object_store_allow_http,
            object_store_virtual_hosted_style,
            metrics_bearer_token,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn cookie_names_accept_tokens_and_reject_header_delimiters() {
        assert!(Config::validate_cookie_name("__Host-kamori_rt", "TEST").is_ok());
        assert!(Config::validate_cookie_name("kamori rt", "TEST").is_err());
        assert!(Config::validate_cookie_name("kamori=rt", "TEST").is_err());
        assert!(Config::validate_cookie_name("kamori;rt", "TEST").is_err());
    }

    #[test]
    fn cookie_paths_reject_attribute_injection() {
        assert!(Config::validate_cookie_path("/").is_ok());
        assert!(Config::validate_cookie_path("/auth/refresh").is_ok());
        assert!(Config::validate_cookie_path("auth").is_err());
        assert!(Config::validate_cookie_path("/; Secure").is_err());
        assert!(Config::validate_cookie_path("/\r\nX-Test: 1").is_err());
    }

    #[test]
    fn cookie_domains_are_canonical_hostnames() {
        assert!(Config::validate_cookie_domain("kamori.app").is_ok());
        assert!(Config::validate_cookie_domain("api.kamori.app").is_ok());
        assert!(Config::validate_cookie_domain(".kamori.app").is_err());
        assert!(Config::validate_cookie_domain("https://kamori.app").is_err());
        assert!(Config::validate_cookie_domain("127.0.0.1").is_err());
    }
}
