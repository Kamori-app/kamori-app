//! Repository functions for user account mutations.

use sqlx::PgPool;
use sqlx::Row;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::features::users::dto::{
    ConsentSettings, DeletionStatusResponse, UpdateConsentSettingsRequest,
};

const CONSENT_POLICY_VERSION: i32 = 1;

pub(crate) async fn get_consents(pool: &PgPool, user_id: Uuid) -> anyhow::Result<ConsentSettings> {
    let row = sqlx::query(
        r#"
        SELECT product_analytics, crash_reports, marketing, policy_version, updated_at
        FROM user_consents
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(ConsentSettings {
            product_analytics: false,
            crash_reports: false,
            marketing: false,
            policy_version: CONSENT_POLICY_VERSION as u32,
            updated_at_unix_ms: None,
        });
    };
    let updated_at: OffsetDateTime = row.try_get("updated_at")?;
    Ok(ConsentSettings {
        product_analytics: row.try_get("product_analytics")?,
        crash_reports: row.try_get("crash_reports")?,
        marketing: row.try_get("marketing")?,
        policy_version: u32::try_from(row.try_get::<i32, _>("policy_version")?)?,
        updated_at_unix_ms: Some(updated_at.unix_timestamp_nanos() as i64 / 1_000_000),
    })
}

pub(crate) async fn update_consents(
    pool: &PgPool,
    user_id: Uuid,
    request: &UpdateConsentSettingsRequest,
) -> anyhow::Result<ConsentSettings> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query(
        r#"
        INSERT INTO user_consents (
            user_id, product_analytics, crash_reports, marketing, policy_version,
            product_analytics_updated_at, crash_reports_updated_at,
            marketing_updated_at, updated_at
        ) VALUES ($1, $2, $3, $4, $5, now(), now(), now(), now())
        ON CONFLICT (user_id) DO UPDATE SET
            product_analytics_updated_at = CASE
                WHEN user_consents.product_analytics IS DISTINCT FROM EXCLUDED.product_analytics
                THEN now() ELSE user_consents.product_analytics_updated_at END,
            crash_reports_updated_at = CASE
                WHEN user_consents.crash_reports IS DISTINCT FROM EXCLUDED.crash_reports
                THEN now() ELSE user_consents.crash_reports_updated_at END,
            marketing_updated_at = CASE
                WHEN user_consents.marketing IS DISTINCT FROM EXCLUDED.marketing
                THEN now() ELSE user_consents.marketing_updated_at END,
            product_analytics = EXCLUDED.product_analytics,
            crash_reports = EXCLUDED.crash_reports,
            marketing = EXCLUDED.marketing,
            policy_version = EXCLUDED.policy_version,
            updated_at = now()
        RETURNING product_analytics, crash_reports, marketing, policy_version, updated_at
        "#,
    )
    .bind(user_id)
    .bind(request.product_analytics)
    .bind(request.crash_reports)
    .bind(request.marketing)
    .bind(CONSENT_POLICY_VERSION)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        r#"
        INSERT INTO user_consent_audit (
            id, user_id, product_analytics, crash_reports, marketing, policy_version
        ) VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(request.product_analytics)
    .bind(request.crash_reports)
    .bind(request.marketing)
    .bind(CONSENT_POLICY_VERSION)
    .execute(&mut *tx)
    .await?;
    let updated_at: OffsetDateTime = row.try_get("updated_at")?;
    let response = ConsentSettings {
        product_analytics: row.try_get("product_analytics")?,
        crash_reports: row.try_get("crash_reports")?,
        marketing: row.try_get("marketing")?,
        policy_version: u32::try_from(row.try_get::<i32, _>("policy_version")?)?,
        updated_at_unix_ms: Some(updated_at.unix_timestamp_nanos() as i64 / 1_000_000),
    };
    tx.commit().await?;
    Ok(response)
}

