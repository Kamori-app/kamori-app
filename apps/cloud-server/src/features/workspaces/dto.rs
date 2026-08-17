//! MessagePack DTOs for workspace primitives.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Workspace kind.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceKind {
    /// One personal workspace per user.
    Personal,
    /// Team/shared workspace.
    Team,
}

impl TryFrom<&str> for WorkspaceKind {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "personal" => Ok(Self::Personal),
            "team" => Ok(Self::Team),
            _ => Err("invalid workspace kind"),
        }
    }
}

/// Workspace member role.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceRole {
    /// Workspace owner.
    Owner,
    /// Workspace administrator.
    Admin,
    /// Regular member.
    Member,
}

impl WorkspaceRole {
    pub(crate) fn as_db_value(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
            Self::Member => "member",
        }
    }
}

impl TryFrom<&str> for WorkspaceRole {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "owner" => Ok(Self::Owner),
            "admin" => Ok(Self::Admin),
            "member" => Ok(Self::Member),
            _ => Err("invalid workspace role"),
        }
    }
}

/// Create-team-workspace request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWorkspaceRequest {
    /// Client-encrypted workspace metadata blob.
    #[serde(with = "serde_bytes")]
    pub encrypted_metadata: Vec<u8>,
}

/// Create-team-workspace response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWorkspaceResponse {
    /// Created workspace id.
    pub workspace_id: Uuid,
}

/// List-workspaces request.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ListWorkspacesRequest {}

/// Workspace summary for the current actor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSummary {
    /// Workspace id.
    pub workspace_id: Uuid,
    /// Workspace kind.
    pub kind: WorkspaceKind,
    /// Current actor role in workspace.
    pub role: WorkspaceRole,
    /// Client-encrypted workspace metadata.
    #[serde(with = "serde_bytes")]
    pub encrypted_metadata: Vec<u8>,
}

/// List-workspaces response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListWorkspacesResponse {
    /// Workspaces visible to current actor.
    pub workspaces: Vec<WorkspaceSummary>,
}

/// List-members request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListWorkspaceMembersRequest {
    /// Workspace id.
    pub workspace_id: Uuid,
}

/// Workspace member entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceMember {
    /// Member user id.
    pub user_id: Uuid,
    /// Current username.
    pub username: String,
    /// Role in workspace.
    pub role: WorkspaceRole,
}

/// List-members response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListWorkspaceMembersResponse {
    /// Active workspace members.
    pub members: Vec<WorkspaceMember>,
}

/// Update member role request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateWorkspaceMemberRoleRequest {
    /// Workspace id.
    pub workspace_id: Uuid,
    /// Target member user id.
    pub user_id: Uuid,
    /// New role.
    pub role: WorkspaceRole,
}

/// Update member role response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateWorkspaceMemberRoleResponse {
    /// Whether target member was updated.
    pub updated: bool,
}

/// Revoke member request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevokeWorkspaceMemberRequest {
    /// Workspace id.
    pub workspace_id: Uuid,
    /// Target member user id.
    pub user_id: Uuid,
}

/// Revoke member response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevokeWorkspaceMemberResponse {
    /// Whether target member status was changed to revoked.
    pub revoked: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_create_request_msgpack_roundtrip() {
        let req = CreateWorkspaceRequest {
            encrypted_metadata: vec![1, 2, 3, 4],
        };
        let bytes = rmp_serde::to_vec_named(&req).expect("encode");
        let decoded: CreateWorkspaceRequest = rmp_serde::from_slice(&bytes).expect("decode");
        assert_eq!(decoded.encrypted_metadata, vec![1, 2, 3, 4]);
    }

    #[test]
    fn workspace_enums_serialize_as_snake_case() {
        let kind = WorkspaceKind::Team;
        let role = WorkspaceRole::Owner;
        let kind_json = serde_json::to_string(&kind).expect("kind json");
        let role_json = serde_json::to_string(&role).expect("role json");
        assert_eq!(kind_json, "\"team\"");
        assert_eq!(role_json, "\"owner\"");
    }

    #[test]
    fn workspace_member_management_requests_roundtrip() {
        let update = UpdateWorkspaceMemberRoleRequest {
            workspace_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            role: WorkspaceRole::Admin,
        };
        let bin = rmp_serde::to_vec_named(&update).expect("encode update");
        let decoded: UpdateWorkspaceMemberRoleRequest =
            rmp_serde::from_slice(&bin).expect("decode update");
        assert_eq!(decoded.role, WorkspaceRole::Admin);

        let revoke = RevokeWorkspaceMemberRequest {
            workspace_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
        };
        let bin = rmp_serde::to_vec_named(&revoke).expect("encode revoke");
        let decoded: RevokeWorkspaceMemberRequest =
            rmp_serde::from_slice(&bin).expect("decode revoke");
        assert_eq!(decoded.workspace_id, revoke.workspace_id);
        assert_eq!(decoded.user_id, revoke.user_id);
    }
}
