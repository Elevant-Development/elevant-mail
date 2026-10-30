//! Writing mail: sending, and the addresses offered while typing a recipient.

use elevant_mail_core::mail::send::{self, OutgoingMail, SentMail};
use elevant_mail_core::mail::sync;
use elevant_mail_core::model::{Address, FolderRole};
use elevant_mail_core::{mailbox, moves};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::CommandError;
use crate::state::AppState;

/// Sent once the mail is in Sent and Sent is synced, with the account's id.
const MAIL_SENT: &str = "mail-sent";

/// Hands the mail to the account's SMTP server and returns once the server took it. Filing it
/// in Sent, which waits a few seconds for the server to do that itself, follows in the
/// background (`mail-sent`).
#[tauri::command]
pub async fn send_message(
    app: AppHandle,
    state: State<'_, AppState>,
    mail: OutgoingMail,
) -> Result<(), CommandError> {
    let sent = send::send(&state.db, state.secrets.as_ref(), &mail).await?;
    tauri::async_runtime::spawn(file_in_sent(app, sent));
    Ok(())
}

async fn file_in_sent(app: AppHandle, sent: SentMail) {
    let state = app.state::<AppState>();
    let lock = state.account_lock(&sent.account_id);
    let _turn = lock.lock().await;
    if let Err(error) = send::file_in_sent(&state.db, state.secrets.as_ref(), &sent).await {
        tracing::warn!(%error, "could not file the sent mail in Sent");
    }
    // The reply joins its conversation once Sent is synced. Quietly: no progress in the list.
    let sent_folder = moves::role_folder(&state.db.conn(), &sent.account_id, FolderRole::Sent);
    if let Ok(Some(folder_id)) = sent_folder {
        let quiet = |_| {};
        if let Err(error) =
            sync::sync_folder(&state.db, state.secrets.as_ref(), &folder_id, &quiet).await
        {
            tracing::warn!(%error, "could not sync Sent after sending");
        }
    }
    if let Err(error) = app.emit(MAIL_SENT, &sent.account_id) {
        tracing::warn!(%error, "could not report the sent mail");
    }
}

/// Up to eight addresses matching what was typed, the ones written with most first.
#[tauri::command]
pub async fn suggest_addresses(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<Address>, CommandError> {
    Ok(mailbox::suggest_addresses(&state.db.conn(), &query, 8)?)
}
