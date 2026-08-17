//! Repository functions for the `account_recovery_codes` table.

use sqlx::{PgPool, QueryBuilder};
use uuid::Uuid;

use crate::features::common::{ApiError, internal_error, unauthenticated};

pub(crate) async fn count_unused_recovery_codes(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<u32, ApiError> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM account_recovery_codes WHERE user_id = $1 AND used_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(internal_error)?;
    Ok(count.max(0) as u32)
}

async fn replace_recovery_codes_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
    recovery_code_rows: &[(Uuid, Vec<u8>)],
) -> Result<(), ApiError> {
    if recovery_code_rows.is_empty() {
        sqlx::query("DELETE FROM account_recovery_codes WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut **tx)
            .await
            .map_err(internal_error)?;
        return Ok(());
    }

    let mut builder = QueryBuilder::<sqlx::Postgres>::new(
        "WITH deleted AS (DELETE FROM account_recovery_codes WHERE user_id = ",
    );
    builder.push_bind(user_id);
    builder.push(" ) INSERT INTO account_recovery_codes (id, user_id, code_hash) ");
    builder.push_values(recovery_code_rows.iter(), |mut b, (code_id, code_hash)| {
        b.push_bind(*code_id)
            .push_bind(user_id)
            .push_bind(code_hash);
    });
    builder
        .build()
        .execute(&mut **tx)
        .await
        .map_err(internal_error)?;

    Ok(())
}

pub(crate) async fn enable_totp_for_user_with_recovery_codes(
    pool: &PgPool,
    user_id: Uuid,
    totp_secret_ciphertext: &[u8],
    recovery_code_rows: &[(Uuid, Vec<u8>)],
) -> Result<bool, ApiError> {
    let mut tx = pool.begin().await.map_err(internal_error)?;
    let result = sqlx::query(
        "UPDATE users SET totp_secret_ciphertext = $2 WHERE id = $1 AND deleted_at IS NULL AND suspended_at IS NULL AND totp_secret_ciphertext IS NULL",
    )
    .bind(user_id)
    .bind(totp_secret_ciphertext)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;
    if result.rows_affected() == 0 {
        tx.rollback().await.map_err(internal_error)?;
        return Ok(false);
    }

    replace_recovery_codes_in_tx(&mut tx, user_id, recovery_code_rows).await?;
    tx.commit().await.map_err(internal_error)?;
    Ok(true)
}

pub(crate) async fn regenerate_recovery_codes_for_user(
    pool: &PgPool,
    user_id: Uuid,
    recovery_code_rows: &[(Uuid, Vec<u8>)],
) -> Result<(), ApiError> {
    let mut tx = pool.begin().await.map_err(internal_error)?;
    let user_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM users WHERE id = $1 AND deleted_at IS NULL AND suspended_at IS NULL)",
    )
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?;
    if !user_exists {
        tx.rollback().await.map_err(internal_error)?;
        return Err(unauthenticated("user not found"));
    }

    replace_recovery_codes_in_tx(&mut tx, user_id, recovery_code_rows).await?;
    tx.commit().await.map_err(internal_error)?;
    Ok(())
}

pub(crate) async fn consume_totp_backup_code(
    pool: &PgPool,
    user_id: Uuid,
    code_hash: &[u8],
) -> Result<bool, ApiError> {
    let result = sqlx::query(
        r#"
        UPDATE account_recovery_codes
        SET used_at = now()
        WHERE user_id = $1 AND code_hash = $2 AND used_at IS NULL
        "#,
    )
    .bind(user_id)
    .bind(code_hash)
    .execute(pool)
    .await
    .map_err(internal_error)?;
    Ok(result.rows_affected() == 1)
}
