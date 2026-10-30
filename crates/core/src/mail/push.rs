//! Sends the queued changes to the server: read and flagged state (`actions`) and moves that are
//! due (`moves`). Each sync runs this first, so a change made offline goes out with the next
//! connection.

use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension, params};

use crate::actions::FlagAction;
use crate::db::Database;
use crate::ids::now_millis;
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
    if !has_due(&db.conn(), account_id)? {
        return Ok(0);
    }
    let (settings, password) = credentials(db, secrets, account_id).await?;
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
    // After the flags, so a message marked read and then archived arrives read.
    let (moved, moves_refused) = push_moves(db, session, account_id).await?;
    sent += moved;
    if refused || moves_refused {
        Err(Error::ActionRefused)
    } else {
        Ok(sent)
    }
}

/// A move or deletion that is due, by source folder.
struct Move {
    message_id: String,
    uid: u32,
    /// None: delete for good. Some(None): the target folder is gone.
    target: Option<Option<String>>,
    target_id: Option<String>,
}

struct MoveFolder {
    path: String,
    uid_validity: Option<u32>,
    moves: Vec<Move>,
}

/// Returns how many messages reached their place, and whether the server refused any. A refused
/// or impossible move shows the message in its folder again.
async fn push_moves(
    db: &Database,
    session: &mut ImapSession,
    account_id: &str,
) -> Result<(u32, bool)> {
    let folders = due_moves(&db.conn(), account_id)?;
    if folders.is_empty() {
        return Ok((0, false));
    }
    let capabilities = with_timeout(COMMAND_TIMEOUT, session.capabilities())
        .await?
        .map_err(protocol_error)?;
    let can_move = capabilities.has_str("MOVE");
    let uid_expunge = capabilities.has_str("UIDPLUS");

    let mut sent = 0;
    let mut refused = false;
    let mut arrived_in: Vec<String> = Vec::new();
    for folder in folders.into_values() {
        let message_ids = |moves: &[&Move]| -> Vec<String> {
            moves.iter().map(|m| m.message_id.clone()).collect()
        };
        let all: Vec<&Move> = folder.moves.iter().collect();
        let mailbox = match with_timeout(COMMAND_TIMEOUT, session.select(&folder.path)).await? {
            Ok(mailbox) => mailbox,
            Err(async_imap::error::Error::No(reason) | async_imap::error::Error::Bad(reason)) => {
                tracing::warn!(%reason, "server refused to open a folder for moving mail");
                refused = true;
                take_back_all(db, &message_ids(&all))?;
                continue;
            }
            Err(other) => return Err(protocol_error(other)),
        };
        if folder.uid_validity.is_some() && mailbox.uid_validity != folder.uid_validity {
            // The UIDs mean other messages now; the next sync rebuilds the folder.
            take_back_all(db, &message_ids(&all))?;
            continue;
        }

        // One command per target (or for deleting), for all messages of the folder at once.
        let mut groups: BTreeMap<Option<Option<&str>>, Vec<&Move>> = BTreeMap::new();
        for m in &folder.moves {
            let target = m.target.as_ref().map(Option::as_deref);
            groups.entry(target).or_default().push(m);
        }
        for (target, moves) in groups {
            let uids: Vec<u32> = moves.iter().map(|m| m.uid).collect();
            let set = uid_set(&uids);
            let answer = match target {
                Some(None) => Err("the target folder is gone".to_owned()),
                Some(Some(path)) if can_move => {
                    run(session, &format!("UID MOVE {set} {}", quote(path))).await?
                }
                Some(Some(path)) => {
                    match run(session, &format!("UID COPY {set} {}", quote(path))).await? {
                        Ok(()) => expunge(session, &set, uid_expunge).await?,
                        Err(reason) => Err(reason),
                    }
                }
                None => expunge(session, &set, uid_expunge).await?,
            };
            match answer {
                Ok(()) => {
                    // Gone from this folder; the target's next sync brings the new copy.
                    let conn = db.conn();
                    for m in &moves {
                        conn.execute("DELETE FROM messages WHERE id = ?1", [&m.message_id])?;
                    }
                    sent += u32::try_from(moves.len()).unwrap_or(u32::MAX);
                    for target in moves.iter().filter_map(|m| m.target_id.clone()) {
                        if !arrived_in.contains(&target) {
                            arrived_in.push(target);
                        }
                    }
                }
                Err(reason) => {
                    tracing::warn!(%reason, "server refused to move or delete mail");
                    refused = true;
                    take_back_all(db, &message_ids(&moves))?;
                }
            }
        }
    }
    // The moved mail shows in its new folder right away, not only once that is opened again.
    for folder_id in arrived_in {
        if let Err(error) = crate::mail::sync::refresh_folder_in(db, session, &folder_id).await {
            tracing::warn!(%error, "could not fetch the folder mail was moved to");
        }
    }
    Ok((sent, refused))
}

