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

use crate::accounts::{self, AccountSettings, SignInMethod};
use crate::db::Database;
use crate::ids::{new_id, now_millis};
use crate::mail::imap::{
    self, COMMAND_TIMEOUT, Credential, ImapSession, list_folders, protocol_error, with_timeout,
};
use crate::mail::parse::{ParsedMessage, parse_message, parse_preview};
use crate::mail::settings::ImapSettings;
use crate::model::Address;
use crate::secrets::{SecretKind, SecretStore};
use crate::{Error, Result};

/// Messages per batch: the newest of a folder, which the sync keeps whatever their date, and
/// every "load older".
pub const BATCH_SIZE: usize = 100;
/// Larger messages are fetched without body; the reader loads them when opened.
const BODY_SIZE_LIMIT: u32 = 512 * 1024;
/// For those, this much of the body is fetched for the preview line.
const PREVIEW_BYTES: u32 = 64 * 1024;
pub(crate) const FETCH_CHUNK: usize = 25;

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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncReport {
    pub added: u32,
    pub updated: u32,
    pub removed: u32,
    /// Unread mail that arrived since the folder's last sync, for notifications. Empty on the
    /// first sync of a folder, which only fills the cache.
    pub new_mail: Vec<NewMail>,
}

/// What a notification about a new message shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NewMail {
    pub id: String,
    pub account_id: String,
    pub from: Address,
    pub subject: Option<String>,
}

pub type ProgressFn<'a> = &'a (dyn Fn(SyncProgress) + Send + Sync);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Newest,
    Older,
}

#[derive(Clone)]
pub(crate) struct FolderState {
    pub(crate) id: String,
    pub(crate) account_id: String,
    pub(crate) path: String,
    pub(crate) uid_validity: Option<u32>,
}

/// Syncs the inbox of an account (creating the folder list first if it is missing).
pub async fn sync_account(
    db: &Database,
    secrets: &dyn SecretStore,
    account_id: &str,
    progress: ProgressFn<'_>,
) -> Result<SyncReport> {
    let (settings, password) = credentials(db, secrets, account_id).await?;
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
    let (settings, password) = credentials(db, secrets, &folder.account_id).await?;
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
    let result = match push_before_sync(db, &mut session, &folder.account_id).await {
        Ok(()) => sync_folder_in(db, &mut session, &folder, mode, progress).await,
        Err(error) => Err(error),
    };
    let _ = with_timeout(COMMAND_TIMEOUT, session.logout()).await;
    result
}

pub(crate) async fn sync_inbox_in(
    db: &Database,
    session: &mut ImapSession,
    account_id: &str,
    progress: ProgressFn<'_>,
) -> Result<SyncReport> {
    push_before_sync(db, session, account_id).await?;
    // Folders made, renamed or deleted elsewhere show up here; one LIST is cheap.
    let folders = with_timeout(COMMAND_TIMEOUT, list_folders(session)).await??;
    crate::accounts::replace_folders(&db.conn(), account_id, &folders)?;
    let inbox = inbox_state(&db.conn(), account_id)?.ok_or(Error::FolderNotFound)?;
    let report = sync_folder_in(db, session, &inbox, Mode::Newest, progress).await?;
    update_folder_counts(db, session, account_id, &folders).await?;
    Ok(report)
}

/// Total and unread mail of every other folder (`STATUS`), so the sidebar shows them before a
/// folder is opened, and old folders without mail are easy to tell apart. A folder the server
/// refuses to report on keeps its numbers.
async fn update_folder_counts(
    db: &Database,
    session: &mut ImapSession,
    account_id: &str,
    folders: &[crate::mail::folders::RemoteFolder],
) -> Result<()> {
    for folder in folders.iter().filter(|folder| folder.selectable) {
        if folder.role == Some(crate::model::FolderRole::Inbox) {
            continue;
        }
        let status = match with_timeout(
            COMMAND_TIMEOUT,
            session.status(&folder.path, "(MESSAGES UNSEEN)"),
        )
        .await?
        {
            Ok(status) => status,
            Err(async_imap::error::Error::No(_) | async_imap::error::Error::Bad(_)) => continue,
            Err(other) => return Err(protocol_error(other)),
        };
        db.conn().execute(
            "UPDATE folders SET total_count = ?3, unread_count = ?4
             WHERE account_id = ?1 AND path = ?2",
            params![
                account_id,
                folder.path,
                status.exists,
                status.unseen.unwrap_or(0)
            ],
        )?;
    }
    Ok(())
}

