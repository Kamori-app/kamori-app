//! Database access for security spaces and per-device key packages.

use std::collections::{HashMap, HashSet};

use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::dto::{
    CreateSpaceRequest, DeviceKeyPackage, MemberRecoveryKeyPackage, RecoverySpaceKeyPackage,
    SpaceDeviceSummary, SpaceMemberSummary, SpaceRole, SpaceSummary,
};
use crypto_core_lib::operation_envelope::OperationEnvelopeV1;

pub(crate) async fn create_space(
    pool: &PgPool,
    user_id: Uuid,
    workspace_id: Uuid,
    request: &CreateSpaceRequest,
) -> anyhow::Result<SpaceSummary> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query(
        r#"
        INSERT INTO security_spaces (
            id, workspace_id, owner_user_id, created_by, encrypted_metadata
        )
        VALUES ($1, $2, $3, $3, $4)
        RETURNING (extract(epoch FROM created_at) * 1000)::bigint AS created_at_ms
        "#,
    )
    .bind(request.space_id)
    .bind(workspace_id)
    .bind(user_id)
    .bind(&request.encrypted_metadata)
    .fetch_one(&mut *tx)
    .await?;
    let created_at_unix_ms: i64 = row.try_get("created_at_ms")?;

    sqlx::query(
        r#"
        INSERT INTO security_space_members (id, space_id, user_id, role, key_epoch)
        VALUES ($1, $2, $3, 'owner', 1)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(request.space_id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO security_space_epochs (
            space_id, key_epoch, rotation_id, status, created_by, committed_at
        ) VALUES ($1, 1, $2, 'committed', $3, now())
        "#,
    )
    .bind(request.space_id)
    .bind(Uuid::new_v4())
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    for package in &request.device_key_packages {
        let result = sqlx::query(
            r#"
            INSERT INTO security_space_device_keys (
                space_id, user_id, device_id, key_epoch, encrypted_key_package
            )
            SELECT $1, $2, d.id, 1, $4
            FROM devices d
            WHERE d.id = $3 AND d.user_id = $2 AND d.status = 'active'
            "#,
        )
        .bind(request.space_id)
        .bind(user_id)
        .bind(package.device_id)
        .bind(&package.encrypted_key_package)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            anyhow::bail!("device key package references an inactive or foreign device");
        }
    }

    sqlx::query(
        r#"
        INSERT INTO security_space_recovery_keys (
            space_id, user_id, key_epoch, encrypted_key_package
        ) VALUES ($1, $2, 1, $3)
        "#,
    )
    .bind(request.space_id)
    .bind(user_id)
    .bind(&request.encrypted_recovery_key_package)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(SpaceSummary {
        space_id: request.space_id,
        workspace_id,
        role: SpaceRole::Owner,
        key_epoch: 1,
        encrypted_metadata: request.encrypted_metadata.clone(),
        device_key_packages: request.device_key_packages.clone(),
        created_at_unix_ms,
    })
}

pub(crate) async fn put_recovery_key_package(
    pool: &PgPool,
    user_id: Uuid,
    space_id: Uuid,
    key_epoch: u32,
    encrypted_key_package: &[u8],
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        r#"
        INSERT INTO security_space_recovery_keys (
            space_id, user_id, key_epoch, encrypted_key_package
        )
        SELECT s.id, $2, s.current_key_epoch, $4
        FROM security_spaces s
        JOIN security_space_members member
          ON member.space_id = s.id
         AND member.user_id = $2
         AND member.status = 'active'
        WHERE s.id = $1 AND s.status = 'active' AND s.current_key_epoch = $3
        ON CONFLICT (space_id, user_id, key_epoch) DO UPDATE SET
            encrypted_key_package = EXCLUDED.encrypted_key_package,
            created_at = now()
        "#,
    )
    .bind(space_id)
    .bind(user_id)
    .bind(i32::try_from(key_epoch)?)
    .bind(encrypted_key_package)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn list_recovery_key_packages(
    pool: &PgPool,
    user_id: Uuid,
) -> anyhow::Result<Vec<RecoverySpaceKeyPackage>> {
    let rows = sqlx::query(
        r#"
        SELECT package.space_id, package.key_epoch, package.encrypted_key_package
        FROM security_space_recovery_keys package
        JOIN security_spaces space
          ON space.id = package.space_id
         AND space.current_key_epoch = package.key_epoch
        JOIN security_space_members member
          ON member.space_id = package.space_id
         AND member.user_id = package.user_id
         AND member.status = 'active'
        WHERE package.user_id = $1
          AND (space.status = 'active' OR space.deleted_at > now() - interval '30 days')
        ORDER BY package.space_id
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|row| {
            let epoch: i32 = row.try_get("key_epoch")?;
            Ok(RecoverySpaceKeyPackage {
                space_id: row.try_get("space_id")?,
                key_epoch: u32::try_from(epoch)?,
                encrypted_key_package: row.try_get("encrypted_key_package")?,
            })
        })
        .collect()
}

