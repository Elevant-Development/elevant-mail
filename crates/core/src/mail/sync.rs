//! Brings a folder of the cache in line with the server.
//!
//! - UIDVALIDITY changed: the folder's cache is dropped and fetched again.
//! - New mail: everything above the newest cached UID; the first sync takes the newest 100.
//! - Mail deleted on the server is removed from the cache.
//! - Read and flagged state of cached mail follows the server.
//! - Older mail comes on request ([`load_older`]), 100 at a time.
//!
//! The folder is opened with EXAMINE (read-only) and bodies are fetched with BODY.PEEK, so
//! syncing never marks anything as read.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use async_imap::types::{Fetch, Flag};
use futures::TryStreamExt;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
#[cfg(test)]
use tokio_rustls::TlsConnector;
use ts_rs::TS;

use crate::db::Database;
use crate::ids::{new_id, now_millis};
use crate::mail::imap::{
    self, COMMAND_TIMEOUT, ImapSession, list_folders, protocol_error, with_timeout,
};
use crate::mail::parse::{ParsedMessage, parse_message, parse_preview};
use crate::mail::settings::{ImapSettings, Security};
use crate::secrets::{SecretKind, SecretStore};
use crate::{Error, Result};

/// Messages per batch: the first sync of a folder and every "load older".
pub const BATCH_SIZE: usize = 100;
/// Larger messages are fetched without body; the reader loads them when opened.
const BODY_SIZE_LIMIT: u32 = 512 * 1024;
/// For those, this much of the body is fetched for the preview line.
const PREVIEW_BYTES: u32 = 64 * 1024;
const FETCH_CHUNK: usize = 25;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SyncStage {
    Connecting,
    Checking,
    Fetching,
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncProgress {
    pub account_id: String,
    pub folder_id: Option<String>,
    pub stage: SyncStage,
    pub done: u32,
    pub total: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncReport {
    pub added: u32,
    pub updated: u32,
    pub removed: u32,
}

pub type ProgressFn<'a> = &'a (dyn Fn(SyncProgress) + Send + Sync);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Newest,
    Older,
}

struct FolderState {
    id: String,
    account_id: String,
    path: String,
    uid_validity: Option<u32>,
}

/// Syncs the inbox of an account (creating the folder list first if it is missing).
pub async fn sync_account(
    db: &Database,
    secrets: &dyn SecretStore,
    account_id: &str,
    progress: ProgressFn<'_>,
) -> Result<SyncReport> {
    let (settings, password) = credentials(db, secrets, account_id)?;
    let connector = imap::platform_connector()?;
    report(progress, account_id, None, SyncStage::Connecting, 0, 0);
    let mut session = imap::open_session(&settings, &password, &connector).await?;
    let result = sync_inbox_in(db, &mut session, account_id, progress).await;
    let _ = with_timeout(COMMAND_TIMEOUT, session.logout()).await;
    result
}

/// Syncs one folder (new, changed and deleted mail).
pub async fn sync_folder(
    db: &Database,
    secrets: &dyn SecretStore,
    folder_id: &str,
    progress: ProgressFn<'_>,
) -> Result<SyncReport> {
    run_on_folder(db, secrets, folder_id, Mode::Newest, progress).await
}

/// Fetches the next batch of mail older than the oldest one in the cache.
pub async fn load_older(
    db: &Database,
    secrets: &dyn SecretStore,
    folder_id: &str,
    progress: ProgressFn<'_>,
) -> Result<SyncReport> {
    run_on_folder(db, secrets, folder_id, Mode::Older, progress).await
}

async fn run_on_folder(
    db: &Database,
    secrets: &dyn SecretStore,
    folder_id: &str,
    mode: Mode,
    progress: ProgressFn<'_>,
) -> Result<SyncReport> {
    let folder = folder_state(&db.conn(), folder_id)?.ok_or(Error::FolderNotFound)?;
    let (settings, password) = credentials(db, secrets, &folder.account_id)?;
    let connector = imap::platform_connector()?;
    report(
        progress,
        &folder.account_id,
        Some(folder_id),
        SyncStage::Connecting,
        0,
        0,
    );
    let mut session = imap::open_session(&settings, &password, &connector).await?;
    let result = sync_folder_in(db, &mut session, &folder, mode, progress).await;
    let _ = with_timeout(COMMAND_TIMEOUT, session.logout()).await;
    result
}

pub(crate) async fn sync_inbox_in(
    db: &Database,
    session: &mut ImapSession,
    account_id: &str,
    progress: ProgressFn<'_>,
) -> Result<SyncReport> {
    // Bound first: in a `match` the lock guard would live through the arms and their awaits.
    let known = inbox_state(&db.conn(), account_id)?;
    let inbox = match known {
        Some(inbox) => inbox,
        None => {
            let folders = with_timeout(COMMAND_TIMEOUT, list_folders(session)).await??;
            crate::accounts::store_folders(&db.conn(), account_id, &folders)?;
            let stored = inbox_state(&db.conn(), account_id)?;
            stored.ok_or(Error::FolderNotFound)?
        }
    };
    sync_folder_in(db, session, &inbox, Mode::Newest, progress).await
}

async fn sync_folder_in(
    db: &Database,
    session: &mut ImapSession,
    folder: &FolderState,
    mode: Mode,
    progress: ProgressFn<'_>,
) -> Result<SyncReport> {
    let folder_id = Some(folder.id.as_str());
    report(
        progress,
        &folder.account_id,
        folder_id,
        SyncStage::Checking,
        0,
        0,
    );

    let mailbox = with_timeout(COMMAND_TIMEOUT, session.examine(&folder.path))
        .await?
        .map_err(protocol_error)?;
    if folder.uid_validity.is_some() && mailbox.uid_validity != folder.uid_validity {
        tracing::info!(folder = %folder.path, "UIDVALIDITY changed, rebuilding the folder cache");
        db.conn()
            .execute("DELETE FROM messages WHERE folder_id = ?1", [&folder.id])?;
    }

    let server_uids: BTreeSet<u32> = with_timeout(COMMAND_TIMEOUT, session.uid_search("ALL"))
        .await?
        .map_err(protocol_error)?
        .into_iter()
        .collect();
    let cached = cached_flags(&db.conn(), &folder.id)?;

    let mut result = SyncReport::default();
    let to_fetch: Vec<u32> = match (mode, cached.keys().next(), cached.keys().next_back()) {
        (Mode::Newest, _, Some(&newest)) => server_uids.range(newest + 1..).copied().collect(),
        (Mode::Newest, _, None) => newest(&server_uids, None),
        (Mode::Older, Some(&oldest), _) => newest(&server_uids, Some(oldest)),
        (Mode::Older, None, _) => newest(&server_uids, None),
    };

    if mode == Mode::Newest {
        let gone: Vec<u32> = cached
            .keys()
            .filter(|uid| !server_uids.contains(uid))
            .copied()
            .collect();
        result.removed = remove_messages(&db.conn(), &folder.id, &gone)?;
        let still_there: Vec<u32> = cached
            .keys()
            .filter(|uid| server_uids.contains(uid))
            .copied()
            .collect();
        result.updated = update_flags(db, session, &folder.id, &cached, &still_there).await?;
    }

    let total = u32::try_from(to_fetch.len()).unwrap_or(u32::MAX);
    report(
        progress,
        &folder.account_id,
        folder_id,
        SyncStage::Fetching,
        0,
        total,
    );
    for chunk in to_fetch.chunks(FETCH_CHUNK) {
        result.added += fetch_and_store(db, session, folder, chunk).await?;
        report(
            progress,
            &folder.account_id,
            folder_id,
            SyncStage::Fetching,
            result.added,
            total,
        );
    }

    let unseen = with_timeout(COMMAND_TIMEOUT, session.uid_search("UNSEEN"))
        .await?
        .map_err(protocol_error)?
        .len();
    let unseen = u32::try_from(unseen).unwrap_or(u32::MAX);
    db.conn().execute(
        "UPDATE folders SET uid_validity = ?2, uid_next = ?3, total_count = ?4, unread_count = ?5,
           last_synced_at = ?6 WHERE id = ?1",
        params![
            folder.id,
            mailbox.uid_validity,
            mailbox.uid_next,
            mailbox.exists,
            unseen,
            now_millis()
        ],
    )?;

    report(
        progress,
        &folder.account_id,
        folder_id,
        SyncStage::Done,
        result.added,
        total,
    );
    tracing::info!(folder = %folder.path, ?mode, added = result.added, updated = result.updated, removed = result.removed, "folder synced");
    Ok(result)
}

/// Fetches the full body of a message that was cached with headers only (large mail).
pub async fn load_body(db: &Database, secrets: &dyn SecretStore, message_id: &str) -> Result<()> {
    let target = body_target(&db.conn(), message_id)?.ok_or(Error::MessageNotFound)?;
    if target.complete {
        return Ok(());
    }
    let (settings, password) = credentials(db, secrets, &target.account_id)?;
    let connector = imap::platform_connector()?;
    let mut session = imap::open_session(&settings, &password, &connector).await?;
    let result = load_body_in(db, &mut session, message_id, &target).await;
    let _ = with_timeout(COMMAND_TIMEOUT, session.logout()).await;
    result
}

struct BodyTarget {
    account_id: String,
    folder_path: String,
    uid: u32,
    complete: bool,
}

fn body_target(conn: &Connection, message_id: &str) -> Result<Option<BodyTarget>> {
    Ok(conn
        .query_row(
            "SELECT m.account_id, f.path, m.uid, m.body_state = 'full'
             FROM messages m JOIN folders f ON f.id = m.folder_id WHERE m.id = ?1",
            [message_id],
            |row| {
                Ok(BodyTarget {
                    account_id: row.get(0)?,
                    folder_path: row.get(1)?,
                    uid: row.get(2)?,
                    complete: row.get(3)?,
                })
            },
        )
        .optional()?)
}

async fn load_body_in(
    db: &Database,
    session: &mut ImapSession,
    message_id: &str,
    target: &BodyTarget,
) -> Result<()> {
    with_timeout(COMMAND_TIMEOUT, session.examine(&target.folder_path))
        .await?
        .map_err(protocol_error)?;
    let fetches = fetch_within(session, &[target.uid], "(UID BODY.PEEK[])", BODY_TIMEOUT).await?;
    // Gone on the server since the last sync: the next sync removes it from the cache.
    let raw = fetches
        .iter()
        .find_map(Fetch::body)
        .ok_or(Error::MessageNotFound)?;
    let message = parse_message(raw);
    db.conn().execute(
        "UPDATE messages SET body_text = ?2, snippet = ?3, has_attachments = ?4, body_state = 'full'
         WHERE id = ?1",
        params![message_id, message.body_text, message.snippet, message.has_attachments],
    )?;
    Ok(())
}

/// The newest batch of UIDs, below `before` if given.
fn newest(uids: &BTreeSet<u32>, before: Option<u32>) -> Vec<u32> {
    let candidates = match before {
        Some(before) => uids.range(..before),
        None => uids.range(..),
    };
    let mut batch: Vec<u32> = candidates.rev().take(BATCH_SIZE).copied().collect();
    batch.reverse();
    batch
}

/// Compact UID set for IMAP commands: `1:3,7,9:12`.
pub(crate) fn uid_set(uids: &[u32]) -> String {
    let mut sorted = uids.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut parts = Vec::new();
    let mut iter = sorted.into_iter().peekable();
    while let Some(start) = iter.next() {
        let mut end = start;
        while iter.peek() == Some(&(end + 1)) {
            end = iter.next().unwrap_or(end);
        }
        parts.push(if start == end {
            start.to_string()
        } else {
            format!("{start}:{end}")
        });
    }
    parts.join(",")
}

async fn fetch_all(session: &mut ImapSession, uids: &[u32], query: &str) -> Result<Vec<Fetch>> {
    fetch_within(session, uids, query, COMMAND_TIMEOUT * 2).await
}

/// A large message with attachments can take minutes on a slow connection.
const BODY_TIMEOUT: Duration = Duration::from_secs(300);

async fn fetch_within(
    session: &mut ImapSession,
    uids: &[u32],
    query: &str,
    timeout: Duration,
) -> Result<Vec<Fetch>> {
    if uids.is_empty() {
        return Ok(Vec::new());
    }
    let set = uid_set(uids);
    let fetches = async {
        session
            .uid_fetch(&set, query)
            .await
            .map_err(protocol_error)?
            .try_collect::<Vec<_>>()
            .await
            .map_err(protocol_error)
    };
    with_timeout(timeout, fetches).await?
}

struct Meta {
    is_read: bool,
    is_flagged: bool,
    date_received: Option<i64>,
    size: u32,
}

fn meta_of(fetch: &Fetch) -> Meta {
    let flags: Vec<Flag<'_>> = fetch.flags().collect();
    Meta {
        is_read: flags.contains(&Flag::Seen),
        is_flagged: flags.contains(&Flag::Flagged),
        date_received: fetch.internal_date().map(|date| date.timestamp_millis()),
        size: fetch.size.unwrap_or(0),
    }
}

async fn fetch_and_store(
    db: &Database,
    session: &mut ImapSession,
    folder: &FolderState,
    uids: &[u32],
) -> Result<u32> {
    let metas: BTreeMap<u32, Meta> =
        fetch_all(session, uids, "(UID FLAGS INTERNALDATE RFC822.SIZE)")
            .await?
            .iter()
            .filter_map(|fetch| fetch.uid.map(|uid| (uid, meta_of(fetch))))
            .collect();

    let (small, large): (Vec<u32>, Vec<u32>) = metas
        .keys()
        .copied()
        .partition(|uid| metas[uid].size <= BODY_SIZE_LIMIT);

    let mut parsed: BTreeMap<u32, (ParsedMessage, bool)> = BTreeMap::new();
    for fetch in fetch_all(session, &small, "(UID BODY.PEEK[])").await? {
        if let (Some(uid), Some(body)) = (fetch.uid, fetch.body()) {
            parsed.insert(uid, (parse_message(body), true));
        }
    }
    let preview_query = format!("(UID BODY.PEEK[HEADER] BODY.PEEK[TEXT]<0.{PREVIEW_BYTES}>)");
    for fetch in fetch_all(session, &large, &preview_query).await? {
        if let (Some(uid), Some(header)) = (fetch.uid, fetch.header()) {
            let preview = parse_preview(header, fetch.text().unwrap_or_default());
            parsed.insert(uid, (preview, false));
        }
    }

    let mut conn = db.conn();
    let tx = conn.transaction()?;
    let mut stored = 0;
    for (uid, (message, complete)) in &parsed {
        let Some(meta) = metas.get(uid) else { continue };
        store_message(&tx, folder, *uid, message, *complete, meta)?;
        stored += 1;
    }
    tx.commit()?;
    Ok(stored)
}

fn store_message(
    conn: &Connection,
    folder: &FolderState,
    uid: u32,
    message: &ParsedMessage,
    complete: bool,
    meta: &Meta,
) -> Result<()> {
    let to_json = |addresses: &[crate::model::Address]| serde_json::to_string(addresses).ok();
    conn.execute(
        "INSERT INTO messages (id, account_id, folder_id, uid, message_id, in_reply_to,
           references_header, subject, from_name, from_address, to_addresses, cc_addresses,
           date_sent, date_received, size_bytes, snippet, is_read, is_flagged, has_attachments,
           body_state, body_text, fetched_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
           ?19, ?20, ?21, ?22)
         ON CONFLICT (folder_id, uid) DO UPDATE
           SET is_read = excluded.is_read, is_flagged = excluded.is_flagged",
        params![
            new_id(),
            folder.account_id,
            folder.id,
            uid,
            message.message_id,
            message.in_reply_to,
            message.references,
            message.subject,
            message.from.as_ref().and_then(|from| from.name.clone()),
            message.from.as_ref().map(|from| from.address.clone()),
            to_json(&message.to),
            to_json(&message.cc),
            message.date_sent.or(meta.date_received),
            meta.date_received,
            meta.size,
            message.snippet,
            meta.is_read,
            meta.is_flagged,
            message.has_attachments,
            if complete { "full" } else { "none" },
            message.body_text,
            now_millis(),
        ],
    )?;
    Ok(())
}

