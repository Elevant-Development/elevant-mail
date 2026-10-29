//! Desktop shell of Elevant Mail: window, startup and the commands the frontend calls. Mail,
//! storage and credentials live in `elevant-mail-core` (see docs/projekt-brief.md).

mod commands;
mod error;
mod state;

use tauri::Manager;
use tracing_subscriber::EnvFilter;

pub fn run() {
    init_logging();

    tauri::Builder::default()
        .setup(|app| {
            let state = state::AppState::init(app.handle())?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app::app_status,
            commands::accounts::detect_settings,
            commands::accounts::add_account,
            commands::accounts::list_accounts,
            commands::accounts::remove_account,
            commands::mailbox::list_folders,
            commands::mailbox::list_messages,
            commands::mailbox::get_message,
            commands::sync::sync_account,
            commands::sync::sync_folder,
            commands::sync::load_older,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Elevant Mail");
}

/// Logs to stderr. The level comes from `ELEVANT_MAIL_LOG` (e.g. `debug`), default `info`.
/// Credentials never reach a log line: see `elevant_mail_core::redact`.
fn init_logging() {
    let filter =
        EnvFilter::try_from_env("ELEVANT_MAIL_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}
