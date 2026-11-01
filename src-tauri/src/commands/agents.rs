//! Access for AI agents over MCP: the switch, what agents may do per account, and the log. The server itself
//! runs as `elevant-mail mcp`, started by the agent (see elevant-mail-mcp).

use elevant_mail_core::agents::{self, AgentLogEntry, AgentPermissions, AgentSettings};
use serde::Serialize;
use tauri::State;

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

/// What agents may do with one account's mail.
#[tauri::command]
pub async fn set_agent_permissions(
    state: State<'_, AppState>,
    account_id: String,
    permissions: AgentPermissions,
) -> Result<(), CommandError> {
    Ok(agents::set_permissions(
        &state.db.conn(),
        &account_id,
        permissions,
    )?)
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