/// Marks the messages deleted and removes them. Without UIDPLUS only a plain EXPUNGE is left,
/// which also removes what another client marked deleted in this folder.
pub(crate) async fn expunge(
    session: &mut ImapSession,
    set: &str,
    uid_expunge: bool,
) -> Result<std::result::Result<(), String>> {
    if let Err(reason) = run(
        session,
        &format!("UID STORE {set} +FLAGS.SILENT (\\Deleted)"),
    )
    .await?
    {
        return Ok(Err(reason));
    }
    if uid_expunge {
        run(session, &format!("UID EXPUNGE {set}")).await
    } else {
        run(session, "EXPUNGE").await
    }
}

fn take_back_all(db: &Database, message_ids: &[String]) -> Result<()> {
    let mut conn = db.conn();
    let tx = conn.transaction()?;
    for message_id in message_ids {
        if !crate::moves::take_back(&tx, message_id, "move")? {
            crate::moves::take_back(&tx, message_id, "delete")?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// A mailbox name as an IMAP quoted string.
fn quote(path: &str) -> String {
    format!("\"{}\"", path.replace('\\', "\\\\").replace('"', "\\\""))
}

fn due_moves(conn: &Connection, account_id: &str) -> Result<BTreeMap<String, MoveFolder>> {
    let mut statement = conn.prepare(
        "SELECT m.id, m.uid, f.id, f.path, f.uid_validity, p.action, t.path, t.id
         FROM pending_actions p
         JOIN messages m ON m.id = p.message_id
         JOIN folders f ON f.id = m.folder_id
         LEFT JOIN folders t ON t.id = json_extract(p.payload, '$.to')
         WHERE p.account_id = ?1 AND p.action IN ('move', 'delete')
           AND coalesce(p.due_at, 0) <= ?2
         ORDER BY p.id",
    )?;
    let rows = statement.query_map(params![account_id, now_millis()], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, u32>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<u32>>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, Option<String>>(7)?,
        ))
    })?;
    let mut folders: BTreeMap<String, MoveFolder> = BTreeMap::new();
    for row in rows {
        let (message_id, uid, folder_id, path, uid_validity, action, target, target_id) = row?;
        folders
            .entry(folder_id)
            .or_insert_with(|| MoveFolder {
                path,
                uid_validity,
                moves: Vec::new(),
            })
            .moves
            .push(Move {
                message_id,
                uid,
                target: (action == "move").then_some(target),
                target_id,
            });
    }
    Ok(folders)
}

/// Read and flagged changes, and moves that no longer wait for an undo.
fn has_due(conn: &Connection, account_id: &str) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM pending_actions
             WHERE account_id = ?1
               AND (action IN ('set_read', 'set_flagged') OR coalesce(due_at, 0) <= ?2)
             LIMIT 1",
            params![account_id, now_millis()],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

async fn store(
    session: &mut ImapSession,
    set: &str,
    query: &str,
) -> Result<std::result::Result<(), String>> {
    run(session, &format!("UID STORE {set} {query}")).await
}

/// The outer result is connection trouble, the inner one the server's answer. Sent as plain
/// commands: async-imap's `uid_store` and `uid_mv` ignore whether the server answered OK or NO.
async fn run(session: &mut ImapSession, command: &str) -> Result<std::result::Result<(), String>> {
    match with_timeout(COMMAND_TIMEOUT, session.run_command_and_check_ok(command)).await? {
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
    let (settings, password) = credentials(db, secrets, account_id).await?;
    let mut session = imap::open_session(&settings, &password, connector).await?;
    let result = push_actions_in(db, &mut session, account_id).await;
    let _ = session.logout().await;
    result
}
