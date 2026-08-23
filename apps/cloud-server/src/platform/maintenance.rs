//! Durable cleanup for expired trash and external ciphertext objects.

use std::time::Duration;

use object_store::{ObjectStoreExt, path::Path};
use sqlx::Row;
use uuid::Uuid;

use crate::platform::state::AppState;

const MAINTENANCE_INTERVAL: Duration = Duration::from_secs(60);

/// Runs idempotent maintenance. Multiple app nodes coordinate through PostgreSQL.
pub async fn run(state: AppState) {
    let mut interval = tokio::time::interval(MAINTENANCE_INTERVAL);
    loop {
        interval.tick().await;
        if let Err(error) = record_heartbeat(&state, "running", false, None).await {
            tracing::warn!(%error, "maintenance heartbeat start failed");
        }
        if let Err(error) = prepare_deletions(&state).await {
            tracing::error!(%error, "maintenance preparation failed");
            let _ = record_heartbeat(&state, "failed", false, Some(&error.to_string())).await;
            continue;
        }
        if let Err(error) = cleanup_stale_downloads(&state).await {
            tracing::error!(%error, "stale blob reservation cleanup failed");
            let _ = record_heartbeat(&state, "failed", false, Some(&error.to_string())).await;
            continue;
        }
        let mut failed = None;
        for _ in 0..100 {
            match delete_one_object(&state).await {
                Ok(true) => {}
                Ok(false) => break,
                Err(error) => {
                    tracing::error!(%error, "ciphertext deletion attempt failed");
                    failed = Some(error.to_string());
                    break;
                }
            }
        }
        let heartbeat = match failed.as_deref() {
            Some(error) => record_heartbeat(&state, "failed", false, Some(error)).await,
            None => record_heartbeat(&state, "ok", true, None).await,
        };
        if let Err(error) = heartbeat {
            tracing::warn!(%error, "maintenance heartbeat completion failed");
        }
    }
}

async fn cleanup_stale_downloads(state: &AppState) -> anyhow::Result<()> {
    let mut coordinator = state.pool.acquire().await?;
    let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock(44200620)")
        .fetch_one(&mut *coordinator)
        .await?;
    if !locked {
        return Ok(());
    }
    let cleanup = async {
        crate::features::cas::repositories::expire_stale_download_reservations(&state.pool).await?;
        sqlx::query(
            r#"
            WITH expired AS (
                SELECT token_hash
                FROM account_recovery_attempts
                WHERE (completed_at IS NULL AND expires_at < now() - interval '1 day')
                   OR completed_at < now() - interval '30 days'
                ORDER BY COALESCE(completed_at, expires_at)
                LIMIT 1000
            )
            DELETE FROM account_recovery_attempts attempt
            USING expired
            WHERE attempt.token_hash = expired.token_hash
            "#,
        )
        .execute(&state.pool)
        .await?;
        Ok::<(), anyhow::Error>(())
    }
    .await;
    let unlock = sqlx::query_scalar::<_, bool>("SELECT pg_advisory_unlock(44200620)")
        .fetch_one(&mut *coordinator)
        .await;
    if let Err(error) = unlock {
        tracing::warn!(%error, "stale reservation cleanup lock release failed");
    }
    cleanup
}

async fn record_heartbeat(
    state: &AppState,
    status: &str,
    succeeded: bool,
    error: Option<&str>,
) -> anyhow::Result<()> {
    let mut message = error.unwrap_or_default().to_string();
    message.truncate(1_000);
    sqlx::query(
        r#"
        INSERT INTO operator_job_heartbeats (
            job_name, status, details, last_started_at, last_succeeded_at
        ) VALUES (
            'object_cleanup', $1,
            CASE WHEN $2 = '' THEN '{}'::jsonb ELSE jsonb_build_object('error', $2::text) END,
            now(), CASE WHEN $3 THEN now() ELSE NULL END
        )
        ON CONFLICT (job_name) DO UPDATE SET
            status = EXCLUDED.status,
            details = EXCLUDED.details,
            last_started_at = CASE WHEN EXCLUDED.status = 'running'
                                   THEN now() ELSE operator_job_heartbeats.last_started_at END,
            last_succeeded_at = CASE WHEN $3 THEN now()
                                     ELSE operator_job_heartbeats.last_succeeded_at END,
            updated_at = now()
        "#,
    )
    .bind(status)
    .bind(message)
    .bind(succeeded)
    .execute(&state.pool)
    .await?;
    Ok(())
}

async fn prepare_deletions(state: &AppState) -> anyhow::Result<()> {
    let mut tx = state.pool.begin().await?;
    let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(44200619)")
        .fetch_one(&mut *tx)
        .await?;
    if !locked {
        tx.rollback().await?;
        return Ok(());
    }
    sqlx::query(
        r#"
        INSERT INTO object_deletion_queue (id, object_key)
        SELECT gen_random_uuid(), blob.object_key
        FROM space_blobs blob
        JOIN security_spaces space ON space.id = blob.space_id
        WHERE space.status = 'deleted'
          AND space.deleted_at <= now() - interval '30 days'
        ON CONFLICT (object_key) DO NOTHING
        "#,
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "DELETE FROM security_spaces WHERE status = 'deleted' AND deleted_at <= now() - interval '30 days'",
    )
    .execute(&mut *tx)
    .await?;

    // Delete metadata and enqueue its exact, never-reused object key as one
    // statement. A concurrent retry refreshes the row lease under a row lock,
    // causing PostgreSQL to re-evaluate the DELETE predicate after waiting.
    sqlx::query(
        r#"
        WITH expired AS (
            DELETE FROM space_blobs
            WHERE status = 'pending'
              AND created_at <= now() - interval '24 hours'
              AND COALESCE(upload_lease_until, created_at) <= now()
            RETURNING object_key
        )
        INSERT INTO object_deletion_queue (id, object_key)
        SELECT gen_random_uuid(), object_key FROM expired
        ON CONFLICT (object_key) DO NOTHING
        "#,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn delete_one_object(state: &AppState) -> anyhow::Result<bool> {
    let mut tx = state.pool.begin().await?;
    let row = sqlx::query(
        r#"
        SELECT id, object_key
        FROM object_deletion_queue
        WHERE last_attempt_at IS NULL OR last_attempt_at <= now() - interval '5 minutes'
        ORDER BY requested_at ASC
        FOR UPDATE SKIP LOCKED
        LIMIT 1
        "#,
    )
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        tx.rollback().await?;
        return Ok(false);
    };
    let id: Uuid = row.try_get("id")?;
    let object_key: String = row.try_get("object_key")?;
    sqlx::query(
        "UPDATE object_deletion_queue SET attempts = attempts + 1, last_attempt_at = now() WHERE id = $1",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    let path = Path::parse(&object_key)?;
    match state.object_store.delete(&path).await {
        Ok(()) | Err(object_store::Error::NotFound { .. }) => {
            sqlx::query("DELETE FROM object_deletion_queue WHERE id = $1")
                .bind(id)
                .execute(&state.pool)
                .await?;
            Ok(true)
        }
        Err(error) => {
            let mut message = error.to_string();
            message.truncate(2_000);
            sqlx::query("UPDATE object_deletion_queue SET last_error = $2 WHERE id = $1")
                .bind(id)
                .bind(message)
                .execute(&state.pool)
                .await?;
            Err(error.into())
        }
    }
}
