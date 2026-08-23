//! Transactional ownership transfer persistence and quota enforcement.

use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use super::dto::{OwnershipResourceKind, OwnershipTransferOffer};

const OFFER_TTL_HOURS: i32 = 24;

pub(crate) enum CreateOfferResult {
    Created(OwnershipTransferOffer),
    AccessDenied,
    InvalidTarget,
    PersonalWorkspace,
    AlreadyPending,
}

pub(crate) enum AcceptOfferResult {
    Accepted,
    NotFound,
    NoLongerValid,
    BlobStorageQuotaExceeded,
    OperationStorageQuotaExceeded,
}

async fn lock_quota(tx: &mut Transaction<'_, Postgres>, owner_id: Uuid) -> anyhow::Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 731))")
        .bind(owner_id.to_string())
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub(crate) async fn create_offer(
    pool: &PgPool,
    actor_id: Uuid,
    kind: OwnershipResourceKind,
    resource_id: Uuid,
    target_user_id: Uuid,
) -> anyhow::Result<CreateOfferResult> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 887))")
        .bind(format!("{}:{resource_id}", kind.as_db()))
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        r#"
        UPDATE ownership_transfer_offers
        SET cancelled_at = now()
        WHERE resource_kind = $1 AND resource_id = $2
          AND accepted_at IS NULL AND cancelled_at IS NULL AND expires_at <= now()
        "#,
    )
    .bind(kind.as_db())
    .bind(resource_id)
    .execute(&mut *tx)
    .await?;

    let validation = match kind {
        OwnershipResourceKind::Workspace => sqlx::query(
            r#"
                SELECT workspace.kind,
                       EXISTS(
                           SELECT 1 FROM workspace_members member
                           JOIN users target ON target.id = member.user_id
                           WHERE member.workspace_id = workspace.id
                             AND member.user_id = $3
                             AND member.status = 'active'
                             AND target.deleted_at IS NULL
                             AND target.suspended_at IS NULL
                       ) AS target_valid
                FROM workspaces workspace
                WHERE workspace.id = $1 AND workspace.owner_user_id = $2
                  AND workspace.deleted_at IS NULL
                FOR UPDATE OF workspace
                "#,
        )
        .bind(resource_id)
        .bind(actor_id)
        .bind(target_user_id)
        .fetch_optional(&mut *tx)
        .await?
        .map(|row| {
            let kind: String = row.try_get("kind")?;
            let target_valid: bool = row.try_get("target_valid")?;
            Ok::<_, sqlx::Error>((kind == "team", target_valid))
        })
        .transpose()?,
        OwnershipResourceKind::SecuritySpace => sqlx::query(
            r#"
                SELECT EXISTS(
                    SELECT 1 FROM security_space_members member
                    JOIN users target ON target.id = member.user_id
                    WHERE member.space_id = space.id
                      AND member.user_id = $3
                      AND member.status = 'active'
                      AND target.deleted_at IS NULL
                      AND target.suspended_at IS NULL
                ) AS target_valid
                FROM security_spaces space
                WHERE space.id = $1 AND space.owner_user_id = $2
                  AND space.status = 'active'
                FOR UPDATE OF space
                "#,
        )
        .bind(resource_id)
        .bind(actor_id)
        .bind(target_user_id)
        .fetch_optional(&mut *tx)
        .await?
        .map(|row| Ok::<_, sqlx::Error>((true, row.try_get("target_valid")?)))
        .transpose()?,
    };
    let Some((transferable, target_valid)) = validation else {
        tx.rollback().await?;
        return Ok(CreateOfferResult::AccessDenied);
    };
    if !transferable {
        tx.rollback().await?;
        return Ok(CreateOfferResult::PersonalWorkspace);
    }
    if !target_valid || target_user_id == actor_id {
        tx.rollback().await?;
        return Ok(CreateOfferResult::InvalidTarget);
    }
    let pending: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM ownership_transfer_offers
            WHERE resource_kind = $1 AND resource_id = $2
              AND accepted_at IS NULL AND cancelled_at IS NULL
        )
        "#,
    )
    .bind(kind.as_db())
    .bind(resource_id)
    .fetch_one(&mut *tx)
    .await?;
    if pending {
        tx.rollback().await?;
        return Ok(CreateOfferResult::AlreadyPending);
    }

    let id = Uuid::new_v4();
    let row = sqlx::query(
        r#"
        INSERT INTO ownership_transfer_offers (
            id, resource_kind, resource_id, current_owner_id, target_user_id, expires_at
        ) VALUES ($1, $2, $3, $4, $5, now() + make_interval(hours => $6))
        RETURNING (extract(epoch FROM expires_at) * 1000)::bigint AS expires_at_ms,
                  (extract(epoch FROM created_at) * 1000)::bigint AS created_at_ms
        "#,
    )
    .bind(id)
    .bind(kind.as_db())
    .bind(resource_id)
    .bind(actor_id)
    .bind(target_user_id)
    .bind(OFFER_TTL_HOURS)
    .fetch_one(&mut *tx)
    .await?;
    let username: String = sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
        .bind(actor_id)
        .fetch_one(&mut *tx)
        .await?;
    let offer = OwnershipTransferOffer {
        transfer_id: id,
        resource_kind: kind,
        resource_id,
        current_owner_id: actor_id,
        current_owner_username: username,
        target_user_id,
        expires_at_unix_ms: row.try_get("expires_at_ms")?,
        created_at_unix_ms: row.try_get("created_at_ms")?,
    };
    tx.commit().await?;
    Ok(CreateOfferResult::Created(offer))
}

