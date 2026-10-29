use elevant_mail_core::accounts::{self, NewAccount};
use elevant_mail_core::mail::presets::{self, SuggestedSettings};
use elevant_mail_core::model::Account;
use elevant_mail_core::secrets::Secret;
use tauri::State;

use crate::error::CommandError;
use crate::state::AppState;

/// Server settings for an email address: preset, MX record or guess.
#[tauri::command]
pub async fn detect_settings(email: String) -> Result<SuggestedSettings, CommandError> {
    Ok(presets::suggest_settings(&email).await?)
}

/// Logs in once to check the settings, then stores the account. The password goes straight
/// into the system keyring.
#[tauri::command]
pub async fn add_account(
    state: State<'_, AppState>,
    account: NewAccount,
    password: String,
) -> Result<Account, CommandError> {
    let password = Secret::new(password);
    Ok(accounts::add_account(&state.db, state.secrets.as_ref(), account, password).await?)
}

#[tauri::command]
pub async fn list_accounts(state: State<'_, AppState>) -> Result<Vec<Account>, CommandError> {
    Ok(accounts::list(&state.db.conn())?)
}

#[tauri::command]
pub async fn remove_account(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<(), CommandError> {
    Ok(accounts::remove(
        &state.db,
        state.secrets.as_ref(),
        &account_id,
    )?)
}
