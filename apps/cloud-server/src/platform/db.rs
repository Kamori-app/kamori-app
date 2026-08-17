/// Postgres database utilities for the cloud server.
use anyhow::{Context, Result};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

/// Creates a Postgres connection pool.
pub async fn create_pool(database_url: &str, database_max_connections: u32) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(database_max_connections)
        .connect(database_url)
        .await
        .with_context(|| format!("failed to connect to database: {database_url}"))?;
    Ok(pool)
}

/// Ensures the database URL uses the Postgres scheme.
pub fn assert_postgres_url(database_url: &str) -> Result<()> {
    if database_url.starts_with("postgres://") || database_url.starts_with("postgresql://") {
        Ok(())
    } else {
        anyhow::bail!("database_url must be a postgres URL")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_postgres_url_accepts_postgres_scheme() {
        assert!(assert_postgres_url("postgres://localhost:5432/kamori").is_ok());
        assert!(assert_postgres_url("postgresql://localhost:5432/kamori").is_ok());
    }

    #[test]
    fn assert_postgres_url_rejects_other_schemes() {
        assert!(assert_postgres_url("sqlite://kamori.db").is_err());
        assert!(assert_postgres_url("http://example.com").is_err());
    }
}
