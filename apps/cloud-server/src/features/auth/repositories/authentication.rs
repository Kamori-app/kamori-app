//! Durable authorization carried by deferred authentication and enrollment.
use crate::features::common::{ApiError, internal_error, unauthenticated};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct AuthenticationBinding {
    pub(crate) user_id: Uuid,
    pub(crate) generation: i64,
    pub(crate) session_id: Option<Uuid>,
}

impl AuthenticationBinding {
    /// Acquire the same account lock as password reset, then the authorizing
    /// session lock. Keep this transaction open through the protected write.
    pub(crate) async fn lock(self, tx: &mut Transaction<'_, Postgres>) -> Result<(), ApiError> {
        let generation: Option<i64> = sqlx::query_scalar(
            "SELECT auth_generation FROM users WHERE id = $1 AND deleted_at IS NULL AND suspended_at IS NULL FOR NO KEY UPDATE")
            .bind(self.user_id).fetch_optional(&mut **tx).await.map_err(internal_error)?;
        if generation != Some(self.generation) {
            return Err(unauthenticated("authentication credentials have changed"));
        }
        if let Some(session_id) = self.session_id {
            let active: Option<Uuid> = sqlx::query_scalar(
                "SELECT id FROM refresh_tokens WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL AND expires_at > now() AND (device_id IS NULL OR EXISTS(SELECT 1 FROM devices d WHERE d.id = refresh_tokens.device_id AND d.user_id = refresh_tokens.user_id AND d.status = 'active')) FOR UPDATE")
                .bind(session_id).bind(self.user_id).fetch_optional(&mut **tx).await.map_err(internal_error)?;
            if active.is_none() {
                return Err(unauthenticated("authorizing session is no longer active"));
            }
        }
        Ok(())
    }
}

pub(crate) async fn session_authentication_binding(
    pool: &PgPool,
    user_id: Uuid,
    session_id: Uuid,
) -> Result<AuthenticationBinding, ApiError> {
    let generation = sqlx::query_scalar("SELECT auth_generation FROM users WHERE id = $1 AND deleted_at IS NULL AND suspended_at IS NULL")
        .bind(user_id).fetch_optional(pool).await.map_err(internal_error)?
        .ok_or_else(|| unauthenticated("account is unavailable"))?;
    let binding = AuthenticationBinding {
        user_id,
        generation,
        session_id: Some(session_id),
    };
    let mut tx = pool.begin().await.map_err(internal_error)?;
    binding.lock(&mut tx).await?;
    tx.commit().await.map_err(internal_error)?;
    Ok(binding)
}

/// Serialize account deletion, device revocation and recovery before either
/// space/key-package or account row locks. Deletion must retain its existing
/// space-before-account quota order; this lock prevents security mutations from
/// holding the account while waiting for a key package deletion already holds.
pub(crate) async fn lock_account_security_mutation(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<(), ApiError> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 913))")
        .bind(user_id.to_string())
        .execute(&mut **tx)
        .await
        .map_err(internal_error)?;
    Ok(())
}
