//! Elevant Mail backend. For now it only opens the window; IMAP, SQLite and the keyring
//! follow step by step (see docs/projekt-brief.md).

pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("failed to start Elevant Mail");
}
