//! Sends the queued read and flagged changes (see `actions`) to the server. Each sync runs this
//! first, so a change made offline goes out with the next connection.

use std::collections::BTreeMap;

use rusqlite::{Connection, params};

use crate::actions::FlagAction;
use crate::db::Database;
use crate::mail::imap::{self, COMMAND_TIMEOUT, ImapSession, protocol_error, with_timeout};
use crate::mail::sync::{credentials, fetch_all, meta_of, uid_set};
use crate::secrets::SecretStore;
use crate::{Error, Result};

struct Pending {
    id: i64,
    uid: u32,
    action: FlagAction,
    value: bool,
}

#[derive(Default)]
struct Group {
    uids: Vec<u32>,
    ids: Vec<i64>,
}

struct FolderQueue {
    path: String,
    uid_validity: Option<u32>,
    actions: Vec<Pending>,
}

/// Sends the account's queue; returns how many changes reached the server.
pub async fn push_actions(
    db: &Database,
    secrets: &dyn SecretStore,
    account_id: &str,
) -> Result<u32> {
    if queue(&db.conn(), account_id)?.is_empty() {
        return Ok(0);
    }
    let (settings, password) = credentials(db, secrets, account_id)?;
    let connector = imap::platform_connector()?;
    let mut session = imap::open_session(&settings, &password, &connector).await?;
    let result = push_actions_in(db, &mut session, account_id).await;
    let _ = with_timeout(COMMAND_TIMEOUT, session.logout()).await;
    result
}

/// Within an open session. Connection trouble leaves the queue as it is, for the next try. A
/// change the server refuses is dropped and the message gets the server's flags back.
pub(crate) async fn push_actions_in(
    db: &Database,
    session: &mut ImapSession,
    account_id: &str,
) -> Result<u32> {
    let folders = queue(&db.conn(), account_id)?;
    let mut sent = 0;
    let mut refused = false;
    for (folder_id, folder) in folders {
        // STORE needs the folder opened for writing; the sync itself only ever uses EXAMINE.
        let ids: Vec<i64> = folder.actions.iter().map(|action| action.id).collect();
        let mailbox = match with_timeout(COMMAND_TIMEOUT, session.select(&folder.path)).await? {
            Ok(mailbox) => mailbox,
            // Gone or not writable: these changes can never be made.
            Err(async_imap::error::Error::No(reason) | async_imap::error::Error::Bad(reason)) => {
                tracing::warn!(%reason, "server refused to open a folder for flag changes");
                refused = true;
                delete(&db.conn(), &ids)?;
                continue;
            }
            Err(other) => return Err(protocol_error(other)),
        };
        if folder.uid_validity.is_some() && mailbox.uid_validity != folder.uid_validity {
            // The UIDs mean other messages now; the next sync rebuilds the folder.
            delete(&db.conn(), &ids)?;
            continue;
        }

        // One STORE per flag and value, for all messages of the folder at once.
        let mut groups: BTreeMap<(&str, bool), Group> = BTreeMap::new();
        for action in &folder.actions {
            let group = groups
                .entry((flag_name(action.action), action.value))
                .or_default();
            group.uids.push(action.uid);
            group.ids.push(action.id);
        }
        for ((flag, value), Group { uids, ids }) in groups {
            let query = format!("{}FLAGS.SILENT ({flag})", if value { '+' } else { '-' });
            match store(session, &uid_set(&uids), &query).await? {
                Ok(()) => sent += u32::try_from(ids.len()).unwrap_or(u32::MAX),
                Err(reason) => {
                    tracing::warn!(%reason, "server refused a flag change");
                    refused = true;
                    restore_flags(db, session, &folder_id, &uids).await?;
                }
            }
            delete(&db.conn(), &ids)?;
        }
    }
    if refused {
        Err(Error::ActionRefused)
    } else {
        Ok(sent)
    }
}

/// The outer result is connection trouble, the inner one the server's answer. Sent as a plain
/// command: async-imap's `uid_store` ignores whether the server answered OK or NO.
async fn store(
    session: &mut ImapSession,
    set: &str,
    query: &str,
) -> Result<std::result::Result<(), String>> {
    let command = format!("UID STORE {set} {query}");
    match with_timeout(COMMAND_TIMEOUT, session.run_command_and_check_ok(&command)).await? {
        Ok(()) => Ok(Ok(())),
        Err(async_imap::error::Error::No(reason) | async_imap::error::Error::Bad(reason)) => {
            Ok(Err(reason))
        }
        Err(other) => Err(protocol_error(other)),
    }
}

/// Undoes refused changes in the cache with what the server has.
async fn restore_flags(
    db: &Database,
    session: &mut ImapSession,
    folder_id: &str,
    uids: &[u32],
) -> Result<()> {
    let fetches = fetch_all(session, uids, "(UID FLAGS)").await?;
    let conn = db.conn();
    for fetch in &fetches {
        let Some(uid) = fetch.uid else { continue };
        let meta = meta_of(fetch);
        conn.execute(
            "UPDATE messages SET is_read = ?3, is_flagged = ?4 WHERE folder_id = ?1 AND uid = ?2",
            params![folder_id, uid, meta.is_read, meta.is_flagged],
        )?;
    }
    Ok(())
}

fn flag_name(action: FlagAction) -> &'static str {
    match action {
        FlagAction::Read => "\\Seen",
        FlagAction::Flagged => "\\Flagged",
    }
}

fn queue(conn: &Connection, account_id: &str) -> Result<BTreeMap<String, FolderQueue>> {
    let mut statement = conn.prepare(
        "SELECT p.id, f.id, f.path, f.uid_validity, m.uid, p.action, p.payload
         FROM pending_actions p
         JOIN messages m ON m.id = p.message_id
         JOIN folders f ON f.id = m.folder_id
         WHERE p.account_id = ?1 AND p.action IN ('set_read', 'set_flagged')
         ORDER BY p.id",
    )?;
    let rows = statement.query_map([account_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<u32>>(3)?,
            row.get::<_, u32>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<String>>(6)?,
        ))
    })?;
    let mut folders: BTreeMap<String, FolderQueue> = BTreeMap::new();
    for row in rows {
        let (id, folder_id, path, uid_validity, uid, action, payload) = row?;
        let Some(action) = FlagAction::parse(&action) else {
            continue;
        };
        let value = payload
            .and_then(|payload| serde_json::from_str::<serde_json::Value>(&payload).ok())
            .and_then(|payload| payload.get("value")?.as_bool())
            .unwrap_or(true);
        folders
            .entry(folder_id)
            .or_insert_with(|| FolderQueue {
                path,
                uid_validity,
                actions: Vec::new(),
            })
            .actions
            .push(Pending {
                id,
                uid,
                action,
                value,
            });
    }
    Ok(folders)
}

fn delete(conn: &Connection, ids: &[i64]) -> Result<()> {
    let mut statement = conn.prepare_cached("DELETE FROM pending_actions WHERE id = ?1")?;
    for id in ids {
        statement.execute([id])?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) async fn push_with_connector(
    db: &Database,
    secrets: &dyn SecretStore,
    account_id: &str,
    connector: &tokio_rustls::TlsConnector,
) -> Result<u32> {
    let (settings, password) = credentials(db, secrets, account_id)?;
    let mut session = imap::open_session(&settings, &password, connector).await?;
    let result = push_actions_in(db, &mut session, account_id).await;
    let _ = session.logout().await;
    result
}