/// Local changes go out first, so the flag sync that follows does not undo them.
async fn push_before_sync(
    db: &Database,
    session: &mut ImapSession,
    account_id: &str,
) -> Result<()> {
    match crate::mail::push::push_actions_in(db, session, account_id).await {
        // Refused changes already have the server's flags back; the sync can go on.
        Ok(_) | Err(Error::ActionRefused) => Ok(()),
        Err(error) => Err(error),
    }
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
    // What the sync keeps, without mail a search on the server fetched from further back.
    let synced = synced_uids(&db.conn(), &folder.id)?;

    let mut result = SyncReport::default();
    // Mail above this arrived since the last sync; none on the first one.
    let known_until = if mode == Mode::Newest {
        synced.last().copied()
    } else {
        None
    };
    let wanted: BTreeSet<u32> = match mode {
        // The folder's newest, everything from the account's period, and what arrived since the
        // last sync whatever its date (mail moved here keeps its old one).
        Mode::Newest => {
            let mut wanted: BTreeSet<u32> = newest(&server_uids, None).into_iter().collect();
            if let Some(&last) = synced.last() {
                wanted.extend(server_uids.range(last + 1..));
            }
            let since = keep_since(&db.conn(), &folder.account_id)?;
            match since {
                Some(since) => {
                    let recent = with_timeout(
                        COMMAND_TIMEOUT,
                        session.uid_search(format!("SINCE {since}")),
                    )
                    .await?
                    .map_err(protocol_error)?;
                    wanted.extend(recent.into_iter().filter(|uid| server_uids.contains(uid)));
                }
                None => wanted.extend(&server_uids),
            }
            wanted
        }
        Mode::Older => newest(&server_uids, synced.first().copied())
            .into_iter()
            .collect(),
    };
    adopt(
        &db.conn(),
        &folder.id,
        wanted
            .iter()
            .filter(|uid| cached.contains_key(uid) && !synced.contains(uid)),
    )?;
    // Newest first, so a long first sync fills the list from the top.
    let to_fetch: Vec<u32> = wanted
        .iter()
        .rev()
        .filter(|uid| !cached.contains_key(uid))
        .copied()
        .collect();

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
        result.added += fetch_and_store(db, session, folder, chunk, false).await?;
        report(
            progress,
            &folder.account_id,
            folder_id,
            SyncStage::Fetching,
            result.added,
            total,
        );
    }

    if let Some(known_until) = known_until.filter(|_| result.added > 0) {
        result.new_mail = new_mail(&db.conn(), &folder.id, known_until)?;
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
    let (settings, password) = credentials(db, secrets, &target.account_id).await?;
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
    let mut conn = db.conn();
    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE messages SET body_text = ?2, body_html = ?3, snippet = ?4, has_attachments = ?5,
           body_state = 'full'
         WHERE id = ?1",
        params![
            message_id,
            message.body_text,
            message.body_html,
            message.snippet,
            message.has_attachments
        ],
    )?;
    crate::attachments::store(
        &tx,
        message_id,
        &message.inline_images,
        &message.attachments,
    )?;
    tx.commit()?;
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

pub(crate) async fn fetch_all(
    session: &mut ImapSession,
    uids: &[u32],
    query: &str,
) -> Result<Vec<Fetch>> {
    fetch_within(session, uids, query, COMMAND_TIMEOUT * 2).await
}

/// A large message with attachments can take minutes on a slow connection.
pub(crate) const BODY_TIMEOUT: Duration = Duration::from_secs(300);

