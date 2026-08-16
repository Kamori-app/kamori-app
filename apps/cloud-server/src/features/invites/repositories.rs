//! Atomic security-space invite lifecycle.

use anyhow::anyhow;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::features::spaces::dto::SpaceRole;

pub(crate) struct InviteCodeInsert<'a> {
    pub(crate) id: Uuid,
    pub(crate) space_id: Uuid,
    pub(crate) created_by: Uuid,
    pub(crate) role: SpaceRole,
    pub(crate) code_hash: &'a [u8],
    pub(crate) encrypted_key_package: &'a [u8],
    pub(crate) encrypted_note: Option<&'a [u8]>,
    pub(crate) ttl_minutes: i32,
}

pub(crate) async fn can_invite(
    pool: &PgPool,
    space_id: Uuid,
    actor_id: Uuid,
) -> anyhow::Result<bool> {
    sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM security_space_members sm
            JOIN security_spaces s ON s.id = sm.space_id
            WHERE sm.space_id = $1
              AND sm.user_id = $2
              AND sm.status = 'active'
              AND sm.role IN ('owner', 'editor')
              AND s.status = 'active'
        )
        "#,
    )
    .bind(space_id)
    .bind(actor_id)
    .fetch_one(pool)
    .await
    .map_err(Into::into)
}

pub(crate) async fn insert_invite_code(
    pool: &PgPool,
    invite: InviteCodeInsert<'_>,
) -> anyhow::Result<()> {
    let role = match invite.role {
        SpaceRole::Editor => "editor",
        SpaceRole::Reader => "reader",
        SpaceRole::Owner => anyhow::bail!("owner role cannot be granted by invite"),
    };
    sqlx::query(
        r#"
        INSERT INTO security_space_invites (
            id, space_id, created_by, role, code_hash, encrypted_key_package,
            encrypted_note, expires_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, now() + make_interval(mins => $8::int))
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
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub(crate) struct RedeemedInvite {
    pub(crate) space_id: Uuid,
    pub(crate) role: SpaceRole,
    pub(crate) key_epoch: u32,
    pub(crate) encrypted_key_package: Vec<u8>,
    pub(crate) encrypted_note: Option<Vec<u8>>,
}

pub(crate) async fn redeem_invite_code_tx(
    pool: &PgPool,
    code_hash: &[u8],
    actor_id: Uuid,
) -> anyhow::Result<RedeemedInvite> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query(
        r#"
        SELECT i.id, i.space_id, i.role, i.encrypted_key_package,
               i.encrypted_note, s.current_key_epoch
        FROM security_space_invites i
        JOIN security_spaces s ON s.id = i.space_id AND s.status = 'active'
        WHERE i.code_hash = $1
          AND i.redeemed_at IS NULL
          AND i.expires_at > now()
        FOR UPDATE OF i
        "#,
    )
    .bind(code_hash)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| anyhow!("invite code is invalid or expired"))?;

    let invite_id: Uuid = row.try_get("id")?;
    let space_id: Uuid = row.try_get("space_id")?;
    let role_value: String = row.try_get("role")?;
    let role = SpaceRole::from_db(&role_value)?;
    let key_epoch_i32: i32 = row.try_get("current_key_epoch")?;
    let key_epoch = u32::try_from(key_epoch_i32)?;
    let encrypted_key_package: Vec<u8> = row.try_get("encrypted_key_package")?;
    let encrypted_note: Option<Vec<u8>> = row.try_get("encrypted_note")?;

    sqlx::query(
        r#"
        INSERT INTO security_space_members (
            id, space_id, user_id, role, status, key_epoch
        ) VALUES ($1, $2, $3, $4, 'active', $5)
        ON CONFLICT (space_id, user_id) DO UPDATE SET
            role = EXCLUDED.role,
            status = 'active',
            key_epoch = EXCLUDED.key_epoch,
            revoked_at = NULL
        WHERE security_space_members.role <> 'owner'
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(space_id)
    .bind(actor_id)
    .bind(role_value)
    .bind(key_epoch_i32)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE security_space_invites SET redeemed_by = $2, redeemed_at = now() WHERE id = $1",
    )
    .bind(invite_id)
    .bind(actor_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(RedeemedInvite {
        space_id,
        role,
        key_epoch,
        encrypted_key_package,
        encrypted_note,
    })
}
