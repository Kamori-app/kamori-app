//! Window and tray preference commands.
use crate::state::{CloseBehavior, DesktopState};
use std::sync::mpsc;
use tauri::{
    AppHandle, State,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

use super::common::{
    MAIN_TRAY_ID, clear_refresh_token_secure, clear_session_username_secure, reveal_main_window,
    to_ui_error,
};

fn normalize_cloud_base_url(value: &str) -> Result<String, String> {
    let mut parsed = url::Url::parse(value.trim()).map_err(to_ui_error)?;
    let host = parsed
        .host_str()
        .ok_or_else(|| "Kamori service URL must include a host".to_string())?;
    let loopback = matches!(host, "localhost" | "127.0.0.1" | "::1");
    if parsed.scheme() != "https" && !(parsed.scheme() == "http" && loopback) {
        return Err("remote Kamori service URLs must use HTTPS".to_string());
    }
    if !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(
            "Kamori service URL must not contain credentials, query, or fragment".to_string(),
        );
    }
    if !parsed.path().bytes().all(|byte| byte == b'/') {
        return Err("Kamori service URL must be an origin without a path".to_string());
    }
    parsed.set_path("");
    Ok(parsed.as_str().trim_end_matches('/').to_string())
}

fn ensure_tray_icon(app: &AppHandle) -> Result<(), String> {
    if app.tray_by_id(MAIN_TRAY_ID).is_some() {
        return Ok(());
    }

    let mut builder = TrayIconBuilder::with_id(MAIN_TRAY_ID)
        .tooltip("Kamori DAV Bridge")
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }
            | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => {
                let _ = reveal_main_window(tray.app_handle());
            }
            _ => {}
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    builder.build(app).map_err(to_ui_error)?;
    Ok(())
}

fn remove_tray_icon(app: &AppHandle) {
    let _ = app.remove_tray_by_id(MAIN_TRAY_ID);
}

/// Executes a UI operation on Tauri's main thread and waits for completion.
///
/// AppKit-backed operations (tray icon, window visibility, etc.) must run on the
/// main thread on macOS. This helper keeps command handlers deterministic by
/// propagating the closure result back to the caller.
fn run_ui_on_main_thread<F>(app: &AppHandle, task: F) -> Result<(), String>
where
    F: FnOnce(&AppHandle) -> Result<(), String> + Send + 'static,
{
    let app_handle = app.clone();
    let (tx, rx) = mpsc::channel();

    app.run_on_main_thread(move || {
        let result = task(&app_handle);
        let _ = tx.send(result);
    })
    .map_err(to_ui_error)?;

    rx.recv()
        .map_err(|error| format!("failed to receive UI operation result: {error}"))?
}

/// Synchronizes tray icon presence with the provided boolean preference.
pub(crate) fn sync_tray_icon(app: &AppHandle, enabled: bool) -> Result<(), String> {
    run_ui_on_main_thread(app, move |app| {
        if enabled {
            ensure_tray_icon(app)
        } else {
            remove_tray_icon(app);
            Ok(())
        }
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NormalizedWindowPreferences {
    close_behavior: CloseBehavior,
    show_tray_icon: bool,
}

/// Normalizes incoming window preferences to a safe runtime configuration.
///
/// `Hide` close behavior always requires a tray icon, otherwise the app becomes
/// difficult to restore once hidden.
fn normalize_window_preferences(
    close_behavior: CloseBehavior,
    show_tray_icon: bool,
) -> NormalizedWindowPreferences {
    NormalizedWindowPreferences {
        close_behavior,
        show_tray_icon: show_tray_icon || close_behavior == CloseBehavior::Hide,
    }
}

#[cfg(target_os = "macos")]
fn sync_activation_policy(
    _app: &AppHandle,
    _close_behavior: CloseBehavior,
    _tray_icon_enabled: bool,
) -> Result<(), String> {
    // Intentionally a no-op while Tauri macOS Dock visibility bug remains unresolved:
    // https://github.com/tauri-apps/tauri/issues/13519
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn sync_activation_policy(
    _app: &AppHandle,
    _close_behavior: CloseBehavior,
    _tray_icon_enabled: bool,
) -> Result<(), String> {
    Ok(())
}

/// Updates cloud backend base URL while keeping a fixed local SQLite path.
#[tauri::command]
pub async fn configure_backend(
    state: State<'_, DesktopState>,
    cloud_base_url: String,
) -> Result<(), String> {
    let normalized = normalize_cloud_base_url(&cloud_base_url)?;
    let previous_base_url = normalize_cloud_base_url(&state.cloud_base_url().await)?;
    if normalized == previous_base_url {
        return Ok(());
    }
    if state.require_access_token().await.is_ok()
        || state.refresh_token().await.is_some()
        || state.username().await.is_some()
    {
        return Err("sign out before changing the Kamori service URL".to_string());
    }
    clear_refresh_token_secure(&previous_base_url)?;
    clear_session_username_secure(&previous_base_url)?;
    state.set_backend(normalized).await;
    state.set_access_token(None).await;
    state.set_refresh_token(None).await;
    state.set_username(None).await;
    state.collections.write().await.clear();
    *state.device_identity.write().await = None;
    *state.dav_credentials.write().await = None;
    *state.pending_totp_login.lock().await = None;
    *state.pending_browser_login.lock().await = None;
    Ok(())
}

/// Applies desktop window behavior preferences (close action and tray icon visibility).
#[tauri::command]
pub async fn apply_window_preferences(
    app: AppHandle,
    state: State<'_, DesktopState>,
    close_behavior: String,
    show_tray_icon: bool,
) -> Result<(), String> {
    let parsed_behavior = CloseBehavior::from_value(close_behavior.trim())
        .ok_or_else(|| "invalid close behavior".to_string())?;
    let normalized = normalize_window_preferences(parsed_behavior, show_tray_icon);

    state.set_close_behavior(normalized.close_behavior);
    sync_tray_icon(&app, normalized.show_tray_icon)?;
    sync_activation_policy(&app, normalized.close_behavior, normalized.show_tray_icon)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_base_url_requires_https_except_loopback() {
        assert_eq!(
            normalize_cloud_base_url(" https://api.kamori.app/// ").unwrap(),
            "https://api.kamori.app"
        );
        assert_eq!(
            normalize_cloud_base_url("http://127.0.0.1:3000/").unwrap(),
            "http://127.0.0.1:3000"
        );
        assert!(normalize_cloud_base_url("http://api.kamori.app").is_err());
        assert!(normalize_cloud_base_url("https://user@api.kamori.app").is_err());
        assert!(normalize_cloud_base_url("https://api.kamori.app/api").is_err());
    }

    #[test]
    fn normalize_window_preferences_forces_tray_for_hide() {
        let normalized = normalize_window_preferences(CloseBehavior::Hide, false);
        assert_eq!(
            normalized,
            NormalizedWindowPreferences {
                close_behavior: CloseBehavior::Hide,
                show_tray_icon: true,
            }
        );
    }

    #[test]
    fn normalize_window_preferences_keeps_non_hide_values() {
        let minimized = normalize_window_preferences(CloseBehavior::Minimize, false);
        assert_eq!(
            minimized,
            NormalizedWindowPreferences {
                close_behavior: CloseBehavior::Minimize,
                show_tray_icon: false,
            }
        );

        let quit = normalize_window_preferences(CloseBehavior::Quit, true);
        assert_eq!(
            quit,
            NormalizedWindowPreferences {
                close_behavior: CloseBehavior::Quit,
                show_tray_icon: true,
            }
        );
    }
}
