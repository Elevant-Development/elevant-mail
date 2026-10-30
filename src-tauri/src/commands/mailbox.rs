use elevant_mail_core::image_senders;
use elevant_mail_core::mail::sanitize::HtmlView;
use elevant_mail_core::mailbox::{self, ListOptions};
use elevant_mail_core::model::{
    Conversation, Folder, ListCursor, ListFilter, ListPage, Message, MessageSummary,
};
use tauri::State;

use crate::error::CommandError;
use crate::protocol::asset_base;
use crate::state::AppState;

#[tauri::command]
pub async fn list_folders(state: State<'_, AppState>) -> Result<Vec<Folder>, CommandError> {
    Ok(mailbox::list_folders(&state.db.conn())?)
}

/// A page of the list. `mailbox_id` is a folder id or "unified-inbox"; `threads`: grouped into
/// conversations; `filters` from the filter bar, with the rows to `keep` although they no longer
/// match; `after`: where the previous page ended.
#[tauri::command]
pub async fn list_messages(
    state: State<'_, AppState>,
    mailbox_id: String,
    threads: bool,
    filters: Vec<ListFilter>,
    keep: Vec<String>,
    after: Option<ListCursor>,
) -> Result<ListPage, CommandError> {
    let options = ListOptions {
        threads,
        filters: &filters,
        keep: &keep,
        after: after.as_ref(),
    };
    Ok(mailbox::list_messages(
        &state.db.conn(),
        &mailbox_id,
        options,
    )?)
}

/// Full-text search over all cached mail, newest first, with filters like `from:`. `also`:
/// messages the search on the servers found.
#[tauri::command]
pub async fn search_messages(
    state: State<'_, AppState>,
    query: String,
    threads: bool,
    also: Vec<String>,
) -> Result<Vec<Conversation>, CommandError> {
    Ok(mailbox::search_messages(
        &state.db.conn(),
        &query,
        threads,
        &also,
    )?)
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

/// `remote_images`: the user allowed the external images of this mail. They also show where
/// the user always allows them for the sender.
#[tauri::command]
pub async fn get_message(
    state: State<'_, AppState>,
    message_id: String,
    remote_images: bool,
) -> Result<Option<Message>, CommandError> {
    let conn = state.db.conn();
    let remote_images = remote_images || image_senders::allowed_for(&conn, &message_id)?;
    let view = HtmlView {
        asset_base: asset_base(),
        remote_images,
    };
    let Some(opened) = mailbox::get_message(&conn, &message_id, view)? else {
        return Ok(None);
    };
    drop(conn);
    if remote_images {
        state.allow_remote_images(&message_id, opened.remote_images);
    }
    Ok(Some(opened.message))
}

/// Senders whose mail always shows its external images.
#[tauri::command]
pub async fn list_image_senders(state: State<'_, AppState>) -> Result<Vec<String>, CommandError> {
    Ok(image_senders::list(&state.db.conn())?)
}

#[tauri::command]
pub async fn allow_images_from(
    state: State<'_, AppState>,
    address: String,
) -> Result<(), CommandError> {
    Ok(image_senders::allow(&state.db.conn(), &address)?)
}

#[tauri::command]
pub async fn forbid_images_from(
    state: State<'_, AppState>,
    address: String,
) -> Result<(), CommandError> {
    Ok(image_senders::forbid(&state.db.conn(), &address)?)
}
