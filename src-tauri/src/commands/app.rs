use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;

use crate::error::CommandError;
use crate::state::AppState;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    version: &'static str,
    schema_version: u32,
    keyring_available: bool,
}

/// Version and health of the backend, e.g. to warn when no keyring is available.
#[tauri::command]
pub fn app_status(state: State<'_, AppState>) -> Result<AppStatus, CommandError> {
    Ok(AppStatus {
        version: env!("CARGO_PKG_VERSION"),
        schema_version: state.db.schema_version()?,
        keyring_available: state.keyring_available,
    })
}

/// A newer published version, if there is one. One request to GitHub's API; the frontend asks
/// only while the setting allows it.
#[tauri::command]
pub async fn check_for_update() -> Result<Option<elevant_mail_core::updates::Update>, CommandError>
{
    Ok(elevant_mail_core::updates::check().await?)
}

/// Whether the app starts at login (into the tray, without its window).
#[tauri::command]
pub fn autostart_enabled(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<bool, CommandError> {
    let launcher = app.autolaunch();
    let result = if enabled {
        launcher.enable()
    } else {
        launcher.disable()
    };
    if let Err(error) = result {
        tracing::warn!(%error, enabled, "could not change starting at login");
    }
    Ok(launcher.is_enabled().unwrap_or(false))
}
