//! Writing mail: sending, drafts, and the addresses offered while typing a recipient. Work on
//! the server (filing in Sent, the drafts folder) runs in the background and ends with the
//! `cache-changed` event.

use elevant_mail_core::Error;
use elevant_mail_core::mail::drafts::{self, Draft, DraftAttachment, ServerCopy};
use elevant_mail_core::mail::send::{self, OutgoingMail, SentMail};
use elevant_mail_core::mail::sync;
use elevant_mail_core::model::{Address, FolderRole};
use elevant_mail_core::{mailbox, moves};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::error::CommandError;
use crate::state::AppState;

/// The cache changed in the background (a sent mail filed, a draft uploaded or removed), with
/// the account's id.
const CACHE_CHANGED: &str = "cache-changed";

/// Hands the mail to the account's SMTP server and returns once the server took it. The draft it
/// was written in goes; filing in Sent, which waits a few seconds for the server to do that
/// itself, and removing the draft's server copy follow in the background.
#[tauri::command]
pub async fn send_message(
    app: AppHandle,
    state: State<'_, AppState>,
    mail: OutgoingMail,
    draft_key: Option<String>,
) -> Result<(), CommandError> {
    let sent = send::send(
        &state.db,
        state.secrets.as_ref(),
        &mail,
        draft_key.as_deref(),
    )
    .await?;
    let copy = match draft_key {
        Some(key) => drafts::take(&state.db.conn(), &key)?,
        None => None,
    };
    tauri::async_runtime::spawn(after_sending(app, sent, copy));
    Ok(())
}

async fn after_sending(app: AppHandle, sent: SentMail, copy: Option<ServerCopy>) {
    let state = app.state::<AppState>();
    {
        let lock = state.account_lock(&sent.account_id);
        let _turn = lock.lock().await;
        if let Err(error) = send::file_in_sent(&state.db, state.secrets.as_ref(), &sent).await {
            tracing::warn!(%error, "could not file the sent mail in Sent");
        }
        // The reply joins its conversation once Sent is synced.
        quiet_sync(&state, &sent.account_id, FolderRole::Sent).await;
    }
    if let Some(copy) = copy {
        remove_server_copy(&app, &copy).await;
    }
    changed(&app, &sent.account_id);
}

/// Keeps the draft as it is now, on this device.
#[tauri::command]
pub async fn save_draft(state: State<'_, AppState>, draft: Draft) -> Result<(), CommandError> {
    Ok(drafts::save(&state.db.conn(), &draft)?)
}

/// The drafts kept on this device, the latest change first.
#[tauri::command]
pub async fn list_drafts(state: State<'_, AppState>) -> Result<Vec<Draft>, CommandError> {
    Ok(drafts::list(&state.db.conn())?)
}

/// Puts the draft into its account's drafts folder, in the background; nothing happens when it
/// has not changed since.
#[tauri::command]
pub async fn upload_draft(app: AppHandle, key: String) -> Result<(), CommandError> {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let account_id: Option<String> = drafts::list(&state.db.conn())
            .ok()
            .and_then(|all| all.into_iter().find(|d| d.key == key))
            .map(|d| d.account_id);
        let Some(account_id) = account_id else { return };
        let lock = state.account_lock(&account_id);
        let _turn = lock.lock().await;
        match drafts::upload(&state.db, state.secrets.as_ref(), &key).await {
            Ok(true) => {
                quiet_sync(&state, &account_id, FolderRole::Drafts).await;
                changed(&app, &account_id);
            }
            Ok(false) => {}
            Err(error) => tracing::warn!(%error, "could not save the draft on the server"),
        }
    });
    Ok(())
}

/// Throws the draft away, here and on the server.
#[tauri::command]
pub async fn discard_draft(
    app: AppHandle,
    state: State<'_, AppState>,
    key: String,
) -> Result<(), CommandError> {
    if let Some(copy) = drafts::take(&state.db.conn(), &key)? {
        changed(&app, &copy.account_id);
        tauri::async_runtime::spawn(async move {
            remove_server_copy(&app, &copy).await;
            changed(&app, &copy.account_id);
        });
    }
    Ok(())
}