/// Read and flagged state from the server for cached mail; returns how many changed.
async fn update_flags(
    db: &Database,
    session: &mut ImapSession,
    folder_id: &str,
    cached: &BTreeMap<u32, (bool, bool)>,
    uids: &[u32],
) -> Result<u32> {
    let mut changed = Vec::new();
    for chunk in uids.chunks(500) {
        for fetch in fetch_all(session, chunk, "(UID FLAGS)").await? {
            let Some(uid) = fetch.uid else { continue };
            let meta = meta_of(&fetch);
            if cached.get(&uid) != Some(&(meta.is_read, meta.is_flagged)) {
                changed.push((uid, meta.is_read, meta.is_flagged));
            }
        }
    }
    let mut conn = db.conn();
    let tx = conn.transaction()?;
    for (uid, is_read, is_flagged) in &changed {
        tx.execute(
            "UPDATE messages SET is_read = ?3, is_flagged = ?4 WHERE folder_id = ?1 AND uid = ?2",
            params![folder_id, uid, is_read, is_flagged],
        )?;
    }
    tx.commit()?;
    Ok(u32::try_from(changed.len()).unwrap_or(u32::MAX))
}

fn remove_messages(conn: &Connection, folder_id: &str, uids: &[u32]) -> Result<u32> {
    let mut statement = conn.prepare("DELETE FROM messages WHERE folder_id = ?1 AND uid = ?2")?;
    let mut removed = 0;
    for uid in uids {
        removed += statement.execute(params![folder_id, uid])?;
    }
    Ok(u32::try_from(removed).unwrap_or(u32::MAX))
}

