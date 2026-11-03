//! Changes AI agents made over MCP (`elevant-mail mcp`, another process) reach the server here:
//! the app looks at the agent log every few seconds, refreshes what changed, uploads the drafts
//! agents wrote and sends the queued changes once they can no longer be undone. And what agents
//! need from the server, the whole of a large mail or an attachment, the app fetches for them
//! ([`serve_requests`]): the MCP server has no credentials and waits for it.

use std::time::Duration;

use elevant_mail_core::agents::{self, Request, RequestKind};
use elevant_mail_core::mail::{download, drafts, push, sync};
use elevant_mail_core::moves::UNDO_WINDOW_MS;
use elevant_mail_core::{Error, mailbox};
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

const LOOK_EVERY: Duration = Duration::from_secs(3);
/// Agents wait for these, so they are looked at more often.
const REQUESTS_EVERY: Duration = Duration::from_secs(1);
/// Old requests and fetched files are dropped about once an hour.
const FORGET_EVERY: u32 = 3600;
/// Sent with the account id when its cached mail changed (see commands::compose).
const CACHE_CHANGED: &str = "cache-changed";

pub fn watch(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        // Drafts agents wrote while the app was closed; queued changes go with the next sync.
        upload_drafts(&app).await;
        let mut seen = agents::last_change(&state.db.conn()).unwrap_or(0);
        loop {
            tokio::time::sleep(LOOK_EVERY).await;
            let changed = {
                let conn = state.db.conn();
                agents::last_change(&conn).and_then(|latest| {
                    let accounts = if latest > seen {
                        agents::changed_since(&conn, seen)?
                    } else {
                        Vec::new()
                    };
                    Ok((latest, accounts))
                })
            };
            let (latest, accounts) = match changed {
                Ok(changed) => changed,
                Err(error) => {
                    tracing::warn!(%error, "could not look for changes by agents");
                    continue;
                }
            };
            seen = latest;
            if accounts.is_empty() {
                continue;
            }
            for account_id in &accounts {
                let _ = app.emit(CACHE_CHANGED, account_id);
            }
            upload_drafts(&app).await;
            // Moves wait out the undo window like the user's own; then the queue goes out.
            let later = app.clone();
            tauri::async_runtime::spawn(async move {
                let wait = u64::try_from(UNDO_WINDOW_MS).unwrap_or(10_000) + 1_000;
                tokio::time::sleep(Duration::from_millis(wait)).await;
                for account_id in accounts {
                    push_account(&later, &account_id).await;
                }
            });
        }
    });
}

async fn upload_drafts(app: &AppHandle) {
    let state = app.state::<AppState>();
    let listed = agents::drafts_to_upload(&state.db.conn());
    let keys = match listed {
        Ok(keys) => keys,
        Err(error) => {
            tracing::warn!(%error, "could not list the drafts agents wrote");
            return;
        }
    };
    for key in keys {
        match drafts::upload(&state.db, state.secrets.as_ref(), &key).await {
            Ok(_) => {}
            Err(error) => tracing::warn!(%error, "could not upload a draft an agent wrote"),
        }
    }
}

async fn push_account(app: &AppHandle, account_id: &str) {
    let state = app.state::<AppState>();
    let lock = state.account_lock(account_id);
    let _turn = lock.lock().await;
    match push::push_actions(&state.db, state.secrets.as_ref(), account_id).await {
        Ok(_) => {
            let _ = app.emit(CACHE_CHANGED, account_id);
        }
        // Left in the queue; the account's next sync tries again.
        Err(error) => tracing::warn!(%error, "could not send changes an agent made"),
    }
}

/// Fetches what agents asked for, one request at a time, with the account's lock like any other
/// server operation. The request is marked done or failed; the MCP server reads the result from
/// the cache.
pub fn serve_requests(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let mut rounds: u32 = 0;
        loop {
            tokio::time::sleep(REQUESTS_EVERY).await;
            rounds = rounds.wrapping_add(1);
            if rounds.is_multiple_of(FORGET_EVERY)
                && let Err(error) = agents::forget_old(&state.db.conn())
            {
                tracing::warn!(%error, "could not drop old requests of agents");
            }
            let open = agents::open_requests(&state.db.conn());
            let open = match open {
                Ok(open) => open,
                Err(error) => {
                    tracing::warn!(%error, "could not look for requests of agents");
                    continue;
                }
            };
            for request in open {
                let outcome = fetch(&state, &request).await.map_err(|error| {
                    tracing::warn!(%error, "could not fetch what an agent asked for");
                    error.to_string()
                });
                if outcome.is_ok()
                    && request.kind == RequestKind::Body
                    && let Ok(Some(account)) =
                        mailbox::message_account(&state.db.conn(), &request.message_id)
                {
                    let _ = app.emit(CACHE_CHANGED, account);
                }
                if let Err(error) = agents::finish_request(&state.db.conn(), &request.id, outcome) {
                    tracing::warn!(%error, "could not answer a request of an agent");
                }
            }
        }
    });
}

async fn fetch(state: &AppState, request: &Request) -> Result<(), Error> {
    match request.kind {
        RequestKind::Body => {
            let account = mailbox::message_account(&state.db.conn(), &request.message_id)?
                .ok_or(Error::MessageNotFound)?;
            let lock = state.account_lock(&account);
            let _turn = lock.lock().await;
            sync::load_body(&state.db, state.secrets.as_ref(), &request.message_id).await
        }
        RequestKind::Attachment => {
            let id = request
                .attachment_id
                .as_deref()
                .ok_or(Error::AttachmentNotFound)?;
            let source = download::locate(&state.db, id)?;
            if source.message_id != request.message_id {
                return Err(Error::AttachmentNotFound);
            }
            let lock = state.account_lock(&source.account_id);
            let _turn = lock.lock().await;
            let file = download::fetch_attachment(&state.db, state.secrets.as_ref(), id).await?;
            agents::store_file(&state.db.conn(), id, &file.data)
        }
    }
}
