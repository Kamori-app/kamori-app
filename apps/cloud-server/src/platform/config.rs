/// Configuration loading for the cloud server.
use anyhow::{Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::env;

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
    /// Display name used for operator security-key ceremonies.
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
    /// Pre-auth JWT TTL in seconds.
    pub jwt_preauth_ttl_seconds: i64,
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
    /// Allowed CORS origins (`*` or explicit origin list).
    pub cors_allow_origins: Vec<String>,
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

    fn parse_bool_env(name: &str, default: bool) -> bool {
        env::var(name)
            .ok()
            .map(|value| {
                let normalized = value.trim().to_ascii_lowercase();
                matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
            })
            .unwrap_or(default)
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
        let database_max_connections: u32 = env::var("KAMORI_DATABASE_MAX_CONNECTIONS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10);

        let bind_addr =
            env::var("KAMORI_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());

        let enable_totp = env::var("KAMORI_ENABLE_TOTP")
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

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
            Self::parse_bool_env("KAMORI_ALLOW_EPHEMERAL_OPAQUE_SETUP", cfg!(test));
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
        if jwt_secret.trim().is_empty() {
            bail!("KAMORI_JWT_SECRET must not be empty");
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
        let jwt_preauth_ttl_seconds: i64 = env::var("KAMORI_JWT_PREAUTH_TTL_SECONDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5 * 60);
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
            Self::parse_bool_env("KAMORI_WEB_REFRESH_COOKIE_SECURE", true);
        let web_refresh_cookie_same_site =
            env::var("KAMORI_WEB_REFRESH_COOKIE_SAMESITE").unwrap_or_else(|_| "lax".to_string());
        let web_csrf_cookie_name = env::var("KAMORI_WEB_CSRF_COOKIE_NAME")
            .unwrap_or_else(|_| "__Host-kamori_csrf".to_string());
        match web_refresh_cookie_same_site
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "lax" | "strict" | "none" => {}
            _ => bail!("KAMORI_WEB_REFRESH_COOKIE_SAMESITE must be one of: lax, strict, none"),
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
        let valkey_ttl_seconds: u64 = env::var("KAMORI_VALKEY_TTL_SECONDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(300);
        let auth_rate_limit_per_minute = env::var("KAMORI_AUTH_RATE_LIMIT_PER_MINUTE")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(30);
        let api_rate_limit_per_minute = env::var("KAMORI_API_RATE_LIMIT_PER_MINUTE")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(1200);
        if auth_rate_limit_per_minute == 0 || api_rate_limit_per_minute == 0 {
            bail!("rate limits must be greater than zero");
        }

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
        let cors_allow_credentials = env::var("KAMORI_CORS_ALLOW_CREDENTIALS")
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(true);
        let registration_enabled = Self::parse_bool_env("KAMORI_REGISTRATION_ENABLED", false);
        let beta_account_limit = Self::parse_positive_u64_env("KAMORI_BETA_ACCOUNT_LIMIT", 1_000)?;
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
        let blob_download_bytes_per_second =
            Self::parse_positive_u64_env("KAMORI_BLOB_DOWNLOAD_BYTES_PER_SECOND", 1_250_000)?;
        let global_nonessential_egress_stop_bytes = Self::parse_positive_u64_env(
            "KAMORI_GLOBAL_NONESSENTIAL_EGRESS_STOP_BYTES",
            16_000_000_000_000,
        )?;
        let global_emergency_egress_breaker_bytes = Self::parse_positive_u64_env(
            "KAMORI_GLOBAL_EMERGENCY_EGRESS_BREAKER_BYTES",
            19_000_000_000_000,
        )?;
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
        let object_store_allow_http = Self::parse_bool_env("KAMORI_OBJECT_STORE_ALLOW_HTTP", false);
        let object_store_virtual_hosted_style =
            Self::parse_bool_env("KAMORI_OBJECT_STORE_VIRTUAL_HOSTED_STYLE", false);
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
            jwt_preauth_ttl_seconds,
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
            cors_allow_origins,
            cors_allow_methods,
            cors_allow_headers,
            cors_allow_credentials,
            registration_enabled,
            beta_account_limit,
            max_blob_bytes,
            account_storage_bytes,
            owner_monthly_egress_bytes,
            owner_rolling_24h_egress_bytes,
            owner_concurrent_blob_downloads,
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
