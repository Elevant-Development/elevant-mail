//! Contacts: address books and their contacts from the cache, and syncing them over CardDAV.

use elevant_mail_core::contacts::sync::{self, ContactsSyncReport};
use elevant_mail_core::contacts::{
    self, AddressBook, ContactDetails, ContactInput, ContactSummary,
};
use tauri::State;

use crate::error::CommandError;
use crate::state::AppState;

#[tauri::command]
pub async fn list_address_books(
    state: State<'_, AppState>,
) -> Result<Vec<AddressBook>, CommandError> {
    Ok(contacts::list_address_books(&state.db.conn())?)
}

/// The contacts of one address book, or of all without one, by name; `query` narrows them.
#[tauri::command]
pub async fn list_contacts(
    state: State<'_, AppState>,
    address_book_id: Option<String>,
    query: String,
) -> Result<Vec<ContactSummary>, CommandError> {
    Ok(contacts::list_contacts(
        &state.db.conn(),
        address_book_id.as_deref(),
        &query,
    )?)
}

#[tauri::command]
pub async fn get_contact(
    state: State<'_, AppState>,
    contact_id: String,
) -> Result<ContactDetails, CommandError> {
    Ok(contacts::get_contact(&state.db.conn(), &contact_id)?)
}

/// A new contact or the changes to one; returns its id.
#[tauri::command]
pub async fn save_contact(
    state: State<'_, AppState>,
    contact: ContactInput,
) -> Result<String, CommandError> {
    Ok(contacts::save_contact(&state.db.conn(), &contact)?)
}

#[tauri::command]
pub async fn delete_contact(
    state: State<'_, AppState>,
    contact_id: String,
) -> Result<(), CommandError> {
    Ok(contacts::delete_contact(&state.db.conn(), &contact_id)?)
}

/// Sends the contact changes made here and takes the server's; one at a time per account.
#[tauri::command]
pub async fn sync_contacts(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<ContactsSyncReport, CommandError> {
    let lock = state.account_lock(&format!("contacts:{account_id}"));
    let _turn = lock.lock().await;
    sync::sync_account(&state.db, state.secrets.as_ref(), &account_id)
        .await
        .map_err(|error| {
            tracing::warn!(account = %account_id, %error, "contacts sync failed");
            error.into()
        })
}
