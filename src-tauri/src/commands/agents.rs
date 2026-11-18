//! Access for AI agents over MCP: the switch, what agents may do per account, and the log. The server itself
//! runs as `elevant-mail mcp`, started by the agent (see elevant-mail-mcp).

use elevant_mail_core::agents::{self, AgentLogEntry, AgentPermissions, AgentSettings};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::error::CommandError;
use crate::state::AppState;

/// How an agent starts the server: this program with `mcp`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentLaunch {
    pub program: String,
    pub args: Vec<String>,
}

#[tauri::command]
pub async fn get_agent_settings(state: State<'_, AppState>) -> Result<AgentSettings, CommandError> {
    Ok(agents::settings(&state.db.conn())?)
}

#[tauri::command]
pub async fn set_agent_access(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), CommandError> {
    Ok(agents::set_enabled(&state.db.conn(), enabled)?)
}

/// What agents may do with one account's mail. Allowing files for the first time lists the
/// downloads folder, so there is a place to attach from; the user changes the list beside it.
#[tauri::command]
pub async fn set_agent_permissions(
    state: State<'_, AppState>,
    account_id: String,
    permissions: AgentPermissions,
) -> Result<(), CommandError> {
    let conn = state.db.conn();
    agents::set_permissions(&conn, &account_id, permissions)?;
    if permissions.attach
        && agents::folders(&conn)?.is_empty()
        && let Some(downloads) = dirs::download_dir()
    {
        agents::add_folder(&conn, &downloads)?;
    }
    Ok(())
}

/// Asks for a folder agents may attach files from (the system's dialog) and adds it. None when
/// the dialog was cancelled.
#[tauri::command]
pub async fn add_agent_folder(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<String>, CommandError> {
    let mut dialog = app.dialog().file();
    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&window);
    }
    // The dialog blocks until the user answers, so it runs off the async workers.
    let chosen = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_folder())
        .await
        .ok()
        .flatten()
        .and_then(|folder| folder.into_path().ok());
    let Some(folder) = chosen else {
        return Ok(None);
    };
    agents::add_folder(&state.db.conn(), &folder)?;
    Ok(Some(folder.to_string_lossy().into_owned()))
}

#[tauri::command]
pub async fn remove_agent_folder(
    state: State<'_, AppState>,
    path: String,
) -> Result<(), CommandError> {
    Ok(agents::remove_folder(&state.db.conn(), &path)?)
}

#[tauri::command]
pub async fn list_agent_log(
    state: State<'_, AppState>,
    limit: u32,
) -> Result<Vec<AgentLogEntry>, CommandError> {
    Ok(agents::recent_log(&state.db.conn(), limit)?)
}

#[tauri::command]
pub async fn clear_agent_log(state: State<'_, AppState>) -> Result<(), CommandError> {
    Ok(agents::clear_log(&state.db.conn())?)
}

#[tauri::command]
pub fn agent_launch() -> Result<AgentLaunch, CommandError> {
    let program = std::env::current_exe()
        .map_err(elevant_mail_core::Error::Io)?
        .to_string_lossy()
        .into_owned();
    Ok(AgentLaunch {
        program,
        args: vec!["mcp".into()],
    })
}