pub(crate) async fn list_incoming(
    pool: &PgPool,
    target_user_id: Uuid,
) -> anyhow::Result<Vec<OwnershipTransferOffer>> {
    let rows = sqlx::query(
        r#"
        SELECT offer.id, offer.resource_kind, offer.resource_id,
               offer.current_owner_id, owner.username, offer.target_user_id,
               (extract(epoch FROM offer.expires_at) * 1000)::bigint AS expires_at_ms,
               (extract(epoch FROM offer.created_at) * 1000)::bigint AS created_at_ms
        FROM ownership_transfer_offers offer
        JOIN users owner ON owner.id = offer.current_owner_id
        WHERE offer.target_user_id = $1
          AND offer.accepted_at IS NULL AND offer.cancelled_at IS NULL
          AND offer.expires_at > now()
        ORDER BY offer.created_at DESC
        "#,
    )
    .bind(target_user_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(OwnershipTransferOffer {
                transfer_id: row.try_get("id")?,
                resource_kind: OwnershipResourceKind::from_db(
                    &row.try_get::<String, _>("resource_kind")?,
                )?,
                resource_id: row.try_get("resource_id")?,
                current_owner_id: row.try_get("current_owner_id")?,
                current_owner_username: row.try_get("username")?,
                target_user_id: row.try_get("target_user_id")?,
                expires_at_unix_ms: row.try_get("expires_at_ms")?,
                created_at_unix_ms: row.try_get("created_at_ms")?,
            })
        })
        .collect()
}

pub(crate) async fn list_outgoing(
    pool: &PgPool,
    current_owner_id: Uuid,
) -> anyhow::Result<Vec<OwnershipTransferOffer>> {
    let rows = sqlx::query(
        r#"
        SELECT offer.id, offer.resource_kind, offer.resource_id,
               offer.current_owner_id, owner.username, offer.target_user_id,
               (extract(epoch FROM offer.expires_at) * 1000)::bigint AS expires_at_ms,
               (extract(epoch FROM offer.created_at) * 1000)::bigint AS created_at_ms
        FROM ownership_transfer_offers offer
        JOIN users owner ON owner.id = offer.current_owner_id
        WHERE offer.current_owner_id = $1
          AND offer.accepted_at IS NULL AND offer.cancelled_at IS NULL
          AND offer.expires_at > now()
        ORDER BY offer.created_at DESC
        "#,
    )
    .bind(current_owner_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(OwnershipTransferOffer {
                transfer_id: row.try_get("id")?,
                resource_kind: OwnershipResourceKind::from_db(
                    &row.try_get::<String, _>("resource_kind")?,
                )?,
                resource_id: row.try_get("resource_id")?,
                current_owner_id: row.try_get("current_owner_id")?,
                current_owner_username: row.try_get("username")?,
                target_user_id: row.try_get("target_user_id")?,
                expires_at_unix_ms: row.try_get("expires_at_ms")?,
                created_at_unix_ms: row.try_get("created_at_ms")?,
            })
        })
        .collect()
}

