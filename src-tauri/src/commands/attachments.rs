//! Attachments: saved where the user picks, or opened with the system's program. Fetched from
//! the server on first use, then kept in the app's cache folder for the session.

use std::path::PathBuf;

use elevant_mail_core::Error;
use elevant_mail_core::attachments::AttachmentSource;
use elevant_mail_core::mail::download;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::error::CommandError;
use crate::state::AppState;

/// Asks where to save, then writes the file there; false when the dialog was cancelled.
#[tauri::command]
pub async fn save_attachment(
    app: AppHandle,
    state: State<'_, AppState>,
    attachment_id: String,
) -> Result<bool, CommandError> {
    let source = download::locate(&state.db, &attachment_id)?;
    let Some(target) = ask_where(&app, &source.attachment.file_name).await else {
        return Ok(false);
    };
    let cached = cached_file(&state, &source).await?;
    std::fs::copy(&cached, &target).map_err(Error::FileWrite)?;
    Ok(true)
}

/// Opens the attachment with the program the system has for it. Files that could run code are
/// refused here as well, not only in the UI.
#[tauri::command]
pub async fn open_attachment(
    state: State<'_, AppState>,
    attachment_id: String,
) -> Result<(), CommandError> {
    let source = download::locate(&state.db, &attachment_id)?;
    if !source.attachment.can_open {
        return Err(Error::NotOpenable.into());
    }
    let cached = cached_file(&state, &source).await?;
    open::that_detached(&cached).map_err(Error::OpenExternal)?;
    Ok(())
}

async fn ask_where(app: &AppHandle, file_name: &str) -> Option<PathBuf> {
    let mut dialog = app.dialog().file().set_file_name(file_name);
    if let Ok(downloads) = app.path().download_dir() {
        dialog = dialog.set_directory(downloads);
    }
    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&window);
    }
    // The dialog blocks until the user answers, so it runs off the async workers.
    let chosen = tauri::async_runtime::spawn_blocking(move || dialog.blocking_save_file())
        .await
        .ok()
        .flatten()?;
    chosen.into_path().ok()
}

/// The attachment as a file in the cache, fetched from the server the first time.
async fn cached_file(state: &AppState, source: &AttachmentSource) -> Result<PathBuf, CommandError> {
    if let Some(path) = state.cached_attachment(&source.attachment.id) {
        return Ok(path);
    }
    let lock = state.account_lock(&source.account_id);
    let _turn = lock.lock().await;
    let file = download::fetch_attachment(&state.db, state.secrets.as_ref(), &source.attachment.id)
        .await?;
    Ok(state.cache_attachment(&file.attachment, &file.data)?)
}
