use serde::Serialize;
use tauri::State;

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
