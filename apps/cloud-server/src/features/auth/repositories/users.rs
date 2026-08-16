//! Repository functions for the `users` table.

use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::features::common::{ApiError, internal_error, unauthenticated};

pub(crate) struct UserRow {
    pub(crate) id: Uuid,
    pub(crate) username: String,
    pub(crate) opaque_record: Option<Vec<u8>>,
    pub(crate) totp_secret_ciphertext: Option<Vec<u8>>,
    pub(crate) encrypted_master_key: Vec<u8>,
    pub(crate) public_key_bundle: Vec<u8>,
}

pub(crate) enum UserAdmissionResult {
    Inserted,
    CapacityReached,
}

pub(crate) struct NewUser<'a> {
    pub(crate) id: Uuid,
    pub(crate) username: &'a str,
    pub(crate) opaque_record: &'a [u8],
    pub(crate) encrypted_master_key: &'a [u8],
    pub(crate) public_key_bundle: &'a [u8],
    pub(crate) recovery_verifier_hash: &'a [u8],
}

/// Fetches a user row by username.
pub(crate) async fn get_user_by_username(
    pool: &PgPool,
    username: &str,
) -> Result<UserRow, ApiError> {
    let sql = r#"SELECT id, username, opaque_record, totp_secret_ciphertext, encrypted_master_key, public_key_bundle
               FROM users
               WHERE username = $1 AND deleted_at IS NULL AND suspended_at IS NULL"#;

    let row = sqlx::query(sql)
        .bind(username)
        .fetch_optional(pool)
        .await
        .map_err(internal_error)?;

    let row = row.ok_or_else(|| unauthenticated("user not found"))?;
    map_user_row(&row)
}

pub(super) fn map_user_row(row: &sqlx::postgres::PgRow) -> Result<UserRow, ApiError> {
    let id: Uuid = row.try_get("id").map_err(internal_error)?;
    let username: String = row.try_get("username").map_err(internal_error)?;
    let opaque_record: Option<Vec<u8>> = row.try_get("opaque_record").ok();
    let totp_secret_ciphertext: Option<Vec<u8>> = row.try_get("totp_secret_ciphertext").ok();
    let encrypted_master_key: Vec<u8> = row
        .try_get("encrypted_master_key")
        .map_err(internal_error)?;
    let public_key_bundle: Vec<u8> = row.try_get("public_key_bundle").map_err(internal_error)?;

    Ok(UserRow {
        id,
        username,
        opaque_record,
        totp_secret_ciphertext,
        encrypted_master_key,
        public_key_bundle,
    })
}

pub(crate) async fn insert_user_with_admission_cap(
    pool: &PgPool,
    user: NewUser<'_>,
    account_limit: u64,
) -> anyhow::Result<UserAdmissionResult> {
    let account_limit = i64::try_from(account_limit)?;
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(44200617)")
        .execute(&mut *tx)
        .await?;
    let active_accounts: i64 =
        sqlx::query_scalar("SELECT count(*)::bigint FROM users WHERE deleted_at IS NULL")
            .fetch_one(&mut *tx)
            .await?;
    if active_accounts >= account_limit {
        tx.rollback().await?;
        return Ok(UserAdmissionResult::CapacityReached);
    }
    sqlx::query(
        r#"
        INSERT INTO users (
            id, username, opaque_record, encrypted_master_key,
            public_key_bundle, recovery_verifier_hash
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(user.id)
    .bind(user.username)
    .bind(user.opaque_record)
    .bind(user.encrypted_master_key)
    .bind(user.public_key_bundle)
    .bind(user.recovery_verifier_hash)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(UserAdmissionResult::Inserted)
}

pub(crate) async fn find_user_for_data_recovery(
    pool: &PgPool,
    username: &str,
    recovery_verifier_hash: &[u8],
) -> Result<Option<Uuid>, ApiError> {
    sqlx::query_scalar(
        r#"
        SELECT id
        FROM users
        WHERE username = $1
          AND recovery_verifier_hash = $2
          AND deleted_at IS NULL
          AND suspended_at IS NULL
        "#,
    )
    .bind(username)
    .bind(recovery_verifier_hash)
    .fetch_optional(pool)
    .await
    .map_err(internal_error)
}

pub(crate) async fn get_user_totp_ciphertext_by_id(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Option<Vec<u8>>, ApiError> {
    let row = sqlx::query("SELECT totp_secret_ciphertext FROM users WHERE id = $1 AND deleted_at IS NULL AND suspended_at IS NULL")
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .map_err(internal_error)?;
    let row = row.ok_or_else(|| unauthenticated("user not found"))?;
    row.try_get("totp_secret_ciphertext")
        .map_err(internal_error)
}

pub(crate) async fn clear_totp_for_user(pool: &PgPool, user_id: Uuid) -> Result<(), ApiError> {
    let result =
        sqlx::query("UPDATE users SET totp_secret_ciphertext = NULL WHERE id = $1 AND deleted_at IS NULL AND suspended_at IS NULL")
            .bind(user_id)
            .execute(pool)
            .await
            .map_err(internal_error)?;
    if result.rows_affected() == 0 {
        return Err(unauthenticated("user not found"));
    }
    Ok(())
}
