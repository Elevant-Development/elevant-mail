use elevant_mail_core::mail::sanitize::HtmlView;
use elevant_mail_core::mailbox;
use elevant_mail_core::model::{Conversation, Folder, Message, MessageSummary};
use tauri::State;

use crate::error::CommandError;
use crate::protocol::asset_base;
use crate::state::AppState;

#[tauri::command]
pub async fn list_folders(state: State<'_, AppState>) -> Result<Vec<Folder>, CommandError> {
    Ok(mailbox::list_folders(&state.db.conn())?)
}

/// `mailbox_id` is a folder id or "unified-inbox". `threads`: grouped into conversations.
#[tauri::command]
pub async fn list_messages(
    state: State<'_, AppState>,
    mailbox_id: String,
    threads: bool,
) -> Result<Vec<Conversation>, CommandError> {
    Ok(mailbox::list_messages(
        &state.db.conn(),
        &mailbox_id,
        threads,
    )?)
}

/// Full-text search over all cached mail, newest first.
#[tauri::command]
pub async fn search_messages(
    state: State<'_, AppState>,
    query: String,
    threads: bool,
) -> Result<Vec<Conversation>, CommandError> {
    Ok(mailbox::search_messages(&state.db.conn(), &query, threads)?)
}

/// The messages the reader shows for one, newest first: its conversation, or the message alone.
#[tauri::command]
pub async fn get_conversation(
    state: State<'_, AppState>,
    message_id: String,
    threads: bool,
) -> Result<Vec<MessageSummary>, CommandError> {
    Ok(mailbox::get_conversation(
        &state.db.conn(),
        &message_id,
        threads,
    )?)
}

/// `remote_images`: the user allowed the external images of this mail.
#[tauri::command]
pub async fn get_message(
    state: State<'_, AppState>,
    message_id: String,
    remote_images: bool,
) -> Result<Option<Message>, CommandError> {
    let view = HtmlView {
        asset_base: asset_base(),
        remote_images,
    };
    let Some(opened) = mailbox::get_message(&state.db.conn(), &message_id, view)? else {
        return Ok(None);
    };
    if remote_images {
        state.allow_remote_images(&message_id, opened.remote_images);
    }
    Ok(Some(opened.message))
}
