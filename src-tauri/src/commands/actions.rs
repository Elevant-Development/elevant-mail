//! Read and flagged state. Setting it changes the cache at once; pushing sends the queue to the
//! server (the frontend does that right after, and every sync does it first).

use elevant_mail_core::actions::{FlagAction, set_flag};
use elevant_mail_core::mail::push;
use tauri::State;

use crate::error::CommandError;
use crate::state::AppState;

/// Returns the accounts with new changes for the server.
#[tauri::command]
pub async fn set_read(
    state: State<'_, AppState>,
    message_ids: Vec<String>,
    read: bool,
) -> Result<Vec<String>, CommandError> {
    Ok(set_flag(&state.db, &message_ids, FlagAction::Read, read)?)
}

/// Returns the accounts with new changes for the server.
#[tauri::command]
pub async fn set_flagged(
    state: State<'_, AppState>,
    message_ids: Vec<String>,
    flagged: bool,
) -> Result<Vec<String>, CommandError> {
    Ok(set_flag(
        &state.db,
        &message_ids,
        FlagAction::Flagged,
        flagged,
    )?)
}

/// Sends the account's queued changes; returns how many reached the server.
#[tauri::command]
pub async fn push_actions(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<u32, CommandError> {
    let lock = state.account_lock(&account_id);
    let _turn = lock.lock().await;
    Ok(push::push_actions(&state.db, state.secrets.as_ref(), &account_id).await?)
}
