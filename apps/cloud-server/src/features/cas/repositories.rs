//! Atomic space authorization, storage admission, and egress reservations.

use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

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
        "SELECT ciphertext_sha256, size_padded, object_key, status FROM space_blobs WHERE id = $1 AND space_id = $2",
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
        return Ok(if status == "ready" {
            StoreBlobResult::AlreadyStored
        } else {
            StoreBlobResult::NeedsUpload(CasRow {
                blob_id,
                ciphertext_sha256: existing_hash,
                size_padded: existing_size,
                object_key: row.try_get("object_key")?,
            })
        });
    }

    let stored_bytes: i64 = sqlx::query_scalar(
        "SELECT COALESCE(sum(size_padded), 0)::bigint FROM space_blobs WHERE owner_user_id = $1",
    )
    .bind(owner_id)
    .fetch_one(&mut *tx)
    .await?;
    if stored_bytes.saturating_add(size_padded) > account_storage_limit {
        return Ok(StoreBlobResult::StorageQuotaExceeded);
    }

    let object_key = format!("spaces/{space_id}/blobs/{blob_id}");
    sqlx::query(
        r#"
        INSERT INTO space_blobs (
            id, space_id, owner_user_id, created_by, ciphertext_sha256, size_padded, object_key
        ) VALUES ($1, $2, $3, $4, $5, $6, $7)
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
        SET status = 'ready'
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
    GlobalQuotaExceeded,
}

pub(crate) struct DownloadReservation {
    pub(crate) id: Uuid,
    pub(crate) blob: CasRow,
}

pub(crate) struct EgressLimits {
    pub(crate) owner_monthly: i64,
    pub(crate) owner_rolling_24h: i64,
    pub(crate) global_nonessential_stop: i64,
    pub(crate) owner_concurrent_downloads: i64,
}

pub(crate) async fn reserve_download(
    pool: &PgPool,
    actor_id: Uuid,
    space_id: Uuid,
    blob_id: Uuid,
    limits: EgressLimits,
) -> anyhow::Result<ReserveDownloadResult> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(44200618)")
        .execute(&mut *tx)
        .await?;
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

    let owner_month: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(sum(
            CASE WHEN completed_at IS NULL THEN bytes_reserved ELSE bytes_delivered END
        ), 0)::bigint
        FROM blob_egress_reservations
        WHERE owner_user_id = $1
          AND reserved_at >= date_trunc('month', now())
        "#,
    )
    .bind(owner_id)
    .fetch_one(&mut *tx)
    .await?;
    let owner_24h: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(sum(
            CASE WHEN completed_at IS NULL THEN bytes_reserved ELSE bytes_delivered END
        ), 0)::bigint
        FROM blob_egress_reservations
        WHERE owner_user_id = $1
          AND reserved_at >= now() - interval '24 hours'
        "#,
    )
    .bind(owner_id)
    .fetch_one(&mut *tx)
    .await?;
    if owner_month.saturating_add(blob.size_padded) > limits.owner_monthly
        || owner_24h.saturating_add(blob.size_padded) > limits.owner_rolling_24h
    {
        return Ok(ReserveDownloadResult::OwnerQuotaExceeded);
    }
    let global_month: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(sum(
            CASE WHEN completed_at IS NULL THEN bytes_reserved ELSE bytes_delivered END
        ), 0)::bigint
        FROM blob_egress_reservations
        WHERE reserved_at >= date_trunc('month', now())
        "#,
    )
    .fetch_one(&mut *tx)
    .await?;
    if global_month.saturating_add(blob.size_padded) > limits.global_nonessential_stop {
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
    sqlx::query(
        r#"
        UPDATE blob_egress_reservations
        SET bytes_delivered = LEAST(bytes_reserved, GREATEST(0, $2)),
            completed_at = COALESCE(completed_at, now())
        WHERE id = $1
        "#,
    )
    .bind(reservation_id)
    .bind(bytes_delivered)
    .execute(pool)
    .await?;
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
