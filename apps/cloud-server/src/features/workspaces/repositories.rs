//! Repository functions for workspace and workspace_members tables.

use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::features::{
    common::{ApiError, internal_error},
    workspaces::dto::{WorkspaceKind, WorkspaceMember, WorkspaceRole, WorkspaceSummary},
};

/// Ensures a user has an active personal workspace and owner membership.
pub(crate) async fn ensure_personal_workspace_for_user(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Uuid, ApiError> {
    let existing: Option<Uuid> = sqlx::query_scalar(
        r#"
        SELECT id
        FROM workspaces
        WHERE owner_user_id = $1
          AND kind = 'personal'
          AND deleted_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(internal_error)?;
    if let Some(workspace_id) = existing {
        ensure_workspace_owner_membership(pool, workspace_id, user_id).await?;
        return Ok(workspace_id);
    }

    let workspace_id = Uuid::new_v4();
    let inserted: Option<Uuid> = sqlx::query_scalar(
        r#"
        INSERT INTO workspaces (id, owner_user_id, kind, encrypted_metadata)
        VALUES ($1, $2, 'personal', $3)
        ON CONFLICT DO NOTHING
        RETURNING id
        "#,
    )
    .bind(workspace_id)
    .bind(user_id)
    .bind(Vec::<u8>::new())
    .fetch_optional(pool)
    .await
    .map_err(internal_error)?;

    let personal_workspace_id = if let Some(id) = inserted {
        id
    } else {
        sqlx::query_scalar(
            r#"
            SELECT id
            FROM workspaces
            WHERE owner_user_id = $1
              AND kind = 'personal'
              AND deleted_at IS NULL
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| internal_error("failed to create personal workspace"))?
    };

    ensure_workspace_owner_membership(pool, personal_workspace_id, user_id).await?;
    Ok(personal_workspace_id)
}

async fn ensure_workspace_owner_membership(
    pool: &PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        INSERT INTO workspace_members (id, workspace_id, user_id, role, status)
        VALUES ($1, $2, $3, 'owner', 'active')
        ON CONFLICT (workspace_id, user_id)
        DO UPDATE SET role = 'owner', status = 'active'
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(workspace_id)
    .bind(user_id)
    .execute(pool)
    .await
    .map_err(internal_error)?;
    Ok(())
}

/// Creates a new team workspace with owner membership.
pub(crate) async fn create_team_workspace_for_owner(
    pool: &PgPool,
    owner_user_id: Uuid,
    encrypted_metadata: &[u8],
) -> Result<Uuid, ApiError> {
    let mut tx = pool.begin().await.map_err(internal_error)?;
    let workspace_id = Uuid::new_v4();

    sqlx::query(
        r#"
        INSERT INTO workspaces (id, owner_user_id, kind, encrypted_metadata)
        VALUES ($1, $2, 'team', $3)
        "#,
    )
    .bind(workspace_id)
    .bind(owner_user_id)
    .bind(encrypted_metadata)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    sqlx::query(
        r#"
        INSERT INTO workspace_members (id, workspace_id, user_id, role, status)
        VALUES ($1, $2, $3, $4, 'active')
        ON CONFLICT (workspace_id, user_id)
        DO UPDATE SET role = EXCLUDED.role, status = 'active'
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(workspace_id)
    .bind(owner_user_id)
    .bind(WorkspaceRole::Owner.as_db_value())
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;
    Ok(workspace_id)
}

/// Lists active workspaces for a user.
pub(crate) async fn list_workspaces_for_user(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Vec<WorkspaceSummary>, ApiError> {
    let rows = sqlx::query(
        r#"
        SELECT w.id AS workspace_id, w.kind AS kind, wm.role AS role, w.encrypted_metadata
        FROM workspace_members wm
        JOIN workspaces w ON w.id = wm.workspace_id
        WHERE wm.user_id = $1
          AND wm.status = 'active'
          AND w.deleted_at IS NULL
        ORDER BY w.created_at ASC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(internal_error)?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let workspace_id: Uuid = row.try_get("workspace_id").map_err(internal_error)?;
        let kind_raw: String = row.try_get("kind").map_err(internal_error)?;
        let role_raw: String = row.try_get("role").map_err(internal_error)?;
        let encrypted_metadata: Vec<u8> =
            row.try_get("encrypted_metadata").map_err(internal_error)?;
        let kind = WorkspaceKind::try_from(kind_raw.as_str()).map_err(internal_error)?;
        let role = WorkspaceRole::try_from(role_raw.as_str()).map_err(internal_error)?;
        out.push(WorkspaceSummary {
            workspace_id,
            kind,
            role,
            encrypted_metadata,
        });
    }
    Ok(out)
}

/// Checks whether user is an active member of workspace.
pub(crate) async fn is_active_workspace_member(
    pool: &PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<bool, ApiError> {
    let exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM workspace_members
            WHERE workspace_id = $1
              AND user_id = $2
              AND status = 'active'
        )
        "#,
    )
    .bind(workspace_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(internal_error)?;
    Ok(exists)
}

/// Returns role for active workspace member.
pub(crate) async fn get_active_workspace_member_role(
    pool: &PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<Option<WorkspaceRole>, ApiError> {
    let role_raw: Option<String> = sqlx::query_scalar(
        r#"
        SELECT role
        FROM workspace_members
        WHERE workspace_id = $1
          AND user_id = $2
          AND status = 'active'
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(internal_error)?;

    match role_raw {
        Some(raw) => {
            let role = WorkspaceRole::try_from(raw.as_str()).map_err(internal_error)?;
            Ok(Some(role))
        }
        None => Ok(None),
    }
}

/// Lists active members of a workspace.
pub(crate) async fn list_active_workspace_members(
    pool: &PgPool,
    workspace_id: Uuid,
) -> Result<Vec<WorkspaceMember>, ApiError> {
    let rows = sqlx::query(
        r#"
        SELECT wm.user_id, u.username, wm.role
        FROM workspace_members wm
        JOIN users u ON u.id = wm.user_id
        WHERE wm.workspace_id = $1
          AND wm.status = 'active'
          AND u.deleted_at IS NULL
          AND u.suspended_at IS NULL
        ORDER BY wm.created_at ASC
        "#,
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await
    .map_err(internal_error)?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let user_id: Uuid = row.try_get("user_id").map_err(internal_error)?;
        let username: String = row.try_get("username").map_err(internal_error)?;
        let role_raw: String = row.try_get("role").map_err(internal_error)?;
        let role = WorkspaceRole::try_from(role_raw.as_str()).map_err(internal_error)?;
        out.push(WorkspaceMember {
            user_id,
            username,
            role,
        });
    }
    Ok(out)
}

/// Updates role for an active workspace member.
pub(crate) async fn update_workspace_member_role(
    pool: &PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
    role: WorkspaceRole,
) -> Result<bool, ApiError> {
    let result = sqlx::query(
        r#"
        UPDATE workspace_members
        SET role = $3
        WHERE workspace_id = $1
          AND user_id = $2
          AND status = 'active'
        "#,
    )
    .bind(workspace_id)
    .bind(user_id)
    .bind(role.as_db_value())
    .execute(pool)
    .await
    .map_err(internal_error)?;
    Ok(result.rows_affected() > 0)
}

/// Revokes an active workspace member.
pub(crate) async fn revoke_workspace_member(
    pool: &PgPool,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<bool, ApiError> {
    let result = sqlx::query(
        r#"
        UPDATE workspace_members
        SET status = 'revoked'
        WHERE workspace_id = $1
          AND user_id = $2
          AND status = 'active'
        "#,
    )
    .bind(workspace_id)
    .bind(user_id)
    .execute(pool)
    .await
    .map_err(internal_error)?;
    Ok(result.rows_affected() > 0)
}
