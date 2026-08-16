//! Application bootstrap: wiring config, infrastructure and HTTP server startup.

use std::sync::Arc;
use std::{
    io::{Read, Write},
    net::TcpStream,
    time::Duration as StdDuration,
};

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::{
    app::router::build_router,
    platform::{
        config::Config,
        db::{assert_postgres_url, create_pool},
        state::AppState,
        state_store::{ValkeyConfig, ValkeyStore},
    },
};

fn init_tracing() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "cloud_server=debug,tower_http=debug,axum=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}

pub async fn run() -> anyhow::Result<()> {
    init_tracing();

    let config = Config::load()?;
    let bind_addr = config.bind_addr.clone();
    assert_postgres_url(&config.database_url)?;
    let pool = create_pool(&config.database_url, config.database_max_connections).await?;

    let valkey = ValkeyStore::new(ValkeyConfig::new(
        config.valkey_url.clone(),
        config.valkey_key_prefix.clone(),
        std::time::Duration::from_secs(config.valkey_ttl_seconds),
    ))?;
    let state = AppState::new(pool, config, Arc::new(valkey))?;
    tokio::spawn(crate::platform::maintenance::run(state.clone()));
    let app = build_router(state)?;
    let addr: std::net::SocketAddr = bind_addr.parse()?;
    tracing::info!(%addr, "starting cloud-server");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;

    Ok(())
}

/// Applies embedded PostgreSQL migrations and exits without starting HTTP.
pub async fn migrate() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    let database_url = std::env::var("KAMORI_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .map_err(|_| anyhow::anyhow!("KAMORI_DATABASE_URL or DATABASE_URL must be set"))?;
    assert_postgres_url(&database_url)?;
    let pool = create_pool(&database_url, 2).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    pool.close().await;
    Ok(())
}

/// Creates or rotates a one-time 15-minute operator enrollment credential.
pub async fn admin_bootstrap(username: &str) -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    let username = username.trim();
    anyhow::ensure!(
        (3..=64).contains(&username.len())
            && username.chars().all(
                |character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            ),
        "operator username must be 3-64 ASCII letters, digits, '-' or '_'"
    );
    let database_url = std::env::var("KAMORI_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .map_err(|_| anyhow::anyhow!("KAMORI_DATABASE_URL or DATABASE_URL must be set"))?;
    assert_postgres_url(&database_url)?;
    let pool = create_pool(&database_url, 2).await?;
    let totp_secret = crate::features::auth::services::support::generate_totp_manual_entry_key();
    let admin_totp_kek = Config::load_admin_totp_kek()?;
    let totp_secret_ciphertext =
        crate::platform::secret_box::encrypt_admin_totp(&admin_totp_kek, username, &totp_secret)?;
    let bootstrap = crate::features::admin::repositories::create_bootstrap(
        &pool,
        username,
        &totp_secret_ciphertext,
    )
    .await?;
    let otpauth_uri = crate::features::auth::services::support::build_totp_otpauth_uri(
        "Kamori Admin",
        username,
        &totp_secret,
    )
    .map_err(|error| anyhow::anyhow!(error.1.0.error))?;
    println!("operator_id={}", bootstrap.admin_user_id);
    println!("bootstrap_token={}", bootstrap.token);
    println!("totp_secret={totp_secret}");
    println!("otpauth_uri={otpauth_uri}");
    println!("expires_in=15 minutes");
    pool.close().await;
    Ok(())
}

/// Checks the local HTTP listener without requiring tooling in the runtime image.
pub fn healthcheck() -> anyhow::Result<()> {
    let mut stream =
        TcpStream::connect_timeout(&"127.0.0.1:8080".parse()?, StdDuration::from_secs(3))?;
    stream.set_read_timeout(Some(StdDuration::from_secs(3)))?;
    stream.set_write_timeout(Some(StdDuration::from_secs(3)))?;
    stream.write_all(
        b"GET /health/ready HTTP/1.1\r\nHost: api.kamori.app\r\nConnection: close\r\n\r\n",
    )?;
    let mut response = [0_u8; 64];
    let read = stream.read(&mut response)?;
    let status_line = std::str::from_utf8(&response[..read])?
        .lines()
        .next()
        .unwrap_or_default();
    anyhow::ensure!(
        status_line.contains(" 200 "),
        "readiness returned {status_line:?}"
    );
    Ok(())
}
