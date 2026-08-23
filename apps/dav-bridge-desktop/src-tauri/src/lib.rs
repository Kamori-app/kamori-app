#![allow(clippy::result_large_err)]

mod commands;
mod models;
mod state;

use state::{CloseBehavior, DesktopState, FIXED_SQLITE_CACHE_PATH};
use tauri::Manager;

/// Runs the Tauri desktop application.
pub fn run() {
    let app_state = DesktopState::new("https://api.kamori.app", FIXED_SQLITE_CACHE_PATH);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }

            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<DesktopState>();
                match state.close_behavior() {
                    CloseBehavior::Quit => {}
                    CloseBehavior::Hide => {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    CloseBehavior::Minimize => {
                        api.prevent_close();
                        let _ = window.minimize();
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::window::configure_backend,
            commands::window::apply_window_preferences,
            commands::auth::password_login,
            commands::auth::restore_session,
            commands::auth::browser_login_start,
            commands::auth::browser_login_poll,
            commands::bridge::start_local_server,
            commands::bridge::stop_local_server,
            commands::bridge::local_server_status,
            commands::bridge::dav_connection_info,
            commands::bridge::rotate_dav_credentials,
            commands::bridge::sync_now,
            commands::collections::create_collection,
            commands::collections::list_collections,
            commands::session::dashboard_snapshot,
            commands::session::logout
        ])
        .run(tauri::generate_context!())
        .expect("failed to run dav-bridge-desktop");
}
