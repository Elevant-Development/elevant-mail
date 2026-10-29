use elevant_mail_core::accounts::{self, AccountChanges, AccountSettings, NewAccount};
use elevant_mail_core::mail::presets::{self, SuggestedSettings};
use elevant_mail_core::mail::settings::ImapSettings;
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
pub async fn get_account_settings(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<AccountSettings, CommandError> {
    Ok(accounts::settings(&state.db.conn(), &account_id)?)
}

/// Name and colour, which change without a login.
#[tauri::command]
pub async fn update_account(
    state: State<'_, AppState>,
    account_id: String,
    changes: AccountChanges,
) -> Result<Account, CommandError> {
    Ok(accounts::update(&state.db.conn(), &account_id, &changes)?)
}

/// New server settings or a new password: logs in with them first and keeps the old ones if that
/// fails. Without a password the stored one is used.
#[tauri::command]
pub async fn change_login(
    state: State<'_, AppState>,
    account_id: String,
    imap: ImapSettings,
    password: Option<String>,
) -> Result<(), CommandError> {
    let password = password.map(Secret::new);
    Ok(accounts::change_login(
        &state.db,
        state.secrets.as_ref(),
        &account_id,
        imap,
        password,
    )
    .await?)
}

/// A Microsoft account whose sign-in expired: Microsoft's sign-in in the browser again, for the
/// account's own address, then the new tokens replace the old ones.
#[tauri::command]
pub async fn sign_in_again(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<(), CommandError> {
    let login_hint = accounts::microsoft_login(&state.db.conn(), &account_id)?;
    let cancelled = state.start_sign_in();
    let tokens = oauth::sign_in(
        &oauth::Endpoints::microsoft(),
        &login_hint,
        |url| open::that_detached(url).map_err(Error::OpenExternal),
        async {
            let _ = cancelled.await;
        },
    )
    .await?;
    Ok(
        accounts::replace_oauth_tokens(&state.db, state.secrets.as_ref(), &account_id, tokens)
            .await?,
    )
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
