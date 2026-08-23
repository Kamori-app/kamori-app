//! Atomic security-space invite lifecycle.

use anyhow::{anyhow, ensure};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::features::spaces::dto::SpaceRole;

pub(crate) struct InviteCodeInsert<'a> {
    pub(crate) id: Uuid,
    pub(crate) space_id: Uuid,
    pub(crate) rotation_id: Uuid,
    pub(crate) created_by: Uuid,
    pub(crate) role: SpaceRole,
    pub(crate) code_hash: &'a [u8],
    pub(crate) encrypted_key_package: &'a [u8],
    pub(crate) encrypted_note: Option<&'a [u8]>,
    pub(crate) ttl_minutes: i32,
    pub(crate) request_hash: &'a [u8; 32],
}

pub(crate) enum InviteCodeInsertResult {
    Stored(Uuid),
    Conflict,
    AccessDenied,
}

pub(crate) async fn insert_invite_code(
    pool: &PgPool,
    invite: InviteCodeInsert<'_>,
) -> anyhow::Result<InviteCodeInsertResult> {
    let role = match invite.role {
        SpaceRole::Editor => "editor",
        SpaceRole::Reader => "reader",
        SpaceRole::Owner => anyhow::bail!("owner role cannot be granted by invite"),
    };
    let mut tx = pool.begin().await?;
    let locked: Option<bool> = sqlx::query_scalar(
        "SELECT TRUE FROM security_spaces WHERE id = $1 AND status = 'active' FOR UPDATE",
    )
    .bind(invite.space_id)
    .fetch_optional(&mut *tx)
    .await?;
    if locked.is_none() {
        return Ok(InviteCodeInsertResult::AccessDenied);
    }
    let result = sqlx::query(
        r#"
        INSERT INTO security_space_invites (
            id, space_id, created_by, role, code_hash, encrypted_key_package,
            encrypted_note, expires_at, key_epoch, invite_version, max_uses,
            rotation_id, request_hash
        )
        SELECT $1, s.id, $3, $4, $5, $6, $7,
               now() + make_interval(mins => $8::int), s.current_key_epoch, 1, 1,
               epoch.rotation_id, $10
        FROM security_spaces s
        JOIN security_space_members creator
         ON creator.space_id = s.id
         AND creator.user_id = $3
         AND creator.status = 'active'
         AND creator.role = 'owner'
        JOIN security_space_epochs epoch
          ON epoch.space_id = s.id
         AND epoch.key_epoch = s.current_key_epoch
         AND epoch.rotation_id = $9
         AND epoch.status = 'committed'
         AND epoch.created_by = $3
         AND epoch.target_user_id IS NULL
        WHERE s.id = $2 AND s.status = 'active'
        ON CONFLICT (rotation_id) WHERE rotation_id IS NOT NULL DO NOTHING
        RETURNING id
        "#,
    )
    .bind(invite.id)
    .bind(invite.space_id)
    .bind(invite.created_by)
    .bind(role)
    .bind(invite.code_hash)
    .bind(invite.encrypted_key_package)
    .bind(invite.encrypted_note)
    .bind(invite.ttl_minutes)
    .bind(invite.rotation_id)
    .bind(invite.request_hash.as_slice())
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(row) = result {
        let id = row.try_get("id")?;
        tx.commit().await?;
        return Ok(InviteCodeInsertResult::Stored(id));
    }
    let existing =
        sqlx::query("SELECT id, request_hash FROM security_space_invites WHERE rotation_id = $1")
            .bind(invite.rotation_id)
            .fetch_optional(&mut *tx)
            .await?;
    if let Some(existing) = existing {
        let matches = existing
            .try_get::<Option<Vec<u8>>, _>("request_hash")?
            .as_deref()
            == Some(invite.request_hash.as_slice());
        let result = if matches {
            InviteCodeInsertResult::Stored(existing.try_get("id")?)
        } else {
            InviteCodeInsertResult::Conflict
        };
        tx.commit().await?;
        return Ok(result);
    }
    let authorized: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM security_spaces space
            JOIN security_space_members member
              ON member.space_id = space.id
             AND member.user_id = $2
             AND member.status = 'active'
             AND member.role = 'owner'
            JOIN security_space_epochs epoch
              ON epoch.space_id = space.id
             AND epoch.key_epoch = space.current_key_epoch
             AND epoch.rotation_id = $3
             AND epoch.status = 'committed'
             AND epoch.created_by = $2
             AND epoch.target_user_id IS NULL
            WHERE space.id = $1 AND space.status = 'active'
        )
        "#,
    )
    .bind(invite.space_id)
    .bind(invite.created_by)
    .bind(invite.rotation_id)
    .fetch_one(&mut *tx)
    .await?;
    let result = if authorized {
        InviteCodeInsertResult::Conflict
    } else {
        InviteCodeInsertResult::AccessDenied
    };
    tx.commit().await?;
    Ok(result)
}

#[derive(Debug, Clone)]
pub(crate) struct RedeemedInvite {
    pub(crate) space_id: Uuid,
    pub(crate) role: SpaceRole,
    pub(crate) key_epoch: u32,
    pub(crate) history_start_seq: u64,
    pub(crate) current_state_start_seq: u64,
    pub(crate) encrypted_key_package: Vec<u8>,
    pub(crate) encrypted_note: Option<Vec<u8>>,
}

