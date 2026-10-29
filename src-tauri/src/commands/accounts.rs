use elevant_mail_core::accounts::{self, NewAccount};
use elevant_mail_core::mail::presets::{self, SuggestedSettings};
use elevant_mail_core::model::Account;
use elevant_mail_core::secrets::Secret;
use elevant_mail_core::{Error, oauth};
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

/// Adds a Microsoft 365 or Outlook.com account: opens Microsoft's sign-in in the system browser
/// and waits for it (until `cancel_sign_in`), then checks the login and stores the account.
#[tauri::command]
pub async fn add_microsoft_account(
    state: State<'_, AppState>,
    account: NewAccount,
) -> Result<Account, CommandError> {
    let cancelled = state.start_sign_in();
    let tokens = oauth::sign_in(
        &oauth::Endpoints::microsoft(),
        account.email_address.trim(),
        |url| open::that_detached(url).map_err(Error::OpenExternal),
        async {
            // A newer sign-in or `cancel_sign_in` ends this one.
            let _ = cancelled.await;
        },
    )
    .await?;
    Ok(accounts::add_oauth_account(&state.db, state.secrets.as_ref(), account, tokens).await?)
}

#[tauri::command]
pub async fn cancel_sign_in(state: State<'_, AppState>) -> Result<(), CommandError> {
    state.cancel_sign_in();
    Ok(())
}
