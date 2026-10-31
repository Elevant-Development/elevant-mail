//! The app in the background: closing the window hides it while the tray icon stays, so syncing
//! and notifications go on. The tray menu opens the window, starts a new mail or quits. Without
//! a tray (none on the system) closing quits, so the app never runs out of reach.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Deserialize;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime, Wry};

/// Sent to the frontend when "New mail" is chosen in the tray menu.
pub const COMPOSE_EVENT: &str = "tray-compose";
/// Sent when closing hid the window, so the frontend can say once where the app went.
pub const HIDDEN_EVENT: &str = "window-hidden";
/// Started at login: the window stays hidden until opened from the tray.
pub const HIDDEN_ARG: &str = "--hidden";

const OPEN_ID: &str = "open";
const COMPOSE_ID: &str = "compose";
const QUIT_ID: &str = "quit";

pub struct Background {
    /// The user's setting, sent by the frontend at start; on until then.
    keep_running: AtomicBool,
    tray: Mutex<Option<TrayIcon<Wry>>>,
}

/// The menu's words in the interface language, which only the frontend knows.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayTexts {
    open: String,
    compose: String,
    quit: String,
    /// The tooltip, with the unread count already in it.
    tooltip: String,
}

pub fn init(app: &AppHandle) {
    let tray = build(app, &default_texts())
        .inspect_err(|error| tracing::warn!(%error, "no tray icon, closing quits the app"))
        .ok();
    app.manage(Background {
        keep_running: AtomicBool::new(true),
        tray: Mutex::new(tray),
    });
}

fn default_texts() -> TrayTexts {
    TrayTexts {
        open: "Open Elevant Mail".into(),
        compose: "New mail".into(),
        quit: "Quit".into(),
        tooltip: "Elevant Mail".into(),
    }
}

fn menu<R: Runtime>(app: &AppHandle<R>, texts: &TrayTexts) -> tauri::Result<Menu<R>> {
    Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, OPEN_ID, &texts.open, true, None::<&str>)?,
            &MenuItem::with_id(app, COMPOSE_ID, &texts.compose, true, None::<&str>)?,
            &MenuItem::with_id(app, QUIT_ID, &texts.quit, true, None::<&str>)?,
        ],
    )
}

fn build(app: &AppHandle, texts: &TrayTexts) -> tauri::Result<TrayIcon<Wry>> {
    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu(app, texts)?)
        .tooltip(&texts.tooltip)
        // A left click opens the window (Windows); Linux trays only show the menu.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            OPEN_ID => show(app),
            COMPOSE_ID => {
                show(app);
                let _ = app.emit(COMPOSE_EVENT, ());
            }
            QUIT_ID => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)
}

/// Brings the main window back and to the front.
pub fn show<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Closing the window: hidden while a tray icon is there and the setting allows it. True when
/// the close was turned into hiding.
pub fn hide_instead_of_close(window: &tauri::Window) -> bool {
    let background = window.state::<Background>();
    let has_tray = background
        .tray
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .is_some();
    if !has_tray || !background.keep_running.load(Ordering::Relaxed) {
        return false;
    }
    let _ = window.hide();
    let _ = window.emit(HIDDEN_EVENT, ());
    true
}

/// The setting and the menu's words, sent by the frontend at start and whenever they change.
#[tauri::command]
pub fn configure_background(
    app: AppHandle,
    background: tauri::State<'_, Background>,
    keep_running: bool,
    texts: TrayTexts,
) -> Result<(), crate::error::CommandError> {
    background
        .keep_running
        .store(keep_running, Ordering::Relaxed);
    if let Some(tray) = background
        .tray
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
    {
        tray.set_menu(Some(menu(&app, &texts)?))?;
        tray.set_tooltip(Some(&texts.tooltip))?;
    }
    Ok(())
}
