//! Desktop shell of Elevant Mail: window, startup and the commands the frontend calls. Mail,
//! storage and credentials live in `elevant-mail-core` (see docs/project-brief.md).

mod commands;
mod error;
mod protocol;
mod state;
mod window;

use tauri::Manager;
use tracing_subscriber::EnvFilter;

pub fn run() {
    init_logging();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let state = state::AppState::init(app.handle())?;
            app.manage(state);
            window::create_main(app.handle())?;
            Ok(())
        })
        .register_asynchronous_uri_scheme_protocol(protocol::SCHEME, protocol::handle)
        .invoke_handler(tauri::generate_handler![
            commands::app::app_status,
            commands::accounts::detect_settings,
            commands::accounts::add_account,
            commands::accounts::add_microsoft_account,
            commands::accounts::cancel_sign_in,
            commands::accounts::list_accounts,
            commands::accounts::remove_account,
            commands::accounts::get_account_settings,
            commands::accounts::update_account,
            commands::accounts::change_login,
            commands::accounts::sign_in_again,
            commands::mailbox::list_folders,
            commands::mailbox::list_messages,
            commands::mailbox::get_message,
            commands::mailbox::search_messages,
            commands::actions::set_read,
            commands::actions::set_flagged,
            commands::actions::push_actions,
            commands::actions::move_messages,
            commands::actions::archive_messages,
            commands::actions::delete_messages,
            commands::actions::undo_moves,
            commands::sync::sync_account,
            commands::sync::sync_folder,
            commands::sync::load_older,
            commands::sync::load_message_body,
            commands::links::open_link,
            commands::attachments::save_attachment,
            commands::attachments::open_attachment,
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
