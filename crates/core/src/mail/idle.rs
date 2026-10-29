//! Waiting for new mail with IMAP IDLE (RFC 2177). One extra connection per account sits in the
//! inbox and reports whenever the server announces a change, so the inbox is synced within
//! seconds instead of at the next poll. The connection only waits; the sync runs as usual on its
//! own connection. Servers without IDLE keep the poll.

use std::future::Future;
use std::time::{Duration, Instant};

use async_imap::extensions::idle::IdleResponse;
use rusqlite::OptionalExtension;
use tokio_rustls::TlsConnector;

use crate::Result;
use crate::db::Database;
use crate::mail::imap::{self, COMMAND_TIMEOUT, protocol_error, with_timeout};
use crate::mail::sync::credentials;
use crate::secrets::SecretStore;

/// IDLE is ended and started again before servers give up on it (29 minutes, RFC 2177).
const IDLE_REFRESH: Duration = Duration::from_secs(25 * 60);
/// After a failure the watcher waits this long before connecting again, doubling up to the max.
const RETRY_FIRST: Duration = Duration::from_secs(30);
const RETRY_MAX: Duration = Duration::from_secs(10 * 60);
/// A connection that held this long counts as working again: the next failure retries soon.
const STABLE_AFTER: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchEnd {
    /// `stop` completed.
    Stopped,
    /// The server does not offer IDLE; polling has to do.
    Unsupported,
}

/// Watches the account's inbox until `stop` completes and calls `changed` whenever the server
/// reports something there (new mail, deleted mail, changed flags). Connection trouble is retried
/// with growing pauses, so a laptop that wakes up or a server that restarts is picked up again.
pub async fn watch_inbox(
    db: &Database,
    secrets: &dyn SecretStore,
    account_id: &str,
    changed: impl Fn() + Send + Sync,
    stop: impl Future<Output = ()>,
) -> WatchEnd {
    watch_with(
        db,
        secrets,
        account_id,
        changed,
        stop,
        imap::platform_connector,
    )
    .await
}

pub(crate) async fn watch_with(
    db: &Database,
    secrets: &dyn SecretStore,
    account_id: &str,
    changed: impl Fn() + Send + Sync,
    stop: impl Future<Output = ()>,
    connector: impl Fn() -> Result<TlsConnector>,
) -> WatchEnd {
    tokio::pin!(stop);
    let mut retry = RETRY_FIRST;
    loop {
        let started = Instant::now();
        let watched = tokio::select! {
            watched = watch_once(db, secrets, account_id, &changed, &connector) => watched,
            () = &mut stop => return WatchEnd::Stopped,
        };
        match watched {
            Ok(end) => return end,
            Err(error) => {
                if started.elapsed() > STABLE_AFTER {
                    retry = RETRY_FIRST;
                }
                tracing::info!(%error, retry_in = ?retry, "waiting for new mail stopped");
            }
        }
        tokio::select! {
            () = tokio::time::sleep(retry) => {}
            () = &mut stop => return WatchEnd::Stopped,
        }
        retry = (retry * 2).min(RETRY_MAX);
    }
}

