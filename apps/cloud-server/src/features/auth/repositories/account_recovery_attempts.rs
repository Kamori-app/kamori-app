//! Durable, retry-safe account-recovery state transitions.

use sqlx::{PgPool, Row};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::features::common::{ApiError, internal_error, unauthenticated};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AccountRecoveryResetOutcome {
    Applied,
    AlreadyApplied,
}

pub(crate) async fn create_account_recovery_attempt(
    pool: &PgPool,
    token_hash: &[u8; 32],
    user_id: Uuid,
    expires_at: OffsetDateTime,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        INSERT INTO account_recovery_attempts (token_hash, user_id, expires_at)
        VALUES ($1, $2, $3)
        "#,
    )
    .bind(token_hash.as_slice())
    .bind(user_id)
    .bind(expires_at)
    .execute(pool)
    .await
    .map_err(internal_error)?;
    Ok(())
}

/// Resolves the bearer token hash recorded when a recovery flow started.
///
/// This durable lookup deliberately remains available after `expires_at` so
/// an already-completed request can be retried after a lost response. The
/// transactional reset still rejects an expired attempt that was not completed.
pub(crate) async fn find_account_recovery_attempt_user(
    pool: &PgPool,
    token_hash: &[u8; 32],
) -> Result<Option<Uuid>, ApiError> {
    sqlx::query_scalar(
        r#"
        SELECT user_id
        FROM account_recovery_attempts
        WHERE token_hash = $1
        "#,
    )
    .bind(token_hash.as_slice())
    .fetch_optional(pool)
    .await
    .map_err(internal_error)
}

pub(crate) struct AccountRecoveryReset<'a> {
    pub(crate) user_id: Uuid,
    pub(crate) token_hash: &'a [u8; 32],
    pub(crate) request_hash: &'a [u8; 32],
    pub(crate) opaque_record: &'a [u8],
    pub(crate) encrypted_master_key: &'a [u8],
}

pub(crate) async fn apply_account_recovery_reset(
    pool: &PgPool,
    reset: AccountRecoveryReset<'_>,
) -> Result<AccountRecoveryResetOutcome, ApiError> {
    let now = OffsetDateTime::now_utc();
    let mut tx = pool.begin().await.map_err(internal_error)?;
    let attempt = sqlx::query(
        r#"
        SELECT user_id, request_hash, expires_at, completed_at
        FROM account_recovery_attempts
        WHERE token_hash = $1
        FOR UPDATE
        "#,
    )
    .bind(reset.token_hash.as_slice())
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| unauthenticated("account recovery token was already used or expired"))?;
    let stored_user_id: Uuid = attempt.try_get("user_id").map_err(internal_error)?;
    let expires_at: OffsetDateTime = attempt.try_get("expires_at").map_err(internal_error)?;
    let stored_request_hash: Option<Vec<u8>> =
        attempt.try_get("request_hash").map_err(internal_error)?;
    let completed_at: Option<OffsetDateTime> =
        attempt.try_get("completed_at").map_err(internal_error)?;
    if stored_user_id != reset.user_id {
        return Err(unauthenticated(
            "account recovery token was already used or expired",
        ));
    }
    if stored_request_hash
        .as_deref()
        .is_some_and(|stored| stored != reset.request_hash.as_slice())
    {
        return Err(unauthenticated(
            "account recovery token is bound to another request",
        ));
    }
    if completed_at.is_some() {
        tx.commit().await.map_err(internal_error)?;
        return Ok(AccountRecoveryResetOutcome::AlreadyApplied);
    }
    if expires_at <= now {
        return Err(unauthenticated(
            "account recovery token was already used or expired",
        ));
    }

    sqlx::query("UPDATE account_recovery_attempts SET request_hash = $2 WHERE token_hash = $1")
        .bind(reset.token_hash.as_slice())
        .bind(reset.request_hash.as_slice())
        .execute(&mut *tx)
        .await
        .map_err(internal_error)?;

    let updated = sqlx::query(
        "UPDATE users SET opaque_record = $2, encrypted_master_key = $3, totp_secret_ciphertext = NULL WHERE id = $1 AND deleted_at IS NULL AND suspended_at IS NULL",
    )
    .bind(reset.user_id)
    .bind(reset.opaque_record)
    .bind(reset.encrypted_master_key)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;
    if updated.rows_affected() == 0 {
        return Err(unauthenticated("user not found"));
    }

    sqlx::query(
        "UPDATE refresh_tokens SET revoked_at = COALESCE(revoked_at, $2) WHERE user_id = $1",
    )
    .bind(reset.user_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;
    sqlx::query("DELETE FROM user_passkeys WHERE user_id = $1")
        .bind(reset.user_id)
        .execute(&mut *tx)
        .await
        .map_err(internal_error)?;
    sqlx::query(
        "UPDATE devices SET status = 'revoked', revoked_at = COALESCE(revoked_at, $2) WHERE user_id = $1 AND status = 'active'",
    )
    .bind(reset.user_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;
    sqlx::query("DELETE FROM security_space_device_keys WHERE user_id = $1")
        .bind(reset.user_id)
        .execute(&mut *tx)
        .await
        .map_err(internal_error)?;
    sqlx::query("DELETE FROM account_recovery_codes WHERE user_id = $1")
        .bind(reset.user_id)
        .execute(&mut *tx)
        .await
        .map_err(internal_error)?;
    sqlx::query("UPDATE account_recovery_attempts SET completed_at = $2 WHERE token_hash = $1")
        .bind(reset.token_hash.as_slice())
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(internal_error)?;
    tx.commit().await.map_err(internal_error)?;
    Ok(AccountRecoveryResetOutcome::Applied)
}