/// A draft from the drafts folder, to write on.
#[tauri::command]
pub async fn open_draft(
    state: State<'_, AppState>,
    message_id: String,
) -> Result<Draft, CommandError> {
    Ok(drafts::open_cached(&state.db.conn(), &message_id)?)
}

/// Asks for files to attach (the system's dialog, opened here, not by the frontend) and attaches
/// them to the draft, which has to be saved already. Empty when the dialog was cancelled.
#[tauri::command]
pub async fn pick_attachments(
    app: AppHandle,
    state: State<'_, AppState>,
    draft_key: String,
) -> Result<Vec<DraftAttachment>, CommandError> {
    let mut dialog = app.dialog().file();
    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&window);
    }
    // The dialog blocks until the user answers, so it runs off the async workers.
    let chosen = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_files())
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    let paths = chosen
        .into_iter()
        .filter_map(|file| file.into_path().ok())
        .collect();
    attach_paths(&state, &draft_key, paths)
}

/// Attaches the files just dropped onto the window.
#[tauri::command]
pub async fn attach_dropped(
    state: State<'_, AppState>,
    draft_key: String,
) -> Result<Vec<DraftAttachment>, CommandError> {
    let paths = state.take_dropped_files();
    attach_paths(&state, &draft_key, paths)
}

fn attach_paths(
    state: &AppState,
    draft_key: &str,
    paths: Vec<std::path::PathBuf>,
) -> Result<Vec<DraftAttachment>, CommandError> {
    let mut attached = Vec::new();
    for path in paths.into_iter().filter(|path| path.is_file()) {
        let data = std::fs::read(&path).map_err(Error::Io)?;
        let name = path.to_string_lossy();
        attached.push(drafts::attach(&state.db.conn(), draft_key, &name, data)?);
    }
    Ok(attached)
}

#[tauri::command]
pub async fn detach_attachment(
    state: State<'_, AppState>,
    attachment_id: String,
) -> Result<(), CommandError> {
    Ok(drafts::detach(&state.db.conn(), &attachment_id)?)
}

#[tauri::command]
pub async fn list_attachments(
    state: State<'_, AppState>,
    draft_key: String,
) -> Result<Vec<DraftAttachment>, CommandError> {
    Ok(drafts::attachments(&state.db.conn(), &draft_key)?)
}

/// Forwarding: the files of the forwarded mail go into the draft, fetched from the server.
#[tauri::command]
pub async fn forward_attachments(
    state: State<'_, AppState>,
    draft_key: String,
    message_id: String,
) -> Result<Vec<DraftAttachment>, CommandError> {
    let account_id =
        mailbox::message_account(&state.db.conn(), &message_id)?.ok_or(Error::MessageNotFound)?;
    let lock = state.account_lock(&account_id);
    let _turn = lock.lock().await;
    Ok(
        drafts::attach_from_message(&state.db, state.secrets.as_ref(), &draft_key, &message_id)
            .await?,
    )
}

/// Up to eight addresses matching what was typed, the ones written with most first.
#[tauri::command]
pub async fn suggest_addresses(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<Address>, CommandError> {
    Ok(mailbox::suggest_addresses(&state.db.conn(), &query, 8)?)
}

async fn remove_server_copy(app: &AppHandle, copy: &ServerCopy) {
    let state = app.state::<AppState>();
    let lock = state.account_lock(&copy.account_id);
    let _turn = lock.lock().await;
    if let Err(error) = drafts::remove_copy(&state.db, state.secrets.as_ref(), copy).await {
        tracing::warn!(%error, "could not remove the draft from the server");
    }
    quiet_sync(&state, &copy.account_id, FolderRole::Drafts).await;
}

/// Syncs one of the account's folders without progress in the list.
async fn quiet_sync(state: &AppState, account_id: &str, role: FolderRole) {
    let folder = moves::role_folder(&state.db.conn(), account_id, role);
    if let Ok(Some(folder_id)) = folder {
        let quiet = |_| {};
        if let Err(error) =
            sync::sync_folder(&state.db, state.secrets.as_ref(), &folder_id, &quiet).await
        {
            tracing::warn!(%error, ?role, "could not sync a folder in the background");
        }
    }
}

fn changed(app: &AppHandle, account_id: &str) {
    if let Err(error) = app.emit(CACHE_CHANGED, account_id) {
        tracing::warn!(%error, "could not report a change of the cache");
    }
}
