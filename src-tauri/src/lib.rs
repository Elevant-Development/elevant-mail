//! Desktop shell of Elevant Mail: window, startup and the commands the frontend calls. Mail,
//! storage and credentials live in `elevant-mail-core` (see docs/project-brief.md).

mod agent_changes;
mod commands;
mod error;
mod protocol;
mod state;
mod tray;
mod watch;
mod window;

use tauri::Manager;
use tracing_subscriber::EnvFilter;

pub fn run() {
    init_logging();

    tauri::Builder::default()
        // First, so a second start only brings the running app's window back.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![tray::HIDDEN_ARG]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let state = state::AppState::init(app.handle())?;
            app.manage(state);
            let hidden = std::env::args().any(|arg| arg == tray::HIDDEN_ARG);
            window::create_main(app.handle(), !hidden)?;
            tray::init(app.handle());
            agent_changes::watch(app.handle());
            let state = app.state::<state::AppState>();
            for account in elevant_mail_core::accounts::list(&state.db.conn())? {
                state.watchers.start(app.handle(), &account.id);
            }
            Ok(())
        })
        // Files dropped onto the window: kept for `attach_dropped`, never named by the frontend.
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) => {
                window
                    .state::<state::AppState>()
                    .set_dropped_files(paths.clone());
            }
            // Closing keeps the app running in the tray, if there is one and the user wants it:
            // the window is hidden then, and the close is called off.
            tauri::WindowEvent::CloseRequested { api, .. }
                if tray::hide_instead_of_close(window) =>
            {
                api.prevent_close();
            }
            _ => {}
        })
        .register_asynchronous_uri_scheme_protocol(protocol::SCHEME, protocol::handle)
        .invoke_handler(tauri::generate_handler![
            commands::app::app_status,
            commands::app::check_for_update,
            commands::agents::get_agent_settings,
            commands::agents::set_agent_access,
            commands::agents::set_agent_permissions,
            commands::agents::list_agent_log,
            commands::agents::clear_agent_log,
            commands::agents::agent_launch,
            commands::app::autostart_enabled,
            commands::app::set_autostart,
            tray::configure_background,
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
            commands::contacts::list_address_books,
            commands::contacts::list_contacts,
            commands::contacts::get_contact,
            commands::contacts::save_contact,
            commands::contacts::delete_contact,
            commands::contacts::sync_contacts,
            commands::calendar::list_calendars,
            commands::calendar::set_calendar_visible,
            commands::calendar::set_calendar_color,
            commands::calendar::list_occurrences,
            commands::calendar::get_event,
            commands::calendar::due_reminders,
            commands::calendar::save_event,
            commands::calendar::delete_event,
            commands::calendar::sync_calendars,
            commands::calendar::caldav_status,
            commands::calendar::set_caldav,
            commands::calendar::answer_invitation,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Elevant Mail");
}

/// Serves AI agents over MCP until stdin ends; the exit code. Logs go to stderr, stdout carries
/// the protocol. The database is the app's own, found where Tauri keeps it.
pub fn run_mcp() -> i32 {
    init_logging();
    let Some(path) = dirs::data_dir().map(|dir| dir.join(IDENTIFIER).join(state::DATABASE_FILE))
    else {
        tracing::error!("no data folder for the mail cache");
        return 1;
    };
    let db = match elevant_mail_core::Database::open(&path) {
        Ok(db) => db,
        Err(error) => {
            tracing::error!(%error, path = %path.display(), "could not open the mail cache");
            return 1;
        }
    };
    let stdin = std::io::stdin().lock();
    match elevant_mail_mcp::serve(&db, stdin, std::io::stdout().lock()) {
        Ok(()) => 0,
        Err(error) => {
            tracing::error!(%error, "MCP connection failed");
            1
        }
    }
}

/// tauri.conf.json's identifier: Tauri keeps the app's data in a folder of this name.
const IDENTIFIER: &str = "dev.elevant.mail";

/// Logs to stderr. The level comes from `ELEVANT_MAIL_LOG` (e.g. `debug`), default `info`.
/// Credentials never reach a log line: see `elevant_mail_core::redact`.
fn init_logging() {
    let filter =
        EnvFilter::try_from_env("ELEVANT_MAIL_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        // Never stdout: in `mcp` mode it carries the protocol.
        .with_writer(std::io::stderr)
        // Colours only for a person at a terminal, not in an agent's log.
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .init();
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_mcp_server_finds_the_apps_data_folder() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(config["identifier"], super::IDENTIFIER);
    }
}
