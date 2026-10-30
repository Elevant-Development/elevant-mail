//! Desktop shell of Elevant Mail: window, startup and the commands the frontend calls. Mail,
//! storage and credentials live in `elevant-mail-core` (see docs/project-brief.md).

mod commands;
mod error;
mod protocol;
mod state;
mod watch;
mod window;

use tauri::Manager;
use tracing_subscriber::EnvFilter;

pub fn run() {
    init_logging();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let state = state::AppState::init(app.handle())?;
            app.manage(state);
            window::create_main(app.handle())?;
            let state = app.state::<state::AppState>();
            for account in elevant_mail_core::accounts::list(&state.db.conn())? {
                state.watchers.start(app.handle(), &account.id);
            }
            Ok(())
        })
        // Files dropped onto the window: kept for `attach_dropped`, never named by the frontend.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) = event {
                window
                    .state::<state::AppState>()
                    .set_dropped_files(paths.clone());
            }
        })
        .register_asynchronous_uri_scheme_protocol(protocol::SCHEME, protocol::handle)
        .invoke_handler(tauri::generate_handler![
            commands::app::app_status,
            commands::app::check_for_update,
            commands::accounts::detect_settings,
            commands::accounts::add_account,
            commands::accounts::add_microsoft_account,
            commands::accounts::cancel_sign_in,
            commands::accounts::list_accounts,
            commands::accounts::remove_account,
            commands::accounts::get_account_settings,
            commands::accounts::update_account,
            commands::accounts::set_keep_days,
            commands::accounts::set_in_unified_inbox,
            commands::accounts::set_sender_name,
            commands::accounts::set_signature,
            commands::accounts::change_smtp,
            commands::accounts::change_login,
            commands::accounts::sign_in_again,
            commands::transfer::export_accounts,
            commands::transfer::open_account_file,
            commands::transfer::unlock_account_file,
            commands::transfer::import_account,
            commands::transfer::close_account_file,
            commands::mailbox::list_folders,
            commands::mailbox::list_messages,
            commands::mailbox::get_message,
            commands::mailbox::get_conversation,
            commands::mailbox::search_messages,
            commands::mailbox::list_image_senders,
            commands::mailbox::allow_images_from,
            commands::mailbox::forbid_images_from,
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
            commands::sync::search_server,
            commands::sync::load_message_body,
            commands::links::open_link,
            commands::attachments::save_attachment,
            commands::attachments::open_attachment,
            commands::compose::send_message,
            commands::compose::pick_attachments,
            commands::compose::attach_dropped,
            commands::compose::detach_attachment,
            commands::compose::list_attachments,
            commands::compose::forward_attachments,
            commands::compose::save_draft,
            commands::compose::list_drafts,
            commands::compose::upload_draft,
            commands::compose::discard_draft,
            commands::compose::open_draft,
            commands::compose::suggest_addresses,
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
