//! Workspace HTTP handlers.

use axum::{extract::State, http::HeaderMap};

use crate::{
    features::{
        common::{ApiError, MsgPack, authorize_session},
        workspaces::{
            dto::{
                CreateWorkspaceRequest, CreateWorkspaceResponse, ListWorkspaceMembersRequest,
                ListWorkspaceMembersResponse, ListWorkspacesRequest, ListWorkspacesResponse,
                RevokeWorkspaceMemberRequest, RevokeWorkspaceMemberResponse,
                UpdateWorkspaceMemberRoleRequest, UpdateWorkspaceMemberRoleResponse,
            },
            services,
        },
    },
    platform::state::AppState,
};

pub async fn create_workspace(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<CreateWorkspaceRequest>,
) -> Result<MsgPack<CreateWorkspaceResponse>, ApiError> {
    let actor_id = authorize_session(&state, &headers).await?;
    Ok(MsgPack(
        services::create_workspace(&state, actor_id, payload).await?,
    ))
}

pub async fn list_workspaces(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(_payload): MsgPack<ListWorkspacesRequest>,
) -> Result<MsgPack<ListWorkspacesResponse>, ApiError> {
    let actor_id = authorize_session(&state, &headers).await?;
    Ok(MsgPack(services::list_workspaces(&state, actor_id).await?))
}

pub async fn list_workspace_members(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<ListWorkspaceMembersRequest>,
) -> Result<MsgPack<ListWorkspaceMembersResponse>, ApiError> {
    let actor_id = authorize_session(&state, &headers).await?;
    Ok(MsgPack(
        services::list_workspace_members(&state, actor_id, payload).await?,
    ))
}

pub async fn update_workspace_member_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<UpdateWorkspaceMemberRoleRequest>,
) -> Result<MsgPack<UpdateWorkspaceMemberRoleResponse>, ApiError> {
    let actor_id = authorize_session(&state, &headers).await?;
    Ok(MsgPack(
        services::update_member_role(&state, actor_id, payload).await?,
    ))
}

pub async fn revoke_workspace_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    MsgPack(payload): MsgPack<RevokeWorkspaceMemberRequest>,
) -> Result<MsgPack<RevokeWorkspaceMemberResponse>, ApiError> {
    let actor_id = authorize_session(&state, &headers).await?;
    Ok(MsgPack(
        services::revoke_member(&state, actor_id, payload).await?,
    ))
}