pub(crate) async fn accept_offer(
    pool: &PgPool,
    actor_id: Uuid,
    transfer_id: Uuid,
    account_storage_limit: i64,
    account_operation_storage_limit: i64,
) -> anyhow::Result<AcceptOfferResult> {
    let mut tx = pool.begin().await?;
    let offer = sqlx::query(
        r#"
        SELECT resource_kind, resource_id, current_owner_id, expires_at > now() AS unexpired
        FROM ownership_transfer_offers
        WHERE id = $1 AND target_user_id = $2
          AND accepted_at IS NULL AND cancelled_at IS NULL
        FOR UPDATE
        "#,
    )
    .bind(transfer_id)
    .bind(actor_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(offer) = offer else {
        tx.rollback().await?;
        return Ok(AcceptOfferResult::NotFound);
    };
    if !offer.try_get::<bool, _>("unexpired")? {
        sqlx::query("UPDATE ownership_transfer_offers SET cancelled_at = now() WHERE id = $1")
            .bind(transfer_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Ok(AcceptOfferResult::NoLongerValid);
    }
    let kind = OwnershipResourceKind::from_db(&offer.try_get::<String, _>("resource_kind")?)?;
    let resource_id: Uuid = offer.try_get("resource_id")?;
    let current_owner_id: Uuid = offer.try_get("current_owner_id")?;

    let valid = match kind {
        OwnershipResourceKind::Workspace => {
            let valid = sqlx::query_scalar::<_, Uuid>(
                r#"
                SELECT workspace.id
                FROM workspaces workspace
                JOIN workspace_members target
                  ON target.workspace_id = workspace.id
                 AND target.user_id = $3 AND target.status = 'active'
                JOIN users recipient ON recipient.id = target.user_id
                WHERE workspace.id = $1 AND workspace.owner_user_id = $2
                  AND workspace.kind = 'team' AND workspace.deleted_at IS NULL
                  AND recipient.deleted_at IS NULL AND recipient.suspended_at IS NULL
                FOR UPDATE OF workspace, recipient
                "#,
            )
            .bind(resource_id)
            .bind(current_owner_id)
            .bind(actor_id)
            .fetch_optional(&mut *tx)
            .await?
            .is_some();
            if valid {
                sqlx::query("UPDATE workspaces SET owner_user_id = $2 WHERE id = $1")
                    .bind(resource_id)
                    .bind(actor_id)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query(
                    "UPDATE workspace_members SET role = CASE WHEN user_id = $2 THEN 'owner' ELSE 'member' END WHERE workspace_id = $1 AND user_id IN ($2, $3) AND status = 'active'",
                )
                .bind(resource_id)
                .bind(actor_id)
                .bind(current_owner_id)
                .execute(&mut *tx)
                .await?;
            }
            valid
        }
        OwnershipResourceKind::SecuritySpace => {
            let owner_id: Option<Uuid> = sqlx::query_scalar(
                r#"
                SELECT space.owner_user_id
                FROM security_spaces space
                JOIN security_space_members target
                  ON target.space_id = space.id
                 AND target.user_id = $3 AND target.status = 'active'
                JOIN users recipient ON recipient.id = target.user_id
                WHERE space.id = $1 AND space.owner_user_id = $2 AND space.status = 'active'
                  AND recipient.deleted_at IS NULL AND recipient.suspended_at IS NULL
                FOR UPDATE OF space, recipient
                "#,
            )
            .bind(resource_id)
            .bind(current_owner_id)
            .bind(actor_id)
            .fetch_optional(&mut *tx)
            .await?;
            if owner_id.is_none() {
                false
            } else {
                lock_quota(&mut tx, actor_id).await?;
                // Lock both account counters while the space owner row is
                // locked. Operation appends cannot charge either side midway
                // through a transfer.
                sqlx::query("SELECT id FROM users WHERE id IN ($1, $2) ORDER BY id FOR UPDATE")
                    .bind(current_owner_id)
                    .bind(actor_id)
                    .fetch_all(&mut *tx)
                    .await?;
                let target_bytes: i64 =
                    sqlx::query_scalar("SELECT blob_storage_bytes FROM users WHERE id = $1")
                        .bind(actor_id)
                        .fetch_one(&mut *tx)
                        .await?;
                let transferred_bytes: i64 = sqlx::query_scalar(
                    "SELECT COALESCE(sum(size_padded), 0)::bigint FROM space_blobs WHERE space_id = $1",
                )
                .bind(resource_id)
                .fetch_one(&mut *tx)
                .await?;
                if target_bytes.saturating_add(transferred_bytes) > account_storage_limit {
                    tx.rollback().await?;
                    return Ok(AcceptOfferResult::BlobStorageQuotaExceeded);
                }
                let target_operation_bytes: i64 =
                    sqlx::query_scalar("SELECT operation_bytes FROM users WHERE id = $1")
                        .bind(actor_id)
                        .fetch_one(&mut *tx)
                        .await?;
                let transferred_operation_bytes: i64 =
                    sqlx::query_scalar("SELECT operation_bytes FROM security_spaces WHERE id = $1")
                        .bind(resource_id)
                        .fetch_one(&mut *tx)
                        .await?;
                if target_operation_bytes.saturating_add(transferred_operation_bytes)
                    > account_operation_storage_limit
                {
                    tx.rollback().await?;
                    return Ok(AcceptOfferResult::OperationStorageQuotaExceeded);
                }
                sqlx::query("UPDATE security_spaces SET owner_user_id = $2 WHERE id = $1")
                    .bind(resource_id)
                    .bind(actor_id)
                    .execute(&mut *tx)
                    .await?;
                // The security-space owner trigger atomically transfers both
                // account operation usage and denormalized blob ownership.
                // Historical egress remains charged to the owner who controlled
                // the space when each download was admitted. Reassigning it here
                // would make both monthly quotas and the immutable usage ledger lie.
                sqlx::query(
                    "UPDATE security_space_members SET role = CASE WHEN user_id = $2 THEN 'owner' ELSE 'editor' END WHERE space_id = $1 AND user_id IN ($2, $3) AND status = 'active'",
                )
                .bind(resource_id)
                .bind(actor_id)
                .bind(current_owner_id)
                .execute(&mut *tx)
                .await?;
                true
            }
        }
    };
    if !valid {
        sqlx::query("UPDATE ownership_transfer_offers SET cancelled_at = now() WHERE id = $1")
            .bind(transfer_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Ok(AcceptOfferResult::NoLongerValid);
    }
    sqlx::query("UPDATE ownership_transfer_offers SET accepted_at = now() WHERE id = $1")
        .bind(transfer_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(AcceptOfferResult::Accepted)
}

pub(crate) async fn cancel_offer(
    pool: &PgPool,
    actor_id: Uuid,
    transfer_id: Uuid,
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE ownership_transfer_offers
        SET cancelled_at = now()
        WHERE id = $1 AND (current_owner_id = $2 OR target_user_id = $2)
          AND accepted_at IS NULL AND cancelled_at IS NULL
        "#,
    )
    .bind(transfer_id)
    .bind(actor_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}
