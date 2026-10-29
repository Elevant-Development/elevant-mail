//! Read and flagged state, archiving, deleting and moving. Each changes the cache at once;
//! pushing sends the queue to the server (the frontend does that right after, or once a move can
//! no longer be undone, and every sync does it first).

use elevant_mail_core::actions::{FlagAction, set_flag};
use elevant_mail_core::mail::push;
use elevant_mail_core::moves::{self, MoveOutcome};
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

/// Into a folder of the same account.
#[tauri::command]
pub async fn move_messages(
    state: State<'_, AppState>,
    message_ids: Vec<String>,
    folder_id: String,
) -> Result<MoveOutcome, CommandError> {
    Ok(moves::move_messages(&state.db, &message_ids, &folder_id)?)
}

/// Into the archive; `folder_name` names the folder made for an account that has none.
#[tauri::command]
pub async fn archive_messages(
    state: State<'_, AppState>,
    message_ids: Vec<String>,
    folder_name: String,
) -> Result<MoveOutcome, CommandError> {
    Ok(moves::archive_messages(
        &state.db,
        state.secrets.as_ref(),
        &message_ids,
        &folder_name,
    )
    .await?)
}

/// Into the trash; what is in the trash already is deleted for good.
#[tauri::command]
pub async fn delete_messages(
    state: State<'_, AppState>,
    message_ids: Vec<String>,
) -> Result<MoveOutcome, CommandError> {
    Ok(moves::delete_messages(&state.db, &message_ids)?)
}

/// Takes back moves that have not reached the server; returns the messages that are back.
#[tauri::command]
pub async fn undo_moves(
    state: State<'_, AppState>,
    message_ids: Vec<String>,
) -> Result<Vec<String>, CommandError> {
    Ok(moves::undo_moves(&state.db, &message_ids)?)
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
