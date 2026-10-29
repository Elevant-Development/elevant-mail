use elevant_mail_core::mailbox;
use elevant_mail_core::model::{Folder, Message, MessageSummary};
use tauri::State;

use crate::error::CommandError;
use crate::state::AppState;

#[tauri::command]
pub async fn list_folders(state: State<'_, AppState>) -> Result<Vec<Folder>, CommandError> {
    Ok(mailbox::list_folders(&state.db.conn())?)
}

/// `mailbox_id` is a folder id or "unified-inbox".
#[tauri::command]
pub async fn list_messages(
    state: State<'_, AppState>,
    mailbox_id: String,
) -> Result<Vec<MessageSummary>, CommandError> {
    Ok(mailbox::list_messages(&state.db.conn(), &mailbox_id)?)
}

#[tauri::command]
pub async fn get_message(
    state: State<'_, AppState>,
    message_id: String,
) -> Result<Option<Message>, CommandError> {
    Ok(mailbox::get_message(&state.db.conn(), &message_id)?)
}
