use elevant_mail_core::accounts::{self, AccountChanges, AccountSettings, NewAccount};
use elevant_mail_core::mail::presets::{self, SuggestedSettings};
use elevant_mail_core::mail::settings::{ImapSettings, SmtpSettings};
use elevant_mail_core::model::Account;
use elevant_mail_core::secrets::Secret;
use elevant_mail_core::{Error, oauth};
use tauri::{AppHandle, State};

use crate::error::CommandError;
use crate::state::AppState;

/// Server settings for an email address: preset, MX record or guess.
#[tauri::command]
pub async fn detect_settings(email: String) -> Result<SuggestedSettings, CommandError> {
    Ok(presets::suggest_settings(&email).await?)
}

/// Logs in once to check the settings, then stores the account. The password goes straight
/// into the system keyring. From then on the app waits for the account's new mail.
#[tauri::command]
pub async fn add_account(
    app: AppHandle,
    state: State<'_, AppState>,
    account: NewAccount,
    password: String,
) -> Result<Account, CommandError> {
    let password = Secret::new(password);
    let account =
        accounts::add_account(&state.db, state.secrets.as_ref(), account, password).await?;
    state.watchers.start(&app, &account.id);
    Ok(account)
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

/// How many days back the account keeps mail; `None` keeps everything. The next sync fetches
/// what a longer period adds.
#[tauri::command]
pub async fn set_keep_days(
    state: State<'_, AppState>,
    account_id: String,
    days: Option<u32>,
) -> Result<(), CommandError> {
    Ok(accounts::set_keep_days(
        &state.db.conn(),
        &account_id,
        days,
    )?)
}

/// The name mail goes out under; empty takes the one from Sent again.
#[tauri::command]
pub async fn set_sender_name(
    state: State<'_, AppState>,
    account_id: String,
    name: String,
) -> Result<(), CommandError> {
    Ok(accounts::set_sender_name(
        &state.db.conn(),
        &account_id,
        &name,
    )?)
}

/// New outgoing server settings, and a password where it differs from the IMAP one: signs in
/// with them first and keeps the old ones if that fails.
#[tauri::command]
pub async fn change_smtp(
    state: State<'_, AppState>,
    account_id: String,
    smtp: SmtpSettings,
    password: Option<String>,
) -> Result<(), CommandError> {
    Ok(accounts::change_smtp(
        &state.db,
        state.secrets.as_ref(),
        &account_id,
        smtp,
        password.map(Secret::new),
    )
    .await?)
}

/// Takes the account's inbox into the unified inbox or out of it.
#[tauri::command]
pub async fn set_in_unified_inbox(
    state: State<'_, AppState>,
    account_id: String,
    included: bool,
) -> Result<(), CommandError> {
    Ok(accounts::set_in_unified_inbox(
        &state.db.conn(),
        &account_id,
        included,
    )?)
}

/// New server settings or a new password: logs in with them first and keeps the old ones if that
/// fails. Without a password the stored one is used.
#[tauri::command]
pub async fn change_login(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: String,
    imap: ImapSettings,
    password: Option<String>,
) -> Result<(), CommandError> {
    let password = password.map(Secret::new);
    accounts::change_login(
        &state.db,
        state.secrets.as_ref(),
        &account_id,
        imap,
        password,
    )
    .await?;
    // The waiting connection still uses the old login.
    state.watchers.start(&app, &account_id);
    Ok(())
}

/// A Microsoft account whose sign-in expired: Microsoft's sign-in in the browser again, for the
/// account's own address, then the new tokens replace the old ones.
#[tauri::command]
pub async fn sign_in_again(
    app: AppHandle,
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
    accounts::replace_oauth_tokens(&state.db, state.secrets.as_ref(), &account_id, tokens).await?;
    state.watchers.start(&app, &account_id);
    Ok(())
}

#[tauri::command]
pub async fn remove_account(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<(), CommandError> {
    state.watchers.stop(&account_id);
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
    app: AppHandle,
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
    let account =
        accounts::add_oauth_account(&state.db, state.secrets.as_ref(), account, tokens).await?;
    state.watchers.start(&app, &account.id);
    Ok(account)
}

#[tauri::command]
pub async fn cancel_sign_in(state: State<'_, AppState>) -> Result<(), CommandError> {
    state.cancel_sign_in();
    Ok(())
}
