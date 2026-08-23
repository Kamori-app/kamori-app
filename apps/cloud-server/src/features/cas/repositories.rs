//! Atomic space authorization, storage admission, and egress reservations.

use sqlx::{PgPool, Postgres, Row, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

const GLOBAL_EGRESS_SCOPE_ID: Uuid = Uuid::nil();

pub(crate) struct CasRow {
    pub(crate) blob_id: Uuid,
    pub(crate) ciphertext_sha256: Vec<u8>,
    pub(crate) size_padded: i64,
    pub(crate) object_key: String,
}

pub(crate) enum StoreBlobResult {
    NeedsUpload(CasRow),
    AlreadyStored,
    AccessDenied,
    StorageQuotaExceeded,
    IdConflict,
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn store_blob(
    pool: &PgPool,
    actor_id: Uuid,
    space_id: Uuid,
    blob_id: Uuid,
    ciphertext_sha256: &[u8],
    size_padded: i64,
    account_storage_limit: i64,
) -> anyhow::Result<StoreBlobResult> {
    let mut tx = pool.begin().await?;
    let owner_id = authorized_owner(&mut tx, actor_id, space_id, true).await?;
    let Some(owner_id) = owner_id else {
        return Ok(StoreBlobResult::AccessDenied);
    };
    lock_owner_quota(&mut tx, owner_id).await?;

    if let Some(row) = sqlx::query(
        "SELECT ciphertext_sha256, size_padded, object_key, status FROM space_blobs WHERE id = $1 AND space_id = $2 FOR UPDATE",
    )
    .bind(blob_id)
    .bind(space_id)
    .fetch_optional(&mut *tx)
    .await?
    {
        let existing_hash: Vec<u8> = row.try_get("ciphertext_sha256")?;
        let existing_size: i64 = row.try_get("size_padded")?;
        if existing_hash != ciphertext_sha256 || existing_size != size_padded {
            return Ok(StoreBlobResult::IdConflict);
        }
        let status: String = row.try_get("status")?;
        let result = if status == "ready" {
            StoreBlobResult::AlreadyStored
        } else {
            sqlx::query(
                "UPDATE space_blobs SET upload_lease_until = now() + interval '15 minutes' WHERE id = $1 AND space_id = $2 AND status = 'pending'",
            )
            .bind(blob_id)
            .bind(space_id)
            .execute(&mut *tx)
            .await?;
            StoreBlobResult::NeedsUpload(CasRow {
                blob_id,
                ciphertext_sha256: existing_hash,
                size_padded: existing_size,
                object_key: row.try_get("object_key")?,
            })
        };
        tx.commit().await?;
        return Ok(result);
    }

    let stored_bytes: i64 =
        sqlx::query_scalar("SELECT blob_storage_bytes FROM users WHERE id = $1 FOR UPDATE")
            .bind(owner_id)
            .fetch_one(&mut *tx)
            .await?;
    if stored_bytes.saturating_add(size_padded) > account_storage_limit {
        return Ok(StoreBlobResult::StorageQuotaExceeded);
    }

    // A storage object key is never reused. This makes an already-claimed
    // deletion queue entry harmless if a client later reuses the same
    // space-scoped blob id after stale metadata was collected.
    let object_key = format!("spaces/{space_id}/blobs/{blob_id}/{}", Uuid::new_v4());
    sqlx::query(
        r#"
        INSERT INTO space_blobs (
            id, space_id, owner_user_id, created_by, ciphertext_sha256, size_padded,
            object_key, upload_lease_until
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, now() + interval '15 minutes')
        "#,
    )
    .bind(blob_id)
    .bind(space_id)
    .bind(owner_id)
    .bind(actor_id)
    .bind(ciphertext_sha256)
    .bind(size_padded)
    .bind(&object_key)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(StoreBlobResult::NeedsUpload(CasRow {
        blob_id,
        ciphertext_sha256: ciphertext_sha256.to_vec(),
        size_padded,
        object_key,
    }))
}

pub(crate) async fn mark_blob_ready(
    pool: &PgPool,
    space_id: Uuid,
    blob_id: Uuid,
    ciphertext_sha256: &[u8],
) -> anyhow::Result<()> {
    let updated = sqlx::query(
        r#"
        UPDATE space_blobs
        SET status = 'ready', upload_lease_until = NULL
        WHERE id = $1 AND space_id = $2 AND ciphertext_sha256 = $3 AND status = 'pending'
        "#,
    )
    .bind(blob_id)
    .bind(space_id)
    .bind(ciphertext_sha256)
    .execute(pool)
    .await?;
    if updated.rows_affected() == 0 {
        let ready: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM space_blobs WHERE id = $1 AND space_id = $2 AND ciphertext_sha256 = $3 AND status = 'ready')",
        )
        .bind(blob_id)
        .bind(space_id)
        .bind(ciphertext_sha256)
        .fetch_one(pool)
        .await?;
        anyhow::ensure!(ready, "blob metadata disappeared before upload completed");
    }
    Ok(())
}

