//! Moving accounts to another computer: a file written where the user picks, and read back from
//! one the user picks. The frontend never sees paths or the file's contents, only which accounts
//! it holds.

use elevant_mail_core::accounts::{self, SignInMethod};
use elevant_mail_core::model::Account;
use elevant_mail_core::secrets::Secret;
use elevant_mail_core::transfer::{self, Import, ImportPreview};
use elevant_mail_core::{Error, oauth};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::error::CommandError;
use crate::state::AppState;

/// Asks where to save, then writes all accounts there; with a passphrase their sign-ins too,
/// encrypted. False when the dialog was cancelled.
#[tauri::command]
pub async fn export_accounts(
    app: AppHandle,
    state: State<'_, AppState>,
    passphrase: Option<String>,
) -> Result<bool, CommandError> {
    let passphrase = passphrase.filter(|p| !p.is_empty()).map(Secret::new);
    let text = transfer::export(
        &state.db.conn(),
        state.secrets.as_ref(),
        passphrase.as_ref(),
    )?;
    let mut dialog = app
        .dialog()
        .file()
        .set_file_name("elevant-mail-konten.json");
    if let Ok(documents) = app.path().document_dir() {
        dialog = dialog.set_directory(documents);
    }
    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&window);
    }
    // The dialog blocks until the user answers, so it runs off the async workers.
    let chosen = tauri::async_runtime::spawn_blocking(move || dialog.blocking_save_file())
        .await
        .ok()
        .flatten();
    let Some(path) = chosen.and_then(|file| file.into_path().ok()) else {
        return Ok(false);
    };
    std::fs::write(&path, text).map_err(Error::FileWrite)?;
    Ok(true)
}

/// Asks for a file of exported accounts and keeps it for the import; none when cancelled.
#[tauri::command]
pub async fn open_account_file(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<ImportPreview>, CommandError> {
    let mut dialog = app.dialog().file().add_filter("JSON", &["json"]);
    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&window);
    }
    let chosen = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_file())
        .await
        .ok()
        .flatten();
    let Some(path) = chosen.and_then(|file| file.into_path().ok()) else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(&path).map_err(|_| Error::NotAnExport)?;
    let import = Import::read(&text)?;
    let preview = import.preview(&state.db.conn())?;
    state.set_pending_import(Some(import));
    Ok(Some(preview))
}

/// Opens the sign-ins in the file with the passphrase.
#[tauri::command]
pub async fn unlock_account_file(
    state: State<'_, AppState>,
    passphrase: String,
) -> Result<ImportPreview, CommandError> {
    let passphrase = Secret::new(passphrase);
    state.with_pending_import(|import| {
        import.unlock(&passphrase)?;
        import.preview(&state.db.conn())
    })
}

/// Adds one account from the file: with the sign-in that came along, the password typed in, or
/// Microsoft's sign-in in the browser. Then everything else from the file.
#[tauri::command]
pub async fn import_account(
    app: AppHandle,
    state: State<'_, AppState>,
    email: String,
    password: Option<String>,
) -> Result<Account, CommandError> {
    let (new, sign_in, came_along, refresh_token) = state.with_pending_import(|import| {
        Ok((
            import.new_account(&email)?,
            import.sign_in(&email)?,
            import.password(&email),
            import.refresh_token(&email),
        ))
    })?;
    let account = match sign_in {
        SignInMethod::Password => {
            let password = came_along
                .or_else(|| password.filter(|p| !p.is_empty()).map(Secret::new))
                .ok_or(Error::SecretNotFound)?;
            accounts::add_account(&state.db, state.secrets.as_ref(), new, password).await?
        }
        SignInMethod::Microsoft => {
            let endpoints = oauth::Endpoints::microsoft();
            // The sign-in that came along, while Microsoft still accepts it; otherwise the browser.
            let refreshed = match refresh_token {
                Some(token) => match oauth::refresh(&endpoints, &token).await {
                    Ok(mut tokens) => {
                        tokens.refresh_token.get_or_insert(token);
                        Some(tokens)
                    }
                    Err(error) => {
                        tracing::warn!(%error, "imported Microsoft sign-in refused, signing in again");
                        None
                    }
                },
                None => None,
            };
            let tokens = match refreshed {
                Some(tokens) => tokens,
                None => {
                    let cancelled = state.start_sign_in();
                    oauth::sign_in(
                        &endpoints,
                        new.email_address.trim(),
                        |url| open::that_detached(url).map_err(Error::OpenExternal),
                        async {
                            let _ = cancelled.await;
                        },
                    )
                    .await?
                }
            };
            accounts::add_oauth_account(&state.db, state.secrets.as_ref(), new, tokens).await?
        }
    };
    state
        .with_pending_import(|import| import.apply(&state.db, state.secrets.as_ref(), &account))?;
    let account = accounts::list(&state.db.conn())?
        .into_iter()
        .find(|a| a.id == account.id)
        .unwrap_or(account);
    state.watchers.start(&app, &account.id);
    Ok(account)
}

/// Done with the file: its sign-ins leave memory.
#[tauri::command]
pub async fn close_account_file(state: State<'_, AppState>) -> Result<(), CommandError> {
    state.set_pending_import(None);
    Ok(())
}