pub(crate) async fn list_spaces(pool: &PgPool, user_id: Uuid) -> anyhow::Result<Vec<SpaceSummary>> {
    let rows = sqlx::query(
        r#"
        SELECT s.id AS space_id, s.workspace_id, sm.role, s.current_key_epoch,
               s.encrypted_metadata,
               (extract(epoch FROM s.created_at) * 1000)::bigint AS created_at_ms
        FROM security_space_members sm
        JOIN security_spaces s ON s.id = sm.space_id
        WHERE sm.user_id = $1
          AND sm.status = 'active'
          AND s.status = 'active'
        ORDER BY s.created_at ASC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let mut spaces = Vec::with_capacity(rows.len());
    for row in rows {
        let role: String = row.try_get("role")?;
        let key_epoch: i32 = row.try_get("current_key_epoch")?;
        let space_id: Uuid = row.try_get("space_id")?;
        spaces.push(SpaceSummary {
            space_id,
            workspace_id: row.try_get("workspace_id")?,
            role: SpaceRole::from_db(&role)?,
            key_epoch: u32::try_from(key_epoch)?,
            encrypted_metadata: row.try_get("encrypted_metadata")?,
            device_key_packages: list_device_key_packages(pool, user_id, space_id).await?,
            created_at_unix_ms: row.try_get("created_at_ms")?,
        });
    }
    Ok(spaces)
}

pub(crate) async fn list_trashed_spaces(
    pool: &PgPool,
    user_id: Uuid,
) -> anyhow::Result<Vec<SpaceSummary>> {
    let rows = sqlx::query(
        r#"
        SELECT s.id AS space_id, s.workspace_id, sm.role, s.current_key_epoch,
               s.encrypted_metadata,
               (extract(epoch FROM s.created_at) * 1000)::bigint AS created_at_ms
        FROM security_space_members sm
        JOIN security_spaces s ON s.id = sm.space_id
        WHERE sm.user_id = $1
          AND sm.status = 'active'
          AND s.owner_user_id = $1
          AND s.status = 'deleted'
          AND s.deleted_at > now() - interval '30 days'
        ORDER BY s.deleted_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let mut spaces = Vec::with_capacity(rows.len());
    for row in rows {
        let role: String = row.try_get("role")?;
        let key_epoch: i32 = row.try_get("current_key_epoch")?;
        let space_id: Uuid = row.try_get("space_id")?;
        spaces.push(SpaceSummary {
            space_id,
            workspace_id: row.try_get("workspace_id")?,
            role: SpaceRole::from_db(&role)?,
            key_epoch: u32::try_from(key_epoch)?,
            encrypted_metadata: row.try_get("encrypted_metadata")?,
            device_key_packages: list_device_key_packages(pool, user_id, space_id).await?,
            created_at_unix_ms: row.try_get("created_at_ms")?,
        });
    }
    Ok(spaces)
}

pub(crate) async fn move_to_trash(
    pool: &PgPool,
    user_id: Uuid,
    space_id: Uuid,
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE security_spaces space
        SET status = 'deleted', deleted_at = now()
        WHERE space.id = $1
          AND space.status = 'active'
          AND space.owner_user_id = $2
        "#,
    )
    .bind(space_id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn restore_from_trash(
    pool: &PgPool,
    user_id: Uuid,
    space_id: Uuid,
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE security_spaces space
        SET status = 'active', deleted_at = NULL
        WHERE space.id = $1
          AND space.status = 'deleted'
          AND space.deleted_at > now() - interval '30 days'
          AND space.owner_user_id = $2
        "#,
    )
    .bind(space_id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn list_device_key_packages(
    pool: &PgPool,
    user_id: Uuid,
    space_id: Uuid,
) -> anyhow::Result<Vec<DeviceKeyPackage>> {
    let rows = sqlx::query(
        r#"
        SELECT device_id, key_epoch, encrypted_key_package
        FROM security_space_device_keys
        WHERE space_id = $1 AND user_id = $2
        ORDER BY key_epoch DESC, created_at ASC
        "#,
    )
    .bind(space_id)
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|row| {
            let epoch: i32 = row.try_get("key_epoch")?;
            Ok(DeviceKeyPackage {
                device_id: row.try_get("device_id")?,
                key_epoch: u32::try_from(epoch)?,
                encrypted_key_package: row.try_get("encrypted_key_package")?,
            })
        })
        .collect()
}

pub(crate) async fn put_device_key_package(
    pool: &PgPool,
    user_id: Uuid,
    space_id: Uuid,
    package: &DeviceKeyPackage,
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        r#"
        INSERT INTO security_space_device_keys (
            space_id, user_id, device_id, key_epoch, encrypted_key_package
        )
        SELECT s.id, $2, d.id, s.current_key_epoch, $5
        FROM security_spaces s
        JOIN security_space_members sm
          ON sm.space_id = s.id AND sm.user_id = $2 AND sm.status = 'active'
        JOIN devices d
          ON d.id = $3 AND d.user_id = $2 AND d.status = 'active'
        WHERE s.id = $1 AND s.status = 'active' AND s.current_key_epoch = $4
        ON CONFLICT (space_id, device_id, key_epoch) DO UPDATE SET
            encrypted_key_package = EXCLUDED.encrypted_key_package
        "#,
    )
    .bind(space_id)
    .bind(user_id)
    .bind(package.device_id)
    .bind(i32::try_from(package.key_epoch)?)
    .bind(&package.encrypted_key_package)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn list_members(
    pool: &PgPool,
    actor_id: Uuid,
    space_id: Uuid,
) -> anyhow::Result<Option<Vec<SpaceMemberSummary>>> {
    let actor_can_read: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM security_space_members WHERE space_id = $1 AND user_id = $2 AND status = 'active')",
    )
    .bind(space_id)
    .bind(actor_id)
    .fetch_one(pool)
    .await?;
    if !actor_can_read {
        return Ok(None);
    }
    let rows = sqlx::query(
        r#"
        SELECT member.user_id, users.username, member.role, member.key_epoch
        FROM security_space_members member
        JOIN users ON users.id = member.user_id
                  AND users.deleted_at IS NULL AND users.suspended_at IS NULL
        WHERE member.space_id = $1 AND member.status = 'active'
        ORDER BY member.created_at ASC
        "#,
    )
    .bind(space_id)
    .fetch_all(pool)
    .await?;
    let members = rows
        .iter()
        .map(|row| {
            let role: String = row.try_get("role")?;
            let epoch: i32 = row.try_get("key_epoch")?;
            Ok(SpaceMemberSummary {
                user_id: row.try_get("user_id")?,
                username: row.try_get("username")?,
                role: SpaceRole::from_db(&role)?,
                key_epoch: u32::try_from(epoch)?,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(Some(members))
}

pub(crate) async fn list_space_devices(
    pool: &PgPool,
    actor_id: Uuid,
    space_id: Uuid,
) -> anyhow::Result<Option<Vec<SpaceDeviceSummary>>> {
    let actor_can_read: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM security_space_members WHERE space_id = $1 AND user_id = $2 AND status = 'active')",
    )
    .bind(space_id)
    .bind(actor_id)
    .fetch_one(pool)
    .await?;
    if !actor_can_read {
        return Ok(None);
    }
    let rows = sqlx::query(
        r#"
        SELECT DISTINCT d.id, d.signing_public_key
        FROM devices d
        WHERE EXISTS (
            SELECT 1
            FROM operation_log operation
            WHERE operation.space_id = $1 AND operation.author_device_id = d.id
        ) OR EXISTS (
            SELECT 1
            FROM security_space_members member
            WHERE member.space_id = $1
              AND member.user_id = d.user_id
              AND member.status = 'active'
              AND d.status = 'active'
        )
        ORDER BY d.id
        "#,
    )
    .bind(space_id)
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|row| {
            Ok(SpaceDeviceSummary {
                device_id: row.try_get("id")?,
                signing_public_key: row.try_get("signing_public_key")?,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()
        .map(Some)
}

pub(crate) enum RevokeMemberResult {
    Revoked,
    AccessDenied,
    EpochConflict,
    TargetNotFound,
    CannotRevokeOwner,
    PackageCoverageMismatch,
}

pub(crate) struct MemberRotation<'a> {
    pub(crate) actor_id: Uuid,
    pub(crate) space_id: Uuid,
    pub(crate) target_user_id: Uuid,
    pub(crate) expected_key_epoch: u32,
    pub(crate) new_key_epoch: u32,
    pub(crate) rotation_id: Uuid,
    pub(crate) new_encrypted_metadata: &'a [u8],
    pub(crate) packages: &'a [DeviceKeyPackage],
    pub(crate) recovery_packages: &'a [MemberRecoveryKeyPackage],
    pub(crate) snapshots: &'a [OperationEnvelopeV1],
}

pub(crate) async fn revoke_member_and_rotate(
    pool: &PgPool,
    rotation: MemberRotation<'_>,
) -> anyhow::Result<RevokeMemberResult> {
    let MemberRotation {
        actor_id,
        space_id,
        target_user_id,
        expected_key_epoch,
        new_key_epoch,
        rotation_id,
        new_encrypted_metadata,
        packages,
        recovery_packages,
        snapshots,
    } = rotation;
    let mut tx = pool.begin().await?;
    let row = sqlx::query(
        r#"
        SELECT s.current_key_epoch, s.owner_user_id, target.role AS target_role
        FROM security_spaces s
        JOIN security_space_members actor
          ON actor.space_id = s.id AND actor.user_id = $2 AND actor.status = 'active'
        LEFT JOIN security_space_members target
          ON target.space_id = s.id AND target.user_id = $3 AND target.status = 'active'
        WHERE s.id = $1 AND s.status = 'active'
        FOR UPDATE OF s
        "#,
    )
    .bind(space_id)
    .bind(actor_id)
    .bind(target_user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        tx.rollback().await?;
        return Ok(RevokeMemberResult::AccessDenied);
    };
    let owner_user_id: Uuid = row.try_get("owner_user_id")?;
    if owner_user_id != actor_id {
        tx.rollback().await?;
        return Ok(RevokeMemberResult::AccessDenied);
    }
    let target_role: Option<String> = row.try_get("target_role")?;
    let Some(target_role) = target_role else {
        tx.rollback().await?;
        return Ok(RevokeMemberResult::TargetNotFound);
    };
    if target_role == "owner" {
        tx.rollback().await?;
        return Ok(RevokeMemberResult::CannotRevokeOwner);
    }
    let current_epoch: i32 = row.try_get("current_key_epoch")?;
    if u32::try_from(current_epoch)? != expected_key_epoch
        || new_key_epoch != expected_key_epoch.saturating_add(1)
    {
        tx.rollback().await?;
        return Ok(RevokeMemberResult::EpochConflict);
    }

    let remaining_devices: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT DISTINCT sdk.device_id
        FROM security_space_device_keys sdk
        JOIN devices d ON d.id = sdk.device_id AND d.status = 'active'
        JOIN security_space_members sm
          ON sm.space_id = sdk.space_id
         AND sm.user_id = sdk.user_id
         AND sm.status = 'active'
        WHERE sdk.space_id = $1 AND sdk.user_id <> $2
        "#,
    )
    .bind(space_id)
    .bind(target_user_id)
    .fetch_all(&mut *tx)
    .await?;
    let expected: HashSet<Uuid> = remaining_devices.into_iter().collect();
    let supplied: HashMap<Uuid, &[u8]> = packages
        .iter()
        .map(|package| (package.device_id, package.encrypted_key_package.as_slice()))
        .collect();
    if supplied.len() != packages.len()
        || supplied.keys().copied().collect::<HashSet<_>>() != expected
    {
        tx.rollback().await?;
        return Ok(RevokeMemberResult::PackageCoverageMismatch);
    }

    let remaining_users: HashSet<Uuid> = sqlx::query_scalar(
        r#"
        SELECT user_id FROM security_space_members
        WHERE space_id = $1 AND user_id <> $2 AND status = 'active'
        "#,
    )
    .bind(space_id)
    .bind(target_user_id)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();
    let supplied_recovery: HashMap<Uuid, &[u8]> = recovery_packages
        .iter()
        .map(|package| (package.user_id, package.encrypted_key_package.as_slice()))
        .collect();
    if supplied_recovery.len() != recovery_packages.len()
        || supplied_recovery.keys().copied().collect::<HashSet<_>>() != remaining_users
    {
        tx.rollback().await?;
        return Ok(RevokeMemberResult::PackageCoverageMismatch);
    }

    let expected_streams: HashSet<Uuid> =
        sqlx::query_scalar("SELECT DISTINCT stream_id FROM operation_log WHERE space_id = $1")
            .bind(space_id)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .collect();
    let supplied_streams: HashSet<Uuid> = snapshots.iter().map(|item| item.stream_id).collect();
    if supplied_streams.len() != snapshots.len() || supplied_streams != expected_streams {
        tx.rollback().await?;
        return Ok(RevokeMemberResult::PackageCoverageMismatch);
    }

    let epoch = i32::try_from(new_key_epoch)?;
    sqlx::query(
        r#"
        INSERT INTO security_space_epochs (
            space_id, key_epoch, rotation_id, status, created_by
        ) VALUES ($1, $2, $3, 'preparing', $4)
        "#,
    )
    .bind(space_id)
    .bind(epoch)
    .bind(rotation_id)
    .bind(actor_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE security_space_members SET status = 'revoked', revoked_at = now() WHERE space_id = $1 AND user_id = $2 AND status = 'active'",
    )
    .bind(space_id)
    .bind(target_user_id)
    .execute(&mut *tx)
    .await?;

    for (device_id, encrypted_package) in supplied {
        let inserted = sqlx::query(
            r#"
            INSERT INTO security_space_device_keys (
                space_id, user_id, device_id, key_epoch, encrypted_key_package
            )
            SELECT $1, d.user_id, d.id, $3, $4
            FROM devices d
            JOIN security_space_members sm
              ON sm.space_id = $1 AND sm.user_id = d.user_id AND sm.status = 'active'
            WHERE d.id = $2 AND d.status = 'active'
            "#,
        )
        .bind(space_id)
        .bind(device_id)
        .bind(epoch)
        .bind(encrypted_package)
        .execute(&mut *tx)
        .await?;
        if inserted.rows_affected() != 1 {
            anyhow::bail!("remaining device became inactive during key rotation");
        }
    }
    for (user_id, encrypted_package) in supplied_recovery {
        sqlx::query(
            r#"
            INSERT INTO security_space_recovery_keys (
                space_id, user_id, key_epoch, encrypted_key_package
            ) VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(space_id)
        .bind(user_id)
        .bind(epoch)
        .bind(encrypted_package)
        .execute(&mut *tx)
        .await?;
    }

    for snapshot in snapshots {
        let sequence: i64 = sqlx::query_scalar(
            "UPDATE security_spaces SET next_sequence = next_sequence + 1 WHERE id = $1 RETURNING next_sequence",
        )
        .bind(space_id)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            r#"
            INSERT INTO operation_log (
                space_id, space_seq, stream_id, client_op_id, author_device_id,
                key_epoch, envelope_kind, cipher_suite, nonce, ciphertext, signature
            ) VALUES ($1, $2, $3, $4, $5, $6, 'snapshot', $7, $8, $9, $10)
            "#,
        )
        .bind(space_id)
        .bind(sequence)
        .bind(snapshot.stream_id)
        .bind(snapshot.client_op_id)
        .bind(snapshot.author_device_id)
        .bind(epoch)
        .bind(snapshot.cipher_suite.as_db_value())
        .bind(&snapshot.nonce)
        .bind(&snapshot.ciphertext)
        .bind(&snapshot.signature)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query(
        r#"
        UPDATE security_space_invites
        SET revoked_at = now()
        WHERE space_id = $1 AND revoked_at IS NULL AND used_count < max_uses
        "#,
    )
    .bind(space_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE security_space_members SET key_epoch = $2 WHERE space_id = $1 AND status = 'active'",
    )
    .bind(space_id)
    .bind(epoch)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE security_spaces SET current_key_epoch = $2, encrypted_metadata = $3 WHERE id = $1",
    )
    .bind(space_id)
    .bind(epoch)
    .bind(new_encrypted_metadata)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        r#"
        UPDATE security_space_epochs
        SET status = 'superseded', superseded_at = now()
        WHERE space_id = $1 AND key_epoch = $2
        "#,
    )
    .bind(space_id)
    .bind(current_epoch)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        r#"
        UPDATE security_space_epochs
        SET status = 'committed', committed_at = now()
        WHERE space_id = $1 AND key_epoch = $2 AND rotation_id = $3
        "#,
    )
    .bind(space_id)
    .bind(epoch)
    .bind(rotation_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(RevokeMemberResult::Revoked)
}