pub(crate) async fn fetch_within(
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

pub(crate) struct Meta {
    pub(crate) is_read: bool,
    pub(crate) is_flagged: bool,
    date_received: Option<i64>,
    size: u32,
}

pub(crate) fn meta_of(fetch: &Fetch) -> Meta {
    let flags: Vec<Flag<'_>> = fetch.flags().collect();
    Meta {
        is_read: flags.contains(&Flag::Seen),
        is_flagged: flags.contains(&Flag::Flagged),
        date_received: fetch.internal_date().map(|date| date.timestamp_millis()),
        size: fetch.size.unwrap_or(0),
    }
}

/// `by_search`: fetched for a search on the server, not by the sync (`messages.by_search`).
pub(crate) async fn fetch_and_store(
    db: &Database,
    session: &mut ImapSession,
    folder: &FolderState,
    uids: &[u32],
    by_search: bool,
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
        store_message(&tx, folder, *uid, message, *complete, meta, by_search)?;
        stored += 1;
    }
    crate::threads::assign_pending(&tx)?;
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
    by_search: bool,
) -> Result<()> {
    let to_json = |addresses: &[crate::model::Address]| serde_json::to_string(addresses).ok();
    // On a conflict the row keeps its id, which RETURNING hands back for the inline images.
    let id: String = conn.query_row(
        "INSERT INTO messages (id, account_id, folder_id, uid, message_id, in_reply_to,
           references_header, subject, from_name, from_address, to_addresses, cc_addresses,
           date_sent, date_received, size_bytes, snippet, is_read, is_flagged, has_attachments,
           body_state, body_text, body_html, fetched_at, by_search, reply_to)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
           ?19, ?20, ?21, ?22, ?23, ?24, ?25)
         ON CONFLICT (folder_id, uid) DO UPDATE
           SET is_read = excluded.is_read, is_flagged = excluded.is_flagged,
             by_search = min(by_search, excluded.by_search)
         RETURNING id",
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
            message.body_html,
            now_millis(),
            by_search,
            (!message.reply_to.is_empty())
                .then(|| to_json(&message.reply_to))
                .flatten(),
        ],
        |row| row.get(0),
    )?;
    if complete {
        crate::attachments::store(conn, &id, &message.inline_images, &message.attachments)?;
    }
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
            // A change the user made meanwhile is still on its way to the server: it wins.
            "UPDATE messages SET is_read = ?3, is_flagged = ?4
             WHERE folder_id = ?1 AND uid = ?2
               AND id NOT IN (SELECT message_id FROM pending_actions)",
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

