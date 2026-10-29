//! One IDLE watcher per account (`elevant_mail_core::mail::idle`): started for every account at
//! launch and when one is added, restarted after a new login, stopped when the account goes. A
//! change in an inbox reaches the frontend as `inbox-changed`, which syncs that account.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use elevant_mail_core::mail::idle::{self, WatchEnd};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::oneshot;

use crate::state::AppState;

/// Event to the frontend, with the account id as payload.
pub const INBOX_CHANGED: &str = "inbox-changed";

#[derive(Default)]
pub struct Watchers {
    running: Mutex<HashMap<String, oneshot::Sender<()>>>,
}

impl Watchers {
    /// Starts watching the account's inbox; a watcher already running for it is replaced.
    pub fn start(&self, app: &AppHandle, account_id: &str) {
        let (stop, stopped) = oneshot::channel();
        if let Some(previous) = self
            .running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(account_id.to_owned(), stop)
        {
            let _ = previous.send(());
        }
        let app = app.clone();
        let account_id = account_id.to_owned();
        tauri::async_runtime::spawn(async move {
            let state = app.state::<AppState>();
            let end = idle::watch_inbox(
                &state.db,
                state.secrets.as_ref(),
                &account_id,
                || {
                    let _ = app.emit(INBOX_CHANGED, &account_id);
                },
                async {
                    let _ = stopped.await;
                },
            )
            .await;
            if end == WatchEnd::Unsupported {
                tracing::info!("server cannot wait for new mail (IDLE), the poll checks instead");
            }
        });
    }

    pub fn stop(&self, account_id: &str) {
        if let Some(stop) = self
            .running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(account_id)
        {
            let _ = stop.send(());
        }
    }
}