pub(crate) async fn deletion_status(
    pool: &PgPool,
    user_id: Uuid,
) -> anyhow::Result<DeletionStatusResponse> {
    let row = sqlx::query(
        r#"
        SELECT
            (SELECT count(*)::bigint
             FROM workspaces workspace
             WHERE workspace.owner_user_id = $1
               AND workspace.deleted_at IS NULL
               AND EXISTS (
                   SELECT 1
                   FROM workspace_members member
                   WHERE member.workspace_id = workspace.id
                     AND member.user_id <> $1 AND member.status = 'active'
                   UNION ALL
                   SELECT 1
                   FROM security_spaces space
                   WHERE space.workspace_id = workspace.id
                     AND space.status IN ('active', 'deleted')
                     AND (
                         space.owner_user_id <> $1 OR EXISTS (
                             SELECT 1 FROM security_space_members space_member
                             WHERE space_member.space_id = space.id
                               AND space_member.user_id <> $1
                               AND space_member.status = 'active'
                         )
                     )
               )) AS shared_workspaces_owned,
            (SELECT count(*)::bigint
             FROM security_spaces space
             WHERE space.owner_user_id = $1
               AND space.status IN ('active', 'deleted')
               AND EXISTS (
                   SELECT 1 FROM security_space_members member
                   WHERE member.space_id = space.id
                     AND member.user_id <> $1
                     AND member.status = 'active'
               )) AS shared_spaces_owned
        "#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    let shared_workspaces_owned = u64::try_from(row.try_get::<i64, _>("shared_workspaces_owned")?)?;
    let shared_spaces_owned = u64::try_from(row.try_get::<i64, _>("shared_spaces_owned")?)?;
    Ok(DeletionStatusResponse {
        can_delete: shared_workspaces_owned == 0 && shared_spaces_owned == 0,
        shared_workspaces_owned,
        shared_spaces_owned,
    })
}

pub(crate) async fn delete_user(pool: &PgPool, user_id: Uuid) -> anyhow::Result<bool> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 913))")
        .bind(user_id.to_string())
        .execute(&mut *tx)
        .await?;
    let blockers = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM workspaces workspace
            WHERE workspace.owner_user_id = $1
              AND workspace.deleted_at IS NULL
              AND (
                  EXISTS (
                      SELECT 1 FROM workspace_members member
                      WHERE member.workspace_id = workspace.id
                        AND member.user_id <> $1 AND member.status = 'active'
                  ) OR EXISTS (
                      SELECT 1 FROM security_spaces space
                      WHERE space.workspace_id = workspace.id
                        AND space.status IN ('active', 'deleted')
                        AND (
                            space.owner_user_id <> $1 OR EXISTS (
                                SELECT 1 FROM security_space_members space_member
                                WHERE space_member.space_id = space.id
                                  AND space_member.user_id <> $1
                                  AND space_member.status = 'active'
                            )
                        )
                  )
              )
        ) OR EXISTS(
            SELECT 1
            FROM security_spaces space
            JOIN security_space_members member ON member.space_id = space.id
            WHERE space.owner_user_id = $1
              AND space.status IN ('active', 'deleted')
              AND member.user_id <> $1
              AND member.status = 'active'
        )
        "#,
    )
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await?;
    if blockers {
        tx.rollback().await?;
        return Ok(false);
    }
    sqlx::query(
        r#"
        INSERT INTO object_deletion_queue (id, object_key)
        SELECT gen_random_uuid(), blob.object_key
        FROM space_blobs blob
        JOIN security_spaces space ON space.id = blob.space_id
        JOIN workspaces workspace ON workspace.id = space.workspace_id
        WHERE (
            space.owner_user_id = $1 AND NOT EXISTS (
                SELECT 1 FROM security_space_members member
                WHERE member.space_id = space.id
                  AND member.user_id <> $1 AND member.status = 'active'
            )
        ) OR workspace.owner_user_id = $1
        ON CONFLICT (object_key) DO NOTHING
        "#,
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        r#"
        DELETE FROM security_spaces space
        WHERE space.owner_user_id = $1
          AND NOT EXISTS (
              SELECT 1 FROM security_space_members member
              WHERE member.space_id = space.id
                AND member.user_id <> $1 AND member.status = 'active'
          )
        "#,
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM workspaces WHERE owner_user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM security_space_device_keys WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE security_space_members SET status = 'revoked', revoked_at = now() WHERE user_id = $1 AND status = 'active'",
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE workspace_members SET status = 'revoked' WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        r#"
        UPDATE space_blobs blob
        SET owner_user_id = space.owner_user_id,
            created_by = CASE WHEN blob.created_by = $1 THEN NULL ELSE blob.created_by END
        FROM security_spaces space
        WHERE blob.space_id = space.id
          AND (blob.owner_user_id = $1 OR blob.created_by = $1)
        "#,
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "DELETE FROM blob_egress_reservations WHERE owner_user_id = $1 OR requested_by = $1",
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM security_space_invites WHERE created_by = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE security_space_invites SET redeemed_by = NULL WHERE redeemed_by = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM refresh_tokens WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM user_passkeys WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM account_recovery_codes WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM user_consents WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM user_consent_audit WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM security_events WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE devices SET status = 'revoked', encrypted_name = ''::bytea, revoked_at = COALESCE(revoked_at, now()) WHERE user_id = $1",
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    let deleted = sqlx::query(
        r#"
        UPDATE users
        SET username = 'deleted-' || id::text,
            opaque_record = ''::bytea,
            encrypted_master_key = ''::bytea,
            public_key_bundle = ''::bytea,
            totp_secret_ciphertext = NULL,
            deleted_at = now()
        WHERE id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    tx.commit().await?;
    Ok(deleted)
}