fn cached_flags(conn: &Connection, folder_id: &str) -> Result<BTreeMap<u32, (bool, bool)>> {
    let mut statement =
        conn.prepare("SELECT uid, is_read, is_flagged FROM messages WHERE folder_id = ?1")?;
    let flags = statement
        .query_map([folder_id], |row| {
            Ok((row.get(0)?, (row.get(1)?, row.get(2)?)))
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(flags)
}

fn folder_state(conn: &Connection, folder_id: &str) -> Result<Option<FolderState>> {
    Ok(conn
        .query_row(
            "SELECT id, account_id, path, uid_validity FROM folders WHERE id = ?1",
            [folder_id],
            |row| {
                Ok(FolderState {
                    id: row.get(0)?,
                    account_id: row.get(1)?,
                    path: row.get(2)?,
                    uid_validity: row.get(3)?,
                })
            },
        )
        .optional()?)
}

fn inbox_state(conn: &Connection, account_id: &str) -> Result<Option<FolderState>> {
    let id: Option<String> = conn
        .query_row(
            "SELECT id FROM folders WHERE account_id = ?1 AND role = 'inbox'",
            [account_id],
            |row| row.get(0),
        )
        .optional()?;
    match id {
        Some(id) => folder_state(conn, &id),
        None => Ok(None),
    }
}

fn credentials(
    db: &Database,
    secrets: &dyn SecretStore,
    account_id: &str,
) -> Result<(ImapSettings, crate::secrets::Secret)> {
    let settings = db
        .conn()
        .query_row(
            "SELECT imap_host, imap_port, imap_security, imap_username FROM accounts WHERE id = ?1",
            [account_id],
            |row| {
                let security: String = row.get(2)?;
                Ok(ImapSettings {
                    host: row.get(0)?,
                    port: row.get(1)?,
                    security: Security::from_db_str(&security),
                    username: row.get(3)?,
                })
            },
        )
        .optional()?
        .ok_or(Error::AccountNotFound)?;
    let password = secrets.get(account_id, SecretKind::ImapPassword)?;
    Ok((settings, password))
}

fn report(
    progress: ProgressFn<'_>,
    account_id: &str,
    folder_id: Option<&str>,
    stage: SyncStage,
    done: u32,
    total: u32,
) {
    progress(SyncProgress {
        account_id: account_id.to_owned(),
        folder_id: folder_id.map(str::to_owned),
        stage,
        done,
        total,
    });
}

/// For tests: sync with a session that is already logged in.
#[cfg(test)]
pub(crate) async fn sync_with_connector(
    db: &Database,
    secrets: &dyn SecretStore,
    account_id: &str,
    connector: &TlsConnector,
) -> Result<SyncReport> {
    let (settings, password) = credentials(db, secrets, account_id)?;
    let mut session = imap::open_session(&settings, &password, connector).await?;
    let result = sync_inbox_in(db, &mut session, account_id, &|_| {}).await;
    let _ = session.logout().await;
    result
}

#[cfg(test)]
pub(crate) async fn load_older_with_connector(
    db: &Database,
    secrets: &dyn SecretStore,
    folder_id: &str,
    connector: &TlsConnector,
) -> Result<SyncReport> {
    let folder = folder_state(&db.conn(), folder_id)?.ok_or(Error::FolderNotFound)?;
    let (settings, password) = credentials(db, secrets, &folder.account_id)?;
    let mut session = imap::open_session(&settings, &password, connector).await?;
    let result = sync_folder_in(db, &mut session, &folder, Mode::Older, &|_| {}).await;
    let _ = session.logout().await;
    result
}

#[cfg(test)]
pub(crate) async fn load_body_with_connector(
    db: &Database,
    secrets: &dyn SecretStore,
    message_id: &str,
    connector: &TlsConnector,
) -> Result<()> {
    let target = body_target(&db.conn(), message_id)?.ok_or(Error::MessageNotFound)?;
    let (settings, password) = credentials(db, secrets, &target.account_id)?;
    let mut session = imap::open_session(&settings, &password, connector).await?;
    let result = load_body_in(db, &mut session, message_id, &target).await;
    let _ = session.logout().await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::{NewAccount, save};
    use crate::mail::folders::RemoteFolder;
    use crate::mail::imap::ConnectionReport;
    use crate::mail::test_server::{self, Mailbox, Mode, PASSWORD, TestMessage, TestServer};
    use crate::mailbox::{UNIFIED_INBOX, list_folders, list_messages};
    use crate::model::FolderRole;
    use crate::secrets::{MemoryStore, Secret};

    struct Setup {
        db: Database,
        secrets: MemoryStore,
        account_id: String,
        inbox_id: String,
        server: TestServer,
    }

    async fn setup(messages: u32) -> Setup {
        let server = test_server::start_with(Mode::Tls, Mailbox::with_messages(messages)).await;
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
            &Secret::new(PASSWORD),
            &ConnectionReport {
                folders: vec![RemoteFolder::new("INBOX", Some("/"), None, true)],
            },
        )
        .unwrap();
        let inbox_id = list_folders(&db.conn())
            .unwrap()
            .into_iter()
            .find(|folder| folder.role == Some(FolderRole::Inbox))
            .unwrap()
            .id;
        Setup {
            db,
            secrets,
            account_id: account.id,
            inbox_id,
            server,
        }
    }

    impl Setup {
        async fn sync(&self) -> SyncReport {
            sync_with_connector(
                &self.db,
                &self.secrets,
                &self.account_id,
                &self.server.connector,
            )
            .await
            .unwrap()
        }

        fn cached_uids(&self) -> Vec<u32> {
            cached_flags(&self.db.conn(), &self.inbox_id)
                .unwrap()
                .into_keys()
                .collect()
        }
    }

    #[tokio::test]
    async fn first_sync_caches_the_newest_batch() {
        let setup = setup(120).await;
        let report = setup.sync().await;

        assert_eq!(report.added, 100);
        assert_eq!(setup.cached_uids(), (21..=120).collect::<Vec<_>>());
        let messages = list_messages(&setup.db.conn(), UNIFIED_INBOX).unwrap();
        assert_eq!(messages[0].subject.as_deref(), Some("Message 120"));
        assert_eq!(messages[0].from.name.as_deref(), Some("Lena Fischer"));
        assert_eq!(messages[0].snippet, "Body of message 120");

        let folder = list_folders(&setup.db.conn()).unwrap().remove(0);
        assert_eq!((folder.total_count, folder.unread_count), (120, 120));
    }

    #[tokio::test]
    async fn syncing_never_marks_mail_as_read() {
        let setup = setup(3).await;
        setup.sync().await;
        let commands = setup.server.mailbox.lock().unwrap().commands.clone();
        assert!(
            commands.iter().any(|c| c.starts_with("EXAMINE")),
            "{commands:?}"
        );
        assert!(
            !commands
                .iter()
                .any(|c| c.starts_with("SELECT") || c.contains("STORE")),
            "{commands:?}"
        );
        let fetches: Vec<_> = commands.iter().filter(|c| c.contains("BODY")).collect();
        assert!(
            fetches.iter().all(|c| c.contains("BODY.PEEK")),
            "{fetches:?}"
        );
    }

    #[tokio::test]
    async fn later_syncs_add_update_and_remove() {
        let setup = setup(5).await;
        setup.sync().await;
        {
            let mut mailbox = setup.server.mailbox.lock().unwrap();
            mailbox.messages.retain(|m| m.uid != 2);
            mailbox.messages[0].seen = true;
            mailbox.messages[0].flagged = true;
            mailbox.messages.push(TestMessage::new(6));
        }

        let report = setup.sync().await;

        assert_eq!(
            report,
            SyncReport {
                added: 1,
                updated: 1,
                removed: 1
            }
        );
        assert_eq!(setup.cached_uids(), vec![1, 3, 4, 5, 6]);
        let flags = cached_flags(&setup.db.conn(), &setup.inbox_id).unwrap();
        assert_eq!(flags[&1], (true, true));
        // A sync without changes changes nothing.
        assert_eq!(setup.sync().await, SyncReport::default());
    }

    #[tokio::test]
    async fn a_new_uidvalidity_rebuilds_the_cache() {
        let setup = setup(3).await;
        setup.sync().await;
        {
            let mut mailbox = setup.server.mailbox.lock().unwrap();
            mailbox.uid_validity = 2;
            mailbox.messages = (10..=11).map(TestMessage::new).collect();
        }
        let report = setup.sync().await;
        assert_eq!(report.added, 2);
        assert_eq!(setup.cached_uids(), vec![10, 11]);
    }

    #[tokio::test]
    async fn older_mail_is_loaded_in_batches() {
        let setup = setup(130).await;
        setup.sync().await;
        let report = load_older_with_connector(
            &setup.db,
            &setup.secrets,
            &setup.inbox_id,
            &setup.server.connector,
        )
        .await
        .unwrap();
        assert_eq!(report.added, 30);
        assert_eq!(setup.cached_uids().len(), 130);
    }

    #[tokio::test]
    async fn large_messages_are_cached_without_body() {
        let setup = setup(0).await;
        {
            let mut big = TestMessage::new(1);
            big.raw.extend(std::iter::repeat_n(b'x', 600 * 1024));
            setup.server.mailbox.lock().unwrap().messages.push(big);
        }
        setup.sync().await;
        let state: (String, Option<String>, String) = setup
            .db
            .conn()
            .query_row(
                "SELECT body_state, body_text, snippet FROM messages",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!((state.0.as_str(), state.1), ("none", None));
        // The preview still comes from the first part of the body.
        assert!(state.2.starts_with("Body of message 1"), "{}", state.2);
    }

    #[tokio::test]
    async fn a_large_body_is_loaded_when_opened() {
        let setup = setup(0).await;
        {
            let mut big = TestMessage::new(1);
            big.raw.extend(std::iter::repeat_n(b'x', 600 * 1024));
            setup.server.mailbox.lock().unwrap().messages.push(big);
        }
        setup.sync().await;
        let id: String = setup
            .db
            .conn()
            .query_row("SELECT id FROM messages", [], |row| row.get(0))
            .unwrap();
        assert!(
            !crate::mailbox::get_message(&setup.db.conn(), &id)
                .unwrap()
                .unwrap()
                .body_complete
        );

        load_body_with_connector(&setup.db, &setup.secrets, &id, &setup.server.connector)
            .await
            .unwrap();

        let message = crate::mailbox::get_message(&setup.db.conn(), &id)
            .unwrap()
            .unwrap();
        assert!(message.body_complete);
        assert!(message.body_text.unwrap().starts_with("Body of message 1"));
        let commands = setup.server.mailbox.lock().unwrap().commands.clone();
        assert!(
            !commands
                .iter()
                .any(|c| c.starts_with("SELECT") || c.contains("STORE"))
        );
    }

    #[test]
    fn uid_sets_are_compact() {
        assert_eq!(uid_set(&[5, 1, 2, 3, 7, 9, 10, 11, 3]), "1:3,5,7,9:11");
        assert_eq!(uid_set(&[42]), "42");
        assert_eq!(uid_set(&[]), "");
    }

    #[test]
    fn batches_take_the_newest_uids() {
        let uids: BTreeSet<u32> = (1..=250).collect();
        let first = newest(&uids, None);
        assert_eq!(first.len(), BATCH_SIZE);
        assert_eq!((first[0], first[99]), (151, 250));
        let older = newest(&uids, Some(151));
        assert_eq!((older[0], older[99]), (51, 150));
        assert_eq!(newest(&uids, Some(51)).len(), 50);
    }
}