pub(crate) enum ReserveDownloadResult {
    Reserved(DownloadReservation),
    AccessDenied,
    NotFound,
    OwnerQuotaExceeded,
    ConcurrentLimitExceeded,
    GlobalConcurrentLimitExceeded,
    GlobalQuotaExceeded,
    EmergencyBreakerExceeded,
}

pub(crate) struct DownloadReservation {
    pub(crate) id: Uuid,
    pub(crate) blob: CasRow,
}

pub(crate) struct EgressLimits {
    pub(crate) owner_monthly: i64,
    pub(crate) owner_rolling_24h: i64,
    pub(crate) global_nonessential_stop: i64,
    pub(crate) global_emergency_breaker: i64,
    pub(crate) owner_concurrent_downloads: i64,
    pub(crate) global_concurrent_downloads: i64,
}

pub(crate) async fn reserve_download(
    pool: &PgPool,
    actor_id: Uuid,
    space_id: Uuid,
    blob_id: Uuid,
    limits: EgressLimits,
) -> anyhow::Result<ReserveDownloadResult> {
    let mut tx = pool.begin().await?;
    let owner_id = authorized_owner(&mut tx, actor_id, space_id, false).await?;
    let Some(owner_id) = owner_id else {
        return Ok(ReserveDownloadResult::AccessDenied);
    };
    lock_owner_quota(&mut tx, owner_id).await?;
    let active_downloads: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*)::bigint FROM blob_egress_reservations
        WHERE owner_user_id = $1
          AND completed_at IS NULL
          AND reserved_at >= now() - interval '1 hour'
        "#,
    )
    .bind(owner_id)
    .fetch_one(&mut *tx)
    .await?;
    if active_downloads >= limits.owner_concurrent_downloads {
        return Ok(ReserveDownloadResult::ConcurrentLimitExceeded);
    }
    let row = sqlx::query(
        r#"
        SELECT id, ciphertext_sha256, size_padded, object_key
        FROM space_blobs
        WHERE id = $1 AND space_id = $2 AND status = 'ready'
        "#,
    )
    .bind(blob_id)
    .bind(space_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        return Ok(ReserveDownloadResult::NotFound);
    };
    let blob = CasRow {
        blob_id: row.try_get("id")?,
        ciphertext_sha256: row.try_get("ciphertext_sha256")?,
        size_padded: row.try_get("size_padded")?,
        object_key: row.try_get("object_key")?,
    };

    // A month-independent lock keeps global concurrency exact even while
    // reservations straddle a UTC month boundary.
    sqlx::query("SELECT pg_advisory_xact_lock(44200621)")
        .execute(&mut *tx)
        .await?;

    let windows = sqlx::query(
        r#"
        SELECT
            date_trunc('month', now() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC' AS month_start,
            date_bin(interval '15 minutes', now(), timestamptz '2000-01-01 00:00:00+00') AS quarter_start
        "#,
    )
    .fetch_one(&mut *tx)
    .await?;
    let month_start: OffsetDateTime = windows.try_get("month_start")?;
    let quarter_start: OffsetDateTime = windows.try_get("quarter_start")?;
    let global_month = ensure_and_lock_usage_bucket(
        &mut tx,
        "global",
        GLOBAL_EGRESS_SCOPE_ID,
        "month",
        month_start,
    )
    .await?;
    // The locked global month bucket serializes cross-owner admission, making
    // this limit exact across every app node rather than per process.
    let global_active_downloads: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*)::bigint FROM blob_egress_reservations
        WHERE completed_at IS NULL
          AND reserved_at >= now() - interval '1 hour'
        "#,
    )
    .fetch_one(&mut *tx)
    .await?;
    if global_active_downloads >= limits.global_concurrent_downloads {
        return Ok(ReserveDownloadResult::GlobalConcurrentLimitExceeded);
    }
    let owner_month =
        ensure_and_lock_usage_bucket(&mut tx, "owner", owner_id, "month", month_start).await?;
    ensure_and_lock_usage_bucket(&mut tx, "owner", owner_id, "quarter_hour", quarter_start).await?;
    let owner_24h: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(sum(bytes_pending + bytes_delivered), 0)::bigint
        FROM blob_egress_usage_buckets
        WHERE scope_kind = 'owner'
          AND scope_id = $1
          AND window_kind = 'quarter_hour'
          AND window_start >= $2 - interval '24 hours'
          AND window_start <= $2
        "#,
    )
    .bind(owner_id)
    .bind(quarter_start)
    .fetch_one(&mut *tx)
    .await?;
    if owner_month.saturating_add(blob.size_padded) > limits.owner_monthly
        || owner_24h.saturating_add(blob.size_padded) > limits.owner_rolling_24h
    {
        return Ok(ReserveDownloadResult::OwnerQuotaExceeded);
    }
    let projected_global_month = global_month.saturating_add(blob.size_padded);
    if projected_global_month > limits.global_emergency_breaker {
        return Ok(ReserveDownloadResult::EmergencyBreakerExceeded);
    }
    if projected_global_month > limits.global_nonessential_stop {
        return Ok(ReserveDownloadResult::GlobalQuotaExceeded);
    }
    let reservation_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO blob_egress_reservations (
            id, owner_user_id, requested_by, space_id, blob_id, bytes_reserved
        ) VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(reservation_id)
    .bind(owner_id)
    .bind(actor_id)
    .bind(space_id)
    .bind(blob_id)
    .bind(blob.size_padded)
    .execute(&mut *tx)
    .await?;
    increment_pending_bucket(
        &mut tx,
        "global",
        GLOBAL_EGRESS_SCOPE_ID,
        "month",
        month_start,
        blob.size_padded,
    )
    .await?;
    increment_pending_bucket(
        &mut tx,
        "owner",
        owner_id,
        "month",
        month_start,
        blob.size_padded,
    )
    .await?;
    increment_pending_bucket(
        &mut tx,
        "owner",
        owner_id,
        "quarter_hour",
        quarter_start,
        blob.size_padded,
    )
    .await?;
    tx.commit().await?;
    Ok(ReserveDownloadResult::Reserved(DownloadReservation {
        id: reservation_id,
        blob,
    }))
}