fn new_mail(conn: &Connection, folder_id: &str, above_uid: u32) -> Result<Vec<NewMail>> {
    let mut statement = conn.prepare(
        "SELECT id, account_id, from_name, from_address, subject FROM messages
         WHERE folder_id = ?1 AND uid > ?2 AND is_read = 0 AND leaving = 0
         ORDER BY uid",
    )?;
    let mail = statement
        .query_map(params![folder_id, above_uid], |row| {
            Ok(NewMail {
                id: row.get(0)?,
                account_id: row.get(1)?,
                from: Address {
                    name: row.get(2)?,
                    address: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                },
                subject: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(mail)
}

/// The folder's mail the sync keeps, without what only a search on the server fetched.
fn synced_uids(conn: &Connection, folder_id: &str) -> Result<BTreeSet<u32>> {
    let mut statement =
        conn.prepare("SELECT uid FROM messages WHERE folder_id = ?1 AND by_search = 0")?;
    let uids = statement
        .query_map([folder_id], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(uids)
}

/// Mail a search on the server fetched that the sync keeps now: it shows in its folder.
fn adopt<'a>(
    conn: &Connection,
    folder_id: &str,
    uids: impl Iterator<Item = &'a u32>,
) -> Result<()> {
    let mut statement =
        conn.prepare("UPDATE messages SET by_search = 0 WHERE folder_id = ?1 AND uid = ?2")?;
    for uid in uids {
        statement.execute(params![folder_id, uid])?;
    }
    Ok(())
}

/// The start of the account's period as an IMAP date ("29-Jul-2026"); `None` keeps all mail.
fn keep_since(conn: &Connection, account_id: &str) -> Result<Option<String>> {
    let days: Option<i64> = conn
        .query_row(
            "SELECT keep_days FROM accounts WHERE id = ?1",
            [account_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(Error::AccountNotFound)?;
    Ok(days.map(|days| imap_date(chrono::Utc::now() - chrono::TimeDelta::days(days))))
}

/// A day as IMAP SEARCH writes it: "29-Oct-2026".
pub(crate) fn imap_date(time: chrono::DateTime<chrono::Utc>) -> String {
    time.format("%-d-%b-%Y").to_string()
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

/// Brings a folder that has been synced before up to date within an open session, e.g. once mail
/// was moved there. A folder never opened is left to its first sync, which fetches the newest
/// mail and with it what just arrived.
pub(crate) async fn refresh_folder_in(
    db: &Database,
    session: &mut ImapSession,
    folder_id: &str,
) -> Result<()> {
    let Some(folder) = folder_state(&db.conn(), folder_id)? else {
        return Ok(());
    };
    if folder.uid_validity.is_none() {
        return Ok(());
    }
    sync_folder_in(db, session, &folder, Mode::Newest, &|_| {}).await?;
    Ok(())
}

pub(crate) fn folder_state(conn: &Connection, folder_id: &str) -> Result<Option<FolderState>> {
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

pub(crate) async fn credentials(
    db: &Database,
    secrets: &dyn SecretStore,
    account_id: &str,
) -> Result<(ImapSettings, Credential)> {
    let AccountSettings { sign_in, imap, .. } = accounts::settings(&db.conn(), account_id)?;
    let credential = match sign_in {
        SignInMethod::Microsoft => {
            Credential::AccessToken(crate::oauth::access_token(secrets, account_id).await?)
        }
        SignInMethod::Password => {
            Credential::Password(secrets.get(account_id, SecretKind::ImapPassword)?)
        }
    };
    Ok((imap, credential))
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
    let (settings, password) = credentials(db, secrets, account_id).await?;
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
    let (settings, password) = credentials(db, secrets, &folder.account_id).await?;
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
    let (settings, password) = credentials(db, secrets, &target.account_id).await?;
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
    use crate::mail::settings::Security;
    use crate::mail::test_server::{self, Mailbox, Mode, PASSWORD, TestMessage, TestServer};
    use crate::mailbox::{UNIFIED_INBOX, list_folders, list_messages};
    use crate::model::FolderRole;
    use crate::secrets::{MemoryStore, Secret};

    const TEST_VIEW: crate::mail::sanitize::HtmlView<'static> = crate::mail::sanitize::HtmlView {
        asset_base: "mail-asset://localhost",
        remote_images: false,
    };

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
            crate::accounts::Login::Password(&Secret::new(PASSWORD)),
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

        fn id_of(&self, uid: u32) -> String {
            self.db
                .conn()
                .query_row("SELECT id FROM messages WHERE uid = ?1", [uid], |row| {
                    row.get(0)
                })
                .unwrap()
        }

        fn cached_state(&self, uid: u32) -> (bool, bool) {
            cached_flags(&self.db.conn(), &self.inbox_id).unwrap()[&uid]
        }

        fn server_state(&self, uid: u32) -> (bool, bool) {
            let mailbox = self.server.mailbox.lock().unwrap();
            let message = mailbox.messages.iter().find(|m| m.uid == uid).unwrap();
            (message.seen, message.flagged)
        }

        fn queued(&self) -> u32 {
            self.db
                .conn()
                .query_row("SELECT count(*) FROM pending_actions", [], |row| row.get(0))
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
        // Filling the cache is no news.
        assert!(report.new_mail.is_empty());
        assert_eq!(setup.cached_uids(), (21..=120).collect::<Vec<_>>());
        let rows = list_messages(&setup.db.conn(), UNIFIED_INBOX, Default::default())
            .unwrap()
            .rows;
        let newest = &rows[0].latest;
        assert_eq!(newest.subject.as_deref(), Some("Message 120"));
        assert_eq!(newest.from.name.as_deref(), Some("Lena Fischer"));
        assert_eq!(newest.snippet, "Body of message 120");

        let folders = list_folders(&setup.db.conn()).unwrap();
        let inbox = folders.iter().find(|f| f.id == setup.inbox_id).unwrap();
        assert_eq!((inbox.total_count, inbox.unread_count), (120, 120));
        // The sync also refreshed the folder list from the server (Shared is \Noselect).
        let mut paths: Vec<&str> = folders.iter().map(|f| f.path.as_str()).collect();
        paths.sort();
        assert_eq!(paths, ["Entw&APw-rfe", "INBOX", "Sent"]);
        // The other folders got their numbers from STATUS without being opened.
        let sent = folders.iter().find(|f| f.path == "Sent").unwrap();
        assert_eq!((sent.total_count, sent.unread_count), (120, 120));
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

        assert_eq!((report.added, report.updated, report.removed), (1, 1, 1));
        // What arrived is announced, for a notification.
        assert_eq!(report.new_mail.len(), 1);
        assert_eq!(report.new_mail[0].subject.as_deref(), Some("Message 6"));
        assert_eq!(report.new_mail[0].from.address, "lena@example.org");
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
    async fn the_account_period_is_kept_beyond_the_newest_batch() {
        let setup = setup(0).await;
        setup.server.mailbox.lock().unwrap().messages = (1..=150)
            .map(|uid| {
                let message = TestMessage::new(uid);
                if uid > 30 {
                    message.arrived_days_ago(10)
                } else {
                    message
                }
            })
            .collect();

        // The default of 90 days: all recent mail, more than the newest 100, newest first.
        let report = setup.sync().await;
        assert_eq!(report.added, 120);
        assert_eq!(setup.cached_uids(), (31..=150).collect::<Vec<_>>());
        let fetches: Vec<String> = setup
            .server
            .commands()
            .into_iter()
            .filter(|c| c.contains("BODY.PEEK"))
            .collect();
        assert!(fetches[0].contains("150"), "{fetches:?}");

        // Everything: the rest comes, and it is no news.
        let set_days = |days: Option<u32>| {
            setup
                .db
                .conn()
                .execute("UPDATE accounts SET keep_days = ?1", [days])
                .unwrap();
        };
        set_days(None);
        let report = setup.sync().await;
        assert_eq!(report.added, 30);
        assert!(report.new_mail.is_empty());

        // A shorter period fetches nothing more and keeps what is there.
        set_days(Some(1));
        assert_eq!(setup.sync().await.added, 0);
        assert_eq!(setup.cached_uids().len(), 150);
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
            !crate::mailbox::get_message(&setup.db.conn(), &id, TEST_VIEW)
                .unwrap()
                .unwrap()
                .message
                .body_complete
        );

        load_body_with_connector(&setup.db, &setup.secrets, &id, &setup.server.connector)
            .await
            .unwrap();

        let message = crate::mailbox::get_message(&setup.db.conn(), &id, TEST_VIEW)
            .unwrap()
            .unwrap()
            .message;
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

    #[tokio::test]
    async fn html_and_inline_images_are_cached() {
        let setup = setup(0).await;
        {
            let mut html = TestMessage::new(1);
            html.raw = b"From: news@example.org\r\nSubject: News\r\n\
Content-Type: multipart/related; boundary=\"r\"\r\n\r\n\
--r\r\nContent-Type: text/html; charset=utf-8\r\n\r\n\
<p>Gr\xc3\xbc\xc3\x9fe <img src=\"cid:logo@example.org\"></p>\r\n\
--r\r\nContent-Type: image/png\r\nContent-ID: <logo@example.org>\r\n\
Content-Transfer-Encoding: base64\r\n\r\niVBORw0KGgo=\r\n--r--\r\n"
                .to_vec();
            setup.server.mailbox.lock().unwrap().messages.push(html);
        }
        setup.sync().await;

        let conn = setup.db.conn();
        let id: String = conn
            .query_row("SELECT id FROM messages", [], |row| row.get(0))
            .unwrap();
        let message = crate::mailbox::get_message(&conn, &id, TEST_VIEW)
            .unwrap()
            .unwrap()
            .message;
        let html = message.body_html.unwrap();
        assert!(html.contains("Grüße"), "{html}");
        assert!(
            html.contains(&format!(
                "mail-asset://localhost/cid/{id}/logo%40example%2Eorg"
            )),
            "{html}"
        );
        let (mime, data) = crate::mailbox::inline_image(&conn, &id, "logo@example.org")
            .unwrap()
            .unwrap();
        assert_eq!(mime, "image/png");
        assert_eq!(&data[1..4], b"PNG");
    }

    #[tokio::test]
    async fn read_and_flagged_reach_the_server() {
        use crate::actions::{FlagAction, set_flag};
        let setup = setup(3).await;
        setup.sync().await;
        set_flag(
            &setup.db,
            &[setup.id_of(1), setup.id_of(2)],
            FlagAction::Read,
            true,
        )
        .unwrap();
        set_flag(&setup.db, &[setup.id_of(3)], FlagAction::Flagged, true).unwrap();

        let sent = crate::mail::push::push_with_connector(
            &setup.db,
            &setup.secrets,
            &setup.account_id,
            &setup.server.connector,
        )
        .await
        .unwrap();

        assert_eq!(sent, 3);
        assert_eq!(setup.server_state(1), (true, false));
        assert_eq!(setup.server_state(2), (true, false));
        assert_eq!(setup.server_state(3), (false, true));
        assert_eq!(setup.queued(), 0);
        let commands = setup.server.mailbox.lock().unwrap().commands.clone();
        assert!(
            commands
                .iter()
                .any(|c| c == "UID STORE 1:2 +FLAGS.SILENT (\\Seen)"),
            "{commands:?}"
        );
    }

    #[tokio::test]
    async fn an_offline_change_goes_out_with_the_next_sync() {
        use crate::actions::{FlagAction, set_flag};
        let setup = setup(2).await;
        setup.sync().await;
        set_flag(&setup.db, &[setup.id_of(1)], FlagAction::Read, true).unwrap();

        // The flag sync would otherwise put the server's "unread" back.
        setup.sync().await;

        assert_eq!(setup.cached_state(1), (true, false));
        assert_eq!(setup.server_state(1), (true, false));
        assert_eq!(setup.queued(), 0);
    }

    impl Setup {
        /// An archive folder next to the inbox, as SPECIAL-USE would report it.
        fn add_archive(&self) -> String {
            self.db
                .conn()
                .execute(
                    "INSERT INTO folders (id, account_id, path, name, role)
                     VALUES ('archive', ?1, 'Archive', 'Archive', 'archive')",
                    [&self.account_id],
                )
                .unwrap();
            "archive".into()
        }

        async fn push(&self) -> Result<u32> {
            crate::mail::push::push_with_connector(
                &self.db,
                &self.secrets,
                &self.account_id,
                &self.server.connector,
            )
            .await
        }

        /// As if the undo window had passed.
        fn make_due(&self) {
            self.db
                .conn()
                .execute("UPDATE pending_actions SET due_at = 0", [])
                .unwrap();
        }

        fn server_uids(&self) -> Vec<u32> {
            let mailbox = self.server.mailbox.lock().unwrap();
            mailbox.messages.iter().map(|m| m.uid).collect()
        }

        /// Cached and listed: a message on its way out is cached but not listed.
        fn listed_uids(&self) -> Vec<u32> {
            let listed: Vec<String> =
                list_messages(&self.db.conn(), &self.inbox_id, Default::default())
                    .unwrap()
                    .rows
                    .into_iter()
                    .map(|row| row.latest.id)
                    .collect();
            self.cached_uids()
                .into_iter()
                .filter(|uid| listed.contains(&self.id_of(*uid)))
                .collect()
        }

        fn commands(&self) -> Vec<String> {
            self.server.mailbox.lock().unwrap().commands.clone()
        }
    }

    #[tokio::test]
    async fn a_move_waits_for_the_undo_window_then_moves_on_the_server() {
        let setup = setup(3).await;
        setup.sync().await;
        let archive = setup.add_archive();
        crate::moves::move_messages(&setup.db, &[setup.id_of(1)], &archive).unwrap();
        assert_eq!(setup.listed_uids(), [2, 3]);

        // Not due yet: still on the server, still undoable.
        assert_eq!(setup.push().await.unwrap(), 0);
        assert_eq!(setup.server_uids(), [1, 2, 3]);

        setup.make_due();
        assert_eq!(setup.push().await.unwrap(), 1);
        assert_eq!(setup.server_uids(), [2, 3]);
        assert!(
            setup
                .commands()
                .iter()
                .any(|c| c == "UID MOVE 1 \"Archive\"")
        );
        assert_eq!(setup.cached_uids(), [2, 3]);
        assert_eq!(setup.queued(), 0);
    }

    #[tokio::test]
    async fn the_target_folder_is_fetched_once_the_mail_is_there() {
        let setup = setup(2).await;
        setup.sync().await;
        let archive = setup.add_archive();
        // Opened before: the move brings it up to date at once.
        setup
            .db
            .conn()
            .execute(
                "UPDATE folders SET uid_validity = 1 WHERE id = ?1",
                [&archive],
            )
            .unwrap();
        crate::moves::move_messages(&setup.db, &[setup.id_of(1)], &archive).unwrap();
        setup.make_due();

        setup.push().await.unwrap();

        let commands = setup.commands();
        let moved = commands.iter().position(|c| c == "UID MOVE 1 \"Archive\"");
        let fetched = commands.iter().position(|c| c == "EXAMINE \"Archive\"");
        assert!(moved.is_some() && fetched > moved, "{commands:?}");
    }

    #[tokio::test]
    async fn without_move_it_copies_and_expunges() {
        let setup = setup(2).await;
        setup.server.mailbox.lock().unwrap().without_move = true;
        setup.sync().await;
        let archive = setup.add_archive();
        crate::moves::move_messages(&setup.db, &[setup.id_of(2)], &archive).unwrap();
        setup.make_due();

        assert_eq!(setup.push().await.unwrap(), 1);

        assert_eq!(setup.server_uids(), [1]);
        let commands = setup.commands();
        for expected in [
            "UID COPY 2 \"Archive\"",
            "UID STORE 2 +FLAGS.SILENT (\\Deleted)",
            "EXPUNGE ",
        ] {
            assert!(
                commands.iter().any(|c| c == expected),
                "{expected}: {commands:?}"
            );
        }
    }

    #[tokio::test]
    async fn deleting_for_good_goes_out_at_once() {
        let setup = setup(2).await;
        setup.sync().await;
        // This account has no trash: deleting is for good.
        let outcome = crate::moves::delete_messages(&setup.db, &[setup.id_of(2)]).unwrap();
        assert_eq!(outcome.deleted.len(), 1);

        assert_eq!(setup.push().await.unwrap(), 1);

        assert_eq!(setup.server_uids(), [1]);
        assert!(setup.commands().iter().any(|c| c == "UID EXPUNGE 2"));
        assert_eq!(setup.cached_uids(), [1]);
    }

    #[tokio::test]
    async fn a_refused_move_shows_the_message_again() {
        let setup = setup(2).await;
        setup.server.mailbox.lock().unwrap().refuse_move = true;
        setup.sync().await;
        let archive = setup.add_archive();
        crate::moves::move_messages(&setup.db, &[setup.id_of(1)], &archive).unwrap();
        setup.make_due();

        assert!(matches!(setup.push().await, Err(Error::ActionRefused)));

        assert_eq!(setup.listed_uids(), [1, 2]);
        assert_eq!(setup.server_uids(), [1, 2]);
        assert_eq!(setup.queued(), 0);
    }

    #[tokio::test]
    async fn an_account_without_archive_gets_one() {
        let setup = setup(2).await;
        setup.sync().await;
        let connector = setup.server.connector.clone();

        let outcome = crate::moves::archive_with(
            &setup.db,
            &setup.secrets,
            &[setup.id_of(1)],
            "Archiv",
            || Ok(connector),
        )
        .await
        .unwrap();

        assert_eq!(outcome.moved, [setup.id_of(1)]);
        assert!(setup.commands().iter().any(|c| c == "CREATE \"Archiv\""));
        let archive = list_folders(&setup.db.conn())
            .unwrap()
            .into_iter()
            .find(|folder| folder.role == Some(FolderRole::Archive))
            .unwrap();
        assert_eq!(archive.path, "Archiv");
    }

    #[tokio::test]
    async fn a_refused_change_is_undone() {
        use crate::actions::{FlagAction, set_flag};
        let setup = setup(1).await;
        setup.sync().await;
        setup.server.mailbox.lock().unwrap().refuse_store = true;
        set_flag(&setup.db, &[setup.id_of(1)], FlagAction::Flagged, true).unwrap();
        assert_eq!(setup.cached_state(1), (false, true));

        let result = crate::mail::push::push_with_connector(
            &setup.db,
            &setup.secrets,
            &setup.account_id,
            &setup.server.connector,
        )
        .await;

        assert!(matches!(result, Err(Error::ActionRefused)), "{result:?}");
        assert_eq!(setup.cached_state(1), (false, false));
        assert_eq!(setup.queued(), 0);
    }

    #[tokio::test]
    async fn a_change_made_during_a_sync_is_kept() {
        let setup = setup(1).await;
        setup.sync().await;
        let id = setup.id_of(1);
        // As if the user marked it read while the sync was fetching flags.
        setup
            .db
            .conn()
            .execute_batch(&format!(
                "UPDATE messages SET is_read = 1 WHERE id = '{id}';
                 INSERT INTO pending_actions (account_id, message_id, action, payload, created_at)
                 SELECT account_id, id, 'set_read', '{{\"value\":true}}', 0 FROM messages WHERE id = '{id}';"
            ))
            .unwrap();
        let cached = cached_flags(&setup.db.conn(), &setup.inbox_id).unwrap();
        let mut cached_before = cached.clone();
        cached_before.insert(1, (false, false));
        let (settings, password) = credentials(&setup.db, &setup.secrets, &setup.account_id)
            .await
            .unwrap();
        let mut session = imap::open_session(&settings, &password, &setup.server.connector)
            .await
            .unwrap();
        session.examine("INBOX").await.unwrap();
        // The server still says unread, but the queued change wins.
        update_flags(
            &setup.db,
            &mut session,
            &setup.inbox_id,
            &cached_before,
            &[1],
        )
        .await
        .unwrap();
        assert_eq!(setup.cached_state(1), (true, false));
    }

    #[tokio::test]
    async fn attachments_are_listed_and_fetched_on_demand() {
        let setup = setup(0).await;
        {
            let mut mail = TestMessage::new(1);
            mail.raw = b"From: a@example.org\r\nSubject: Rechnung\r\n\
Content-Type: multipart/mixed; boundary=\"m\"\r\n\r\n\
--m\r\nContent-Type: text/plain\r\n\r\nAnbei die Rechnung.\r\n\
--m\r\nContent-Type: application/pdf\r\nContent-Disposition: attachment; filename=\"Rechnung.pdf\"\r\n\
Content-Transfer-Encoding: base64\r\n\r\nJVBERi0xLjQ=\r\n--m--\r\n"
                .to_vec();
            setup.server.mailbox.lock().unwrap().messages.push(mail);
        }
        setup.sync().await;
        let id = setup.id_of(1);
        let attachments = crate::attachments::list(&setup.db.conn(), &id).unwrap();
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].file_name, "Rechnung.pdf");
        assert_eq!(attachments[0].size, 8);

        let data = crate::mail::download::fetch_with_connector(
            &setup.db,
            &setup.secrets,
            &attachments[0].id,
            &setup.server.connector,
        )
        .await
        .unwrap();
        assert_eq!(data, b"%PDF-1.4");
        // Fetching an attachment never marks the mail read.
        let commands = setup.server.mailbox.lock().unwrap().commands.clone();
        assert!(
            !commands
                .iter()
                .any(|c| c.starts_with("SELECT") || c.contains("STORE"))
        );
    }
}