pub(crate) enum RedeemInviteOutcome {
    Redeemed(RedeemedInvite),
    InvalidOrExpired,
    AlreadyOwner,
}

pub(crate) async fn redeem_invite_code_tx(
    pool: &PgPool,
    code_hash: &[u8],
    actor_id: Uuid,
) -> anyhow::Result<RedeemInviteOutcome> {
    let mut tx = pool.begin().await?;
    // Serialize redemption with key rotation and invite creation. Without the
    // space-row lock, a redemption can validate epoch N while a concurrent
    // rotation commits N+1, leaving a newly active member with a stale key.
    let space_id: Option<Uuid> = sqlx::query_scalar(
        r#"
        SELECT i.space_id
        FROM security_space_invites i
        JOIN security_spaces s ON s.id = i.space_id
        WHERE i.code_hash = $1 AND s.status = 'active'
        FOR UPDATE OF s
        "#,
    )
    .bind(code_hash)
    .fetch_optional(&mut *tx)
    .await?;
    if space_id.is_none() {
        return Ok(RedeemInviteOutcome::InvalidOrExpired);
    }
    let Some(row) = sqlx::query(
        r#"
        SELECT i.id, i.space_id, i.role, i.encrypted_key_package,
               i.encrypted_note, i.key_epoch,
               s.current_state_start_seq
        FROM security_space_invites i
        JOIN security_spaces s ON s.id = i.space_id AND s.status = 'active'
        WHERE i.code_hash = $1
          AND i.revoked_at IS NULL
          AND i.used_count < i.max_uses
          AND i.expires_at > now()
          AND i.key_epoch = s.current_key_epoch
        FOR UPDATE OF i
        "#,
    )
    .bind(code_hash)
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(RedeemInviteOutcome::InvalidOrExpired);
    };

    let invite_id: Uuid = row.try_get("id")?;
    let space_id: Uuid = row.try_get("space_id")?;
    let role_value: String = row.try_get("role")?;
    let role = SpaceRole::from_db(&role_value)?;
    let key_epoch_i32: i32 = row.try_get("key_epoch")?;
    let key_epoch = u32::try_from(key_epoch_i32)?;
    let current_state_start_seq = u64::try_from(row.try_get::<i64, _>("current_state_start_seq")?)?;
    let encrypted_key_package: Vec<u8> = row.try_get("encrypted_key_package")?;
    let encrypted_note: Option<Vec<u8>> = row.try_get("encrypted_note")?;

    let existing_role: Option<String> = sqlx::query_scalar(
        r#"
        SELECT role
        FROM security_space_members
        WHERE space_id = $1 AND user_id = $2 AND status = 'active'
        FOR UPDATE
        "#,
    )
    .bind(space_id)
    .bind(actor_id)
    .fetch_optional(&mut *tx)
    .await?;
    if existing_role.as_deref() == Some("owner") {
        return Ok(RedeemInviteOutcome::AlreadyOwner);
    }
    let effective_role = if existing_role.as_deref() == Some("editor") {
        SpaceRole::Editor
    } else {
        role
    };
    let effective_role_value = match effective_role {
        SpaceRole::Editor => "editor",
        SpaceRole::Reader => "reader",
        SpaceRole::Owner => return Err(anyhow!("invite unexpectedly resolved to owner")),
    };

    let persisted_history_start: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO security_space_members (
            id, space_id, user_id, role, status, key_epoch, history_start_seq
        ) VALUES ($1, $2, $3, $4, 'active', $5, $6)
        ON CONFLICT (space_id, user_id) DO UPDATE SET
            role = CASE
                WHEN security_space_members.role = 'editor'
                     AND EXCLUDED.role = 'reader'
                THEN 'editor'
                ELSE EXCLUDED.role
            END,
            status = 'active',
            key_epoch = EXCLUDED.key_epoch,
            history_start_seq = CASE
                WHEN security_space_members.status = 'active'
                THEN LEAST(
                    security_space_members.history_start_seq,
                    EXCLUDED.history_start_seq
                )
                ELSE EXCLUDED.history_start_seq
            END,
            revoked_at = NULL
        WHERE security_space_members.role <> 'owner'
        RETURNING history_start_seq
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(space_id)
    .bind(actor_id)
    .bind(effective_role_value)
    .bind(key_epoch_i32)
    .bind(i64::try_from(current_state_start_seq)?)
    .fetch_one(&mut *tx)
    .await?;
    let effective_history_start_seq = u64::try_from(persisted_history_start)?;

    let consume = sqlx::query(
        r#"
        UPDATE security_space_invites
        SET redeemed_by = $2, redeemed_at = now(), used_count = used_count + 1
        WHERE id = $1 AND revoked_at IS NULL AND used_count < max_uses
        "#,
    )
    .bind(invite_id)
    .bind(actor_id)
    .execute(&mut *tx)
    .await?;
    ensure!(
        consume.rows_affected() == 1,
        "locked invite was not consumed"
    );

    tx.commit().await?;
    Ok(RedeemInviteOutcome::Redeemed(RedeemedInvite {
        space_id,
        role: effective_role,
        key_epoch,
        history_start_seq: effective_history_start_seq,
        current_state_start_seq,
        encrypted_key_package,
        encrypted_note,
    }))
}
