//! Workspace access-control policy helpers.

use crate::features::{
    common::{ApiError, unauthorized},
    workspaces::dto::WorkspaceRole,
};

/// Returns whether actor role can manage workspace members.
pub(crate) fn can_manage_members(actor_role: WorkspaceRole) -> bool {
    matches!(actor_role, WorkspaceRole::Owner | WorkspaceRole::Admin)
}

/// Enforces that actor is allowed to manage members.
pub(crate) fn ensure_can_manage_members(actor_role: WorkspaceRole) -> Result<(), ApiError> {
    if can_manage_members(actor_role) {
        return Ok(());
    }
    Err(unauthorized("workspace member management forbidden"))
}

/// Enforces role-update constraints for target user.
pub(crate) fn ensure_role_update_allowed(
    _actor_role: WorkspaceRole,
    target_role: WorkspaceRole,
    new_role: WorkspaceRole,
) -> Result<(), ApiError> {
    if target_role == WorkspaceRole::Owner {
        return Err(unauthorized("cannot modify workspace owner"));
    }
    if new_role == WorkspaceRole::Owner {
        return Err(unauthorized(
            "workspace ownership must be transferred and accepted explicitly",
        ));
    }
    Ok(())
}

/// Enforces revoke constraints for target user.
pub(crate) fn ensure_revoke_allowed(target_role: WorkspaceRole) -> Result<(), ApiError> {
    if target_role == WorkspaceRole::Owner {
        return Err(unauthorized("cannot revoke workspace owner"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn member_management_requires_owner_or_admin() {
        assert!(can_manage_members(WorkspaceRole::Owner));
        assert!(can_manage_members(WorkspaceRole::Admin));
        assert!(!can_manage_members(WorkspaceRole::Member));
    }
}
