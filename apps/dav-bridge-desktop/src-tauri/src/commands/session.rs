//! Session and dashboard commands.
use tauri::State;

use crate::{
    models::{DashboardSnapshot, LogoutResult},
    state::DesktopState,
};

use super::common::{
    MSGPACK_CONTENT_TYPE, clear_refresh_token_secure, decode_msgpack, encode_msgpack, endpoint,
    to_ui_error,
};

#[derive(serde::Serialize)]
struct LogoutRequest {
    refresh_token: Option<String>,
}

#[derive(serde::Deserialize)]
struct LogoutResponse {
    revoked: bool,
}

/// Returns a dashboard snapshot used by SPA cards.
#[tauri::command]
pub async fn dashboard_snapshot(
    state: State<'_, DesktopState>,
) -> Result<DashboardSnapshot, String> {
    Ok(state.snapshot().await)
}

/// Clears active session and stops local server.
#[tauri::command]
pub async fn logout(state: State<'_, DesktopState>) -> Result<LogoutResult, String> {
    let cloud_base_url = state.cloud_base_url().await;
    let access_token = state.access_token.read().await.clone();
    let refresh_token = state.refresh_token().await;
    let server_result = match (access_token, refresh_token.clone()) {
        (Some(access_token), Some(refresh_token)) => {
            let body = encode_msgpack(&LogoutRequest {
                refresh_token: Some(refresh_token),
            })?;
            async {
                let response = reqwest::Client::new()
                    .post(endpoint(&cloud_base_url, "/auth/logout"))
                    .bearer_auth(access_token)
                    .header(reqwest::header::CONTENT_TYPE, MSGPACK_CONTENT_TYPE)
                    .header(reqwest::header::ACCEPT, MSGPACK_CONTENT_TYPE)
                    .body(body)
                    .send()
                    .await
                    .map_err(to_ui_error)?
                    .error_for_status()
                    .map_err(to_ui_error)?;
                decode_msgpack::<LogoutResponse>(response).await
            }
            .await
        }
        _ => Ok(LogoutResponse { revoked: false }),
    };

    if let Some(handle) = state.sync_task.lock().await.take() {
        handle.abort();
        let _ = handle.await;
    }
    if let Some(runner) = state.bridge.lock().await.take() {
        runner.stop().await.map_err(to_ui_error)?;
    }
    let keychain_warning = clear_refresh_token_secure(&cloud_base_url).err();
    state.set_access_token(None).await;
    state.set_refresh_token(None).await;
    state.set_username(None).await;
    state.collections.write().await.clear();
    *state.device_identity.write().await = None;
    *state.dav_credentials.write().await = None;
    let (server_session_revoked, server_warning) = match server_result {
        Ok(response) => (response.revoked, None),
        Err(error) => (
            false,
            Some(format!(
                "Local data was locked, but the server session could not be revoked: {error}"
            )),
        ),
    };
    let warning = match (server_warning, keychain_warning) {
        (Some(server), Some(keychain)) => Some(format!("{server}; {keychain}")),
        (Some(server), None) => Some(server),
        (None, Some(keychain)) => Some(format!(
            "Signed out, but the refresh token could not be removed from the keychain: {keychain}"
        )),
        (None, None) => None,
    };
    Ok(LogoutResult {
        server_session_revoked,
        warning,
    })
}