pub(crate) async fn finalize_download_reservation(
    pool: &PgPool,
    reservation_id: Uuid,
    bytes_delivered: i64,
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    let reservation = sqlx::query(
        r#"
        SELECT owner_user_id, bytes_reserved, completed_at,
               date_trunc('month', reserved_at AT TIME ZONE 'UTC') AT TIME ZONE 'UTC' AS month_start,
               date_bin(interval '15 minutes', reserved_at, timestamptz '2000-01-01 00:00:00+00') AS quarter_start
        FROM blob_egress_reservations
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(reservation_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(reservation) = reservation else {
        tx.rollback().await?;
        return Ok(());
    };
    if reservation
        .try_get::<Option<OffsetDateTime>, _>("completed_at")?
        .is_some()
    {
        tx.rollback().await?;
        return Ok(());
    }
    let owner_id: Uuid = reservation.try_get("owner_user_id")?;
    let reserved: i64 = reservation.try_get("bytes_reserved")?;
    let delivered = bytes_delivered.clamp(0, reserved);
    let month_start: OffsetDateTime = reservation.try_get("month_start")?;
    let quarter_start: OffsetDateTime = reservation.try_get("quarter_start")?;

    reconcile_bucket(
        &mut tx,
        "global",
        GLOBAL_EGRESS_SCOPE_ID,
        "month",
        month_start,
        reserved,
        delivered,
    )
    .await?;
    reconcile_bucket(
        &mut tx,
        "owner",
        owner_id,
        "month",
        month_start,
        reserved,
        delivered,
    )
    .await?;
    reconcile_bucket(
        &mut tx,
        "owner",
        owner_id,
        "quarter_hour",
        quarter_start,
        reserved,
        delivered,
    )
    .await?;
    sqlx::query(
        r#"
        UPDATE blob_egress_reservations
        SET bytes_delivered = $2, completed_at = now()
        WHERE id = $1 AND completed_at IS NULL
        "#,
    )
    .bind(reservation_id)
    .bind(delivered)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Reconciles reservations left behind by a crashed or disconnected gateway.
/// Work is bounded so a normal download request cannot turn into a cleanup job.
pub(crate) async fn expire_stale_download_reservations(pool: &PgPool) -> anyhow::Result<u64> {
    let ids: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT id FROM blob_egress_reservations
        WHERE completed_at IS NULL AND reserved_at < now() - interval '1 hour'
        ORDER BY reserved_at
        LIMIT 100
        "#,
    )
    .fetch_all(pool)
    .await?;
    for id in &ids {
        // After a process crash no trustworthy byte counter survives. Charge
        // the full reservation: the object store may already have delivered
        // every byte, and releasing the reservation would make the provider
        // budget bypassable by repeated interrupted downloads.
        finalize_download_reservation(pool, *id, i64::MAX).await?;
    }
    Ok(ids.len() as u64)
}

async fn ensure_and_lock_usage_bucket(
    tx: &mut Transaction<'_, Postgres>,
    scope_kind: &str,
    scope_id: Uuid,
    window_kind: &str,
    window_start: OffsetDateTime,
) -> anyhow::Result<i64> {
    sqlx::query(
        r#"
        INSERT INTO blob_egress_usage_buckets (scope_kind, scope_id, window_kind, window_start)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(scope_kind)
    .bind(scope_id)
    .bind(window_kind)
    .bind(window_start)
    .execute(&mut **tx)
    .await?;
    let usage: i64 = sqlx::query_scalar(
        r#"
        SELECT bytes_pending + bytes_delivered
        FROM blob_egress_usage_buckets
        WHERE scope_kind = $1 AND scope_id = $2
          AND window_kind = $3 AND window_start = $4
        FOR UPDATE
        "#,
    )
    .bind(scope_kind)
    .bind(scope_id)
    .bind(window_kind)
    .bind(window_start)
    .fetch_one(&mut **tx)
    .await?;
    Ok(usage)
}

async fn increment_pending_bucket(
    tx: &mut Transaction<'_, Postgres>,
    scope_kind: &str,
    scope_id: Uuid,
    window_kind: &str,
    window_start: OffsetDateTime,
    bytes: i64,
) -> anyhow::Result<()> {
    let updated = sqlx::query(
        r#"
        UPDATE blob_egress_usage_buckets
        SET bytes_pending = bytes_pending + $5, updated_at = now()
        WHERE scope_kind = $1 AND scope_id = $2
          AND window_kind = $3 AND window_start = $4
        "#,
    )
    .bind(scope_kind)
    .bind(scope_id)
    .bind(window_kind)
    .bind(window_start)
    .bind(bytes)
    .execute(&mut **tx)
    .await?;
    anyhow::ensure!(updated.rows_affected() == 1, "usage bucket disappeared");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn reconcile_bucket(
    tx: &mut Transaction<'_, Postgres>,
    scope_kind: &str,
    scope_id: Uuid,
    window_kind: &str,
    window_start: OffsetDateTime,
    bytes_reserved: i64,
    bytes_delivered: i64,
) -> anyhow::Result<()> {
    ensure_and_lock_usage_bucket(tx, scope_kind, scope_id, window_kind, window_start).await?;
    let updated = sqlx::query(
        r#"
        UPDATE blob_egress_usage_buckets
        SET bytes_pending = bytes_pending - $5,
            bytes_delivered = bytes_delivered + $6,
            updated_at = now()
        WHERE scope_kind = $1 AND scope_id = $2
          AND window_kind = $3 AND window_start = $4
          AND bytes_pending >= $5
        "#,
    )
    .bind(scope_kind)
    .bind(scope_id)
    .bind(window_kind)
    .bind(window_start)
    .bind(bytes_reserved)
    .bind(bytes_delivered)
    .execute(&mut **tx)
    .await?;
    anyhow::ensure!(updated.rows_affected() == 1, "usage bucket is inconsistent");
    Ok(())
}

async fn authorized_owner(
    tx: &mut Transaction<'_, Postgres>,
    actor_id: Uuid,
    space_id: Uuid,
    require_write: bool,
) -> anyhow::Result<Option<Uuid>> {
    sqlx::query_scalar(
        r#"
        SELECT space.owner_user_id
        FROM security_spaces space
        JOIN security_space_members member ON member.space_id = space.id
        WHERE space.id = $1
          AND space.status = 'active'
          AND member.user_id = $2
          AND member.status = 'active'
          AND (NOT $3 OR member.role IN ('owner', 'editor'))
        FOR SHARE OF space
        "#,
    )
    .bind(space_id)
    .bind(actor_id)
    .bind(require_write)
    .fetch_optional(&mut **tx)
    .await
    .map_err(Into::into)
}

async fn lock_owner_quota(
    tx: &mut Transaction<'_, Postgres>,
    owner_id: Uuid,
) -> anyhow::Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 731))")
        .bind(owner_id.to_string())
        .execute(&mut **tx)
        .await?;
    Ok(())
}
