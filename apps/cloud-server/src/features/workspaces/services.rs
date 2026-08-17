//! Service logic for workspace primitives.

use uuid::Uuid;

use crate::{
    features::{
        common::{ApiError, bad_request, unauthorized},
        workspaces::{
            access::{
                ensure_can_manage_members, ensure_revoke_allowed, ensure_role_update_allowed,
            },
            dto::{
                CreateWorkspaceRequest, CreateWorkspaceResponse, ListWorkspaceMembersRequest,
                ListWorkspaceMembersResponse, ListWorkspacesResponse, RevokeWorkspaceMemberRequest,
                RevokeWorkspaceMemberResponse, UpdateWorkspaceMemberRoleRequest,
                UpdateWorkspaceMemberRoleResponse,
            },
            repositories::{
                create_team_workspace_for_owner, ensure_personal_workspace_for_user,
                get_active_workspace_member_role, is_active_workspace_member,
                list_active_workspace_members, list_workspaces_for_user, revoke_workspace_member,
                update_workspace_member_role,
            },
        },
    },
    platform::state::AppState,
};

fn validate_workspace_metadata(encrypted_metadata: &[u8]) -> Result<(), ApiError> {
    if encrypted_metadata.is_empty() {
        return Err(bad_request("encrypted_metadata must not be empty"));
    }
    if encrypted_metadata.len() > 64 * 1024 {
        return Err(bad_request("encrypted_metadata is too large"));
    }
    Ok(())
}

pub(crate) async fn create_workspace(
    state: &AppState,
    actor_id: Uuid,
    payload: CreateWorkspaceRequest,
) -> Result<CreateWorkspaceResponse, ApiError> {
    validate_workspace_metadata(&payload.encrypted_metadata)?;
    let workspace_id =
        create_team_workspace_for_owner(&state.pool, actor_id, &payload.encrypted_metadata).await?;
    Ok(CreateWorkspaceResponse { workspace_id })
}

pub(crate) async fn list_workspaces(
    state: &AppState,
    actor_id: Uuid,
) -> Result<ListWorkspacesResponse, ApiError> {
    ensure_personal_workspace_for_user(&state.pool, actor_id).await?;
    let workspaces = list_workspaces_for_user(&state.pool, actor_id).await?;
    Ok(ListWorkspacesResponse { workspaces })
}

pub(crate) async fn list_workspace_members(
    state: &AppState,
    actor_id: Uuid,
    payload: ListWorkspaceMembersRequest,
) -> Result<ListWorkspaceMembersResponse, ApiError> {
    let is_member = is_active_workspace_member(&state.pool, payload.workspace_id, actor_id).await?;
    if !is_member {
        return Err(unauthorized("workspace access denied"));
    }

    let members = list_active_workspace_members(&state.pool, payload.workspace_id).await?;
    Ok(ListWorkspaceMembersResponse { members })
}

pub(crate) async fn update_member_role(
    state: &AppState,
    actor_id: Uuid,
    payload: UpdateWorkspaceMemberRoleRequest,
) -> Result<UpdateWorkspaceMemberRoleResponse, ApiError> {
    let actor_role = get_active_workspace_member_role(&state.pool, payload.workspace_id, actor_id)
        .await?
        .ok_or_else(|| unauthorized("workspace access denied"))?;
    ensure_can_manage_members(actor_role)?;

    let target_role =
        get_active_workspace_member_role(&state.pool, payload.workspace_id, payload.user_id)
            .await?
            .ok_or_else(|| unauthorized("workspace member not found"))?;
    ensure_role_update_allowed(actor_role, target_role, payload.role)?;

    let updated = update_workspace_member_role(
        &state.pool,
        payload.workspace_id,
        payload.user_id,
        payload.role,
    )
    .await?;
    Ok(UpdateWorkspaceMemberRoleResponse { updated })
}

pub(crate) async fn revoke_member(
    state: &AppState,
    actor_id: Uuid,
    payload: RevokeWorkspaceMemberRequest,
) -> Result<RevokeWorkspaceMemberResponse, ApiError> {
    let actor_role = get_active_workspace_member_role(&state.pool, payload.workspace_id, actor_id)
        .await?
        .ok_or_else(|| unauthorized("workspace access denied"))?;
    ensure_can_manage_members(actor_role)?;

    let target_role =
        get_active_workspace_member_role(&state.pool, payload.workspace_id, payload.user_id)
            .await?
            .ok_or_else(|| unauthorized("workspace member not found"))?;
    ensure_revoke_allowed(target_role)?;

    let revoked =
        revoke_workspace_member(&state.pool, payload.workspace_id, payload.user_id).await?;
    Ok(RevokeWorkspaceMemberResponse { revoked })
}

#[cfg(test)]
mod tests {
    use super::validate_workspace_metadata;

    #[test]
    fn metadata_validation_enforces_non_empty_and_limit() {
        let one_byte = [0u8; 1];
        let max_allowed = [0u8; 64 * 1024];
        let too_large = [0u8; 64 * 1024 + 1];

        assert!(validate_workspace_metadata(&[]).is_err());
        assert!(validate_workspace_metadata(&one_byte).is_ok());
        assert!(validate_workspace_metadata(&max_allowed).is_ok());
        assert!(validate_workspace_metadata(&too_large).is_err());
    }
}
