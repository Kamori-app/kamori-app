//! Session and dashboard commands.
use tauri::State;
use zeroize::Zeroize;

use crate::{
    models::{DashboardSnapshot, LogoutResult},
    state::DesktopState,
};

use super::common::{
    clear_refresh_token_secure, clear_session_username_secure, revoke_refresh_session, to_ui_error,
};

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
    let refresh_token = state.refresh_token().await;
    let server_result = match refresh_token.as_deref() {
        Some(refresh_token) => revoke_refresh_session(&cloud_base_url, refresh_token).await,
        None => Ok(false),
    };

    if let Some(handle) = state.sync_task.lock().await.take() {
        handle.abort();
        let _ = handle.await;
    }
    let mut local_warnings = Vec::new();
    if let Some(runner) = state.bridge.lock().await.take() {
        if let Err(error) = runner.clear_persisted_credentials().await {
            local_warnings.push(format!(
                "Encrypted cached credentials could not be removed: {}",
                to_ui_error(error)
            ));
        }
        if let Err(error) = runner.stop().await {
            local_warnings.push(format!(
                "The local DAV bridge could not be stopped cleanly: {}",
                to_ui_error(error)
            ));
        }
    }
    let keychain_warning = clear_refresh_token_secure(&cloud_base_url).err();
    let username_keychain_warning = clear_session_username_secure(&cloud_base_url).err();
    if let Some(mut token) = state.access_token.write().await.take() {
        token.zeroize();
    }
    if let Some(mut token) = state.refresh_token.write().await.take() {
        token.zeroize();
    }
    state.set_username(None).await;
    let mut collections = state.collections.write().await;
    for (_, mut collection) in collections.drain() {
        collection.name.zeroize();
        collection.cmk.zeroize();
    }
    drop(collections);
    if let Some(mut identity) = state.device_identity.write().await.take() {
        identity.signing_private_key.zeroize();
    }
    if let Some((mut username, mut password)) = state.dav_credentials.write().await.take() {
        username.zeroize();
        password.zeroize();
    }
    if let Some(mut pending) = state.pending_totp_login.lock().await.take() {
        pending.username.zeroize();
        pending.continuation_token.zeroize();
        pending.export_key.zeroize();
    }
    *state.pending_browser_login.lock().await = None;
    let (server_session_revoked, server_warning) = match server_result {
        Ok(revoked) => (revoked, None),
        Err(error) => (
            false,
            Some(format!(
                "Local data was locked, but the server session could not be revoked: {error}"
            )),
        ),
    };
    let mut warnings = local_warnings;
    if let Some(server) = server_warning {
        warnings.push(server);
    }
    if let Some(keychain) = keychain_warning {
        warnings.push(format!(
            "Signed out, but the refresh token could not be removed from the keychain: {keychain}"
        ));
    }
    if let Some(keychain) = username_keychain_warning {
        warnings.push(format!(
            "Signed out, but session metadata could not be removed from the keychain: {keychain}"
        ));
    }
    let warning = (!warnings.is_empty()).then(|| warnings.join("; "));
    Ok(LogoutResult {
        server_session_revoked,
        warning,
    })
}