/// One connection, waiting in IDLE again and again. Only returns with an error, or when the
/// server cannot IDLE.
async fn watch_once(
    db: &Database,
    secrets: &dyn SecretStore,
    account_id: &str,
    changed: &(impl Fn() + Send + Sync),
    connector: &impl Fn() -> Result<TlsConnector>,
) -> Result<WatchEnd> {
    let (settings, credential) = credentials(db, secrets, account_id).await?;
    let mut session = imap::open_session(&settings, &credential, &connector()?).await?;
    let capabilities = with_timeout(COMMAND_TIMEOUT, session.capabilities())
        .await?
        .map_err(protocol_error)?;
    if !capabilities.has_str("IDLE") {
        let _ = with_timeout(COMMAND_TIMEOUT, session.logout()).await;
        return Ok(WatchEnd::Unsupported);
    }
    let inbox: String = db
        .conn()
        .query_row(
            "SELECT path FROM folders WHERE account_id = ?1 AND role = 'inbox'",
            [account_id],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or_else(|| "INBOX".to_owned());
    // Read-only, like the sync: waiting never changes anything on the server.
    with_timeout(COMMAND_TIMEOUT, session.examine(&inbox))
        .await?
        .map_err(protocol_error)?;
    tracing::info!(folder = %inbox, "waiting for new mail");

    loop {
        let mut idle = session.idle();
        with_timeout(COMMAND_TIMEOUT, idle.init())
            .await?
            .map_err(protocol_error)?;
        let (wait, interrupt) = idle.wait_with_timeout(IDLE_REFRESH);
        let response = wait.await.map_err(protocol_error)?;
        drop(interrupt);
        session = with_timeout(COMMAND_TIMEOUT, idle.done())
            .await?
            .map_err(protocol_error)?;
        if matches!(response, IdleResponse::NewData(_)) {
            changed();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;
    use crate::accounts::{Login, NewAccount, save};
    use crate::mail::folders::RemoteFolder;
    use crate::mail::imap::ConnectionReport;
    use crate::mail::settings::{ImapSettings, Security};
    use crate::mail::test_server::{self, Mailbox, Mode, PASSWORD, TestMessage, TestServer};
    use crate::secrets::{MemoryStore, Secret};

    fn account_on(server: &TestServer) -> (Database, MemoryStore, String) {
        let db = Database::open_in_memory().unwrap();
        let secrets = MemoryStore::default();
        let account = save(
            &db,
            &secrets,
            &NewAccount {
                display_name: "Personal".into(),
                email_address: "alex@example.org".into(),
                imap: ImapSettings {
                    host: "localhost".into(),
                    port: server.port,
                    security: Security::Tls,
                    username: "alex@example.org".into(),
                },
            },
            Login::Password(&Secret::new(PASSWORD)),
            &ConnectionReport {
                folders: vec![RemoteFolder::new("INBOX", Some("/"), None, true)],
            },
        )
        .unwrap();
        (db, secrets, account.id)
    }

    async fn until(condition: impl Fn() -> bool) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !condition() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("timed out");
    }

    #[tokio::test]
    async fn new_mail_is_reported_while_waiting() {
        let server = test_server::start_with(Mode::Tls, Mailbox::with_messages(1)).await;
        let (db, secrets, account_id) = account_on(&server);
        let reports = Arc::new(AtomicU32::new(0));
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let connector = server.connector.clone();

        let counted = reports.clone();
        let watcher = watch_with(
            &db,
            &secrets,
            &account_id,
            move || {
                counted.fetch_add(1, Ordering::SeqCst);
            },
            async {
                let _ = stopped.await;
            },
            move || Ok(connector.clone()),
        );
        let server_side = async {
            until(|| server.commands().iter().any(|c| c.starts_with("IDLE"))).await;
            server.deliver(TestMessage::new(2));
            until(|| reports.load(Ordering::SeqCst) == 1).await;
            // Waiting again after reporting, in the read-only inbox.
            until(|| {
                server
                    .commands()
                    .iter()
                    .filter(|c| c.starts_with("IDLE"))
                    .count()
                    == 2
            })
            .await;
            stop.send(()).unwrap();
        };

        let (end, ()) = tokio::join!(watcher, server_side);
        assert_eq!(end, WatchEnd::Stopped);
        assert!(server.commands().iter().any(|c| c == "EXAMINE \"INBOX\""));
    }

    #[tokio::test]
    async fn servers_without_idle_are_left_to_polling() {
        let server = test_server::start_with(Mode::Tls, Mailbox::with_messages(1)).await;
        server.mailbox.lock().unwrap().without_idle = true;
        let (db, secrets, account_id) = account_on(&server);
        let connector = server.connector.clone();

        let end = watch_with(
            &db,
            &secrets,
            &account_id,
            || {},
            std::future::pending(),
            move || Ok(connector.clone()),
        )
        .await;

        assert_eq!(end, WatchEnd::Unsupported);
    }
}
