//! Fetching mail. Progress goes to the frontend as `sync-progress` events.

use elevant_mail_core::mail::sync::{self, SyncProgress, SyncReport};
use elevant_mail_core::{Error, mailbox};
use tauri::{AppHandle, Emitter, State};

use crate::error::CommandError;
use crate::state::AppState;

const PROGRESS_EVENT: &str = "sync-progress";

fn emitter(app: &AppHandle) -> impl Fn(SyncProgress) + Send + Sync + '_ {
    move |progress| {
        if let Err(error) = app.emit(PROGRESS_EVENT, progress) {
            tracing::warn!(%error, "could not send sync progress");
        }
    }
}

fn account_of(state: &AppState, folder_id: &str) -> Result<String, CommandError> {
    Ok(mailbox::folder_account(&state.db.conn(), folder_id)?.ok_or(Error::FolderNotFound)?)
}

/// New, changed and deleted mail in the account's inbox.
#[tauri::command]
pub async fn sync_account(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
) -> Result<SyncReport, CommandError> {
    let lock = state.account_lock(&account_id);
    let _turn = lock.lock().await;
    let emit = emitter(&app);
    Ok(sync::sync_account(&state.db, state.secrets.as_ref(), &account_id, &emit).await?)
}

#[tauri::command]
pub async fn sync_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    folder_id: String,
) -> Result<SyncReport, CommandError> {
    let lock = state.account_lock(&account_of(&state, &folder_id)?);
    let _turn = lock.lock().await;
    let emit = emitter(&app);
    Ok(sync::sync_folder(&state.db, state.secrets.as_ref(), &folder_id, &emit).await?)
}

/// The next 100 messages older than the oldest cached one.
#[tauri::command]
pub async fn load_older(
    app: AppHandle,
    state: State<'_, AppState>,
    folder_id: String,
) -> Result<SyncReport, CommandError> {
    let lock = state.account_lock(&account_of(&state, &folder_id)?);
    let _turn = lock.lock().await;
    let emit = emitter(&app);
    Ok(sync::load_older(&state.db, state.secrets.as_ref(), &folder_id, &emit).await?)
}
