//! Archiving, deleting and moving mail. Like read and flagged state (`actions`), a move changes
//! the cache at once and is queued for the server (`mail::push`). The message stays in the
//! cache, marked as leaving and no longer listed, until the server has moved it; the target
//! folder's next sync brings it in under its new UID. Moves wait `UNDO_WINDOW_MS` before they go
//! out, so they can still be undone; deleting for good goes out at once.

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use tokio_rustls::TlsConnector;
use ts_rs::TS;

use crate::db::Database;
use crate::ids::now_millis;
use crate::mail::imap::{self, COMMAND_TIMEOUT, ImapSession, list_folders, with_timeout};
use crate::mail::sync::credentials;
use crate::model::FolderRole;
use crate::secrets::SecretStore;
use crate::{Error, Result};

/// How long a move can still be undone before it goes to the server.
pub const UNDO_WINDOW_MS: i64 = 10_000;

/// What happened to the requested messages.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MoveOutcome {
    /// Moved to another folder; they can be taken back for a moment.
    pub moved: Vec<String>,
    /// Deleted for good: they were in the trash already, or the account has none.
    pub deleted: Vec<String>,
    /// Accounts whose queue got new entries.
    pub accounts: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Destination<'a> {
    Folder(&'a str),
    /// The archive folder, or Gmail's "All Mail".
    Archive,
    /// The trash; from the trash, deleted for good.
    Trash,
}

/// Moves into a folder of the same account; messages of other accounts are left alone.
pub fn move_messages(
    db: &Database,
    message_ids: &[String],
    folder_id: &str,
) -> Result<MoveOutcome> {
    queue(db, message_ids, Destination::Folder(folder_id))
}

/// Into the trash, and out of it for good. The interface asks before it deletes for good.
pub fn delete_messages(db: &Database, message_ids: &[String]) -> Result<MoveOutcome> {
    queue(db, message_ids, Destination::Trash)
}

/// Into the archive. An account with neither an archive folder nor Gmail's "All Mail" gets a
/// folder named `folder_name` on the server first (in the interface language, "Archiv").
pub async fn archive_messages(
    db: &Database,
    secrets: &dyn SecretStore,
    message_ids: &[String],
    folder_name: &str,
) -> Result<MoveOutcome> {
    archive_with(
        db,
        secrets,
        message_ids,
        folder_name,
        imap::platform_connector,
    )
    .await
}

/// `connector`: only called when a folder has to be made.
pub(crate) async fn archive_with(
    db: &Database,
    secrets: &dyn SecretStore,
    message_ids: &[String],
    folder_name: &str,
    connector: impl FnOnce() -> Result<TlsConnector>,
) -> Result<MoveOutcome> {
    let missing = accounts_without_archive(&db.conn(), message_ids)?;
    if !missing.is_empty() {
        let connector = connector()?;
        for account_id in missing {
            create_archive_folder(db, secrets, &account_id, folder_name, &connector).await?;
        }
    }
    queue(db, message_ids, Destination::Archive)
}

/// Into the archive, only where the account has one already: for callers that cannot make a
/// folder on the server (agents).
pub fn archive_existing(db: &Database, message_ids: &[String]) -> Result<MoveOutcome> {
    if !accounts_without_archive(&db.conn(), message_ids)?.is_empty() {
        return Err(Error::FolderNotFound);
    }
    queue(db, message_ids, Destination::Archive)
}

/// Takes back moves that have not reached the server yet; returns the messages that are back.
pub fn undo_moves(db: &Database, message_ids: &[String]) -> Result<Vec<String>> {
    let mut conn = db.conn();
    let tx = conn.transaction()?;
    let mut restored = Vec::new();
    for message_id in message_ids {
        if take_back(&tx, message_id, "move")? {
            restored.push(message_id.clone());
        }
    }
    tx.commit()?;
    Ok(restored)
}

struct Leaving {
    account_id: String,
    folder_id: String,
    is_read: bool,
}

fn queue(
    db: &Database,
    message_ids: &[String],
    destination: Destination<'_>,
) -> Result<MoveOutcome> {
    let mut conn = db.conn();
    let tx = conn.transaction()?;
    let mut outcome = MoveOutcome::default();
    let now = now_millis();
    for message_id in message_ids {
        let Some(message) = message_of(&tx, message_id)? else {
            continue;
        };
        let target = match destination {
            Destination::Folder(folder_id) => (folder_account(&tx, folder_id)?.as_deref()
                == Some(&message.account_id))
            .then(|| folder_id.to_owned()),
            Destination::Archive => role_folder(&tx, &message.account_id, FolderRole::Archive)?
                .or(role_folder(&tx, &message.account_id, FolderRole::All)?),
            Destination::Trash => role_folder(&tx, &message.account_id, FolderRole::Trash)?,
        };
        match target {
            Some(target) if target != message.folder_id => {
                leave(&tx, message_id, &message)?;
                adjust_counts(&tx, &target, 1, i64::from(!message.is_read))?;
                tx.execute(
                    "INSERT INTO pending_actions
                       (account_id, message_id, action, payload, due_at, created_at)
                     VALUES (?1, ?2, 'move', ?3, ?4, ?5)",
                    params![
                        message.account_id,
                        message_id,
                        serde_json::json!({ "to": target }).to_string(),
                        now + UNDO_WINDOW_MS,
                        now
                    ],
                )?;
                outcome.moved.push(message_id.clone());
            }
            // Already in the trash, or there is none.
            _ if destination == Destination::Trash => {
                leave(&tx, message_id, &message)?;
                tx.execute(
                    "INSERT INTO pending_actions
                       (account_id, message_id, action, due_at, created_at)
                     VALUES (?1, ?2, 'delete', ?3, ?3)",
                    params![message.account_id, message_id, now],
                )?;
                outcome.deleted.push(message_id.clone());
            }
            // Already there, another account's folder, or no archive yet.
            _ => continue,
        }
        if !outcome.accounts.contains(&message.account_id) {
            outcome.accounts.push(message.account_id);
        }
    }
    tx.commit()?;
    Ok(outcome)
}

/// A message that is still listed; one that is already leaving is not moved twice.
fn message_of(conn: &Connection, message_id: &str) -> Result<Option<Leaving>> {
    Ok(conn
        .query_row(
            "SELECT account_id, folder_id, is_read FROM messages WHERE id = ?1 AND leaving = 0",
            [message_id],
            |row| {
                Ok(Leaving {
                    account_id: row.get(0)?,
                    folder_id: row.get(1)?,
                    is_read: row.get(2)?,
                })
            },
        )
        .optional()?)
}

fn folder_account(conn: &Connection, folder_id: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT account_id FROM folders WHERE id = ?1",
            [folder_id],
            |row| row.get(0),
        )
        .optional()?)
}

/// The account's folder with this role, if the server has one.
pub fn role_folder(
    conn: &Connection,
    account_id: &str,
    role: FolderRole,
) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT id FROM folders WHERE account_id = ?1 AND role = ?2",
            params![account_id, role.as_str()],
            |row| row.get(0),
        )
        .optional()?)
}

/// Hidden from the lists; the folder counts follow at once, the server's numbers replace them
/// with the next sync.
fn leave(conn: &Connection, message_id: &str, message: &Leaving) -> Result<()> {
    conn.execute(
        "UPDATE messages SET leaving = 1 WHERE id = ?1",
        [message_id],
    )?;
    adjust_counts(conn, &message.folder_id, -1, -i64::from(!message.is_read))
}

fn adjust_counts(conn: &Connection, folder_id: &str, total: i64, unread: i64) -> Result<()> {
    conn.execute(
        "UPDATE folders SET total_count = max(total_count + ?2, 0),
           unread_count = max(unread_count + ?3, 0)
         WHERE id = ?1",
        params![folder_id, total, unread],
    )?;
    Ok(())
}

/// Shows a leaving message again and drops its queued `action` ("move" or "delete"): on undo,
/// or when the server refused it. False if there was nothing to take back.
pub(crate) fn take_back(conn: &Connection, message_id: &str, action: &str) -> Result<bool> {
    let pending = conn
        .query_row(
            "SELECT p.id, json_extract(p.payload, '$.to'), m.folder_id, m.is_read
             FROM pending_actions p JOIN messages m ON m.id = p.message_id
             WHERE p.message_id = ?1 AND p.action = ?2 AND m.leaving = 1",
            params![message_id, action],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, bool>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((action_id, target, folder_id, is_read)) = pending else {
        return Ok(false);
    };
    conn.execute("DELETE FROM pending_actions WHERE id = ?1", [action_id])?;
    conn.execute(
        "UPDATE messages SET leaving = 0 WHERE id = ?1",
        [message_id],
    )?;
    let unread = i64::from(!is_read);
    adjust_counts(conn, &folder_id, 1, unread)?;
    if let Some(target) = target {
        adjust_counts(conn, &target, -1, -unread)?;
    }
    Ok(true)
}

/// Accounts of these messages that have nowhere to archive to yet.
fn accounts_without_archive(conn: &Connection, message_ids: &[String]) -> Result<Vec<String>> {
    let mut accounts: Vec<String> = Vec::new();
    for message_id in message_ids {
        let account: Option<String> = conn
            .query_row(
                "SELECT account_id FROM messages WHERE id = ?1",
                [message_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(account) = account else { continue };
        if accounts.contains(&account) {
            continue;
        }
        let archive = role_folder(conn, &account, FolderRole::Archive)?.or(role_folder(
            conn,
            &account,
            FolderRole::All,
        )?);
        if archive.is_none() {
            accounts.push(account);
        }
    }
    Ok(accounts)
}

async fn create_archive_folder(
    db: &Database,
    secrets: &dyn SecretStore,
    account_id: &str,
    name: &str,
    connector: &TlsConnector,
) -> Result<()> {
    // Non-ASCII names would need modified UTF-7; "Archive" is recognised everywhere.
    let name = name.trim();
    let name = if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == ' ') {
        name
    } else {
        "Archive"
    };
    let (settings, credential) = credentials(db, secrets, account_id).await?;
    let mut session = imap::open_session(&settings, &credential, connector).await?;
    let result = create_archive_folder_in(db, &mut session, account_id, name).await;
    let _ = with_timeout(COMMAND_TIMEOUT, session.logout()).await;
    result
}

/// Creates the folder (some servers only allow folders below INBOX) and reads the folder list
/// again; the name makes it the archive (`mail::folders`).
async fn create_archive_folder_in(
    db: &Database,
    session: &mut ImapSession,
    account_id: &str,
    name: &str,
) -> Result<()> {
    let delimiter: Option<String> = db
        .conn()
        .query_row(
            "SELECT delimiter FROM folders WHERE account_id = ?1 AND role = 'inbox'",
            [account_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    let below_inbox = delimiter.map(|delimiter| format!("INBOX{delimiter}{name}"));
    for path in std::iter::once(name.to_owned()).chain(below_inbox) {
        // NO: it exists already, or not allowed here; the listing tells.
        let created = with_timeout(COMMAND_TIMEOUT, session.create(&path)).await?;
        if let Err(error) = created {
            tracing::info!(%error, "server did not create the archive folder");
        }
        let folders = with_timeout(COMMAND_TIMEOUT, list_folders(session)).await??;
        crate::accounts::replace_folders(&db.conn(), account_id, &folders)?;
        if role_folder(&db.conn(), account_id, FolderRole::Archive)?.is_some() {
            return Ok(());
        }
    }
    Err(Error::ActionRefused)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::tests::seed;

    fn add_folders(db: &Database) {
        db.conn()
            .execute_batch(
                "INSERT INTO folders (id, account_id, path, name, role, total_count)
                 VALUES ('archive', 'acc', 'Archive', 'Archive', 'archive', 0),
                        ('trash', 'acc', 'Trash', 'Trash', 'trash', 0),
                        ('projects', 'acc', 'Projects', 'Projects', NULL, 0);
                 INSERT INTO accounts (id, display_name, email_address, auth_method, imap_host,
                   imap_port, imap_security, imap_username, created_at)
                 VALUES ('other', 'Work', 'alex@example.com', 'password', 'imap.example.com',
                   993, 'tls', 'alex@example.com', 0);
                 INSERT INTO folders (id, account_id, path, name, role)
                 VALUES ('other-inbox', 'other', 'INBOX', 'INBOX', 'inbox');",
            )
            .unwrap();
    }

    fn listed(db: &Database, folder_id: &str) -> Vec<String> {
        crate::mailbox::list_messages(&db.conn(), folder_id, Default::default())
            .unwrap()
            .rows
            .into_iter()
            .map(|row| row.latest.id)
            .collect()
    }

    fn counts(db: &Database, folder_id: &str) -> (u32, u32) {
        db.conn()
            .query_row(
                "SELECT total_count, unread_count FROM folders WHERE id = ?1",
                [folder_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap()
    }

    fn queued(db: &Database) -> Vec<(String, String, Option<String>, bool)> {
        let now = now_millis();
        db.conn()
            .prepare(
                "SELECT message_id, action, payload, due_at > ?1 FROM pending_actions ORDER BY id",
            )
            .unwrap()
            .query_map([now], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    #[test]
    fn archiving_hides_at_once_and_waits_before_the_server() {
        let db = Database::open_in_memory().unwrap();
        let (m1, m2) = seed(&db);
        add_folders(&db);

        let outcome = queue(&db, std::slice::from_ref(&m1), Destination::Archive).unwrap();

        assert_eq!(outcome.moved, std::slice::from_ref(&m1));
        assert_eq!(outcome.accounts, ["acc"]);
        assert_eq!(listed(&db, "inbox"), [m2]);
        assert_eq!(counts(&db, "inbox"), (1, 1));
        assert_eq!(counts(&db, "archive"), (1, 1));
        assert_eq!(
            queued(&db),
            [(m1, "move".into(), Some(r#"{"to":"archive"}"#.into()), true)]
        );
    }

    #[test]
    fn undo_brings_it_back() {
        let db = Database::open_in_memory().unwrap();
        let (m1, m2) = seed(&db);
        add_folders(&db);
        queue(
            &db,
            &[m1.clone(), m2.clone()],
            Destination::Folder("projects"),
        )
        .unwrap();

        assert_eq!(
            undo_moves(&db, std::slice::from_ref(&m1)).unwrap(),
            std::slice::from_ref(&m1)
        );

        assert_eq!(listed(&db, "inbox"), std::slice::from_ref(&m1));
        assert_eq!(counts(&db, "inbox"), (1, 1));
        assert_eq!(counts(&db, "projects"), (1, 1));
        assert_eq!(queued(&db).len(), 1);
        // Nothing left to take back.
        assert!(undo_moves(&db, &[m1]).unwrap().is_empty());
    }

    #[test]
    fn the_trash_deletes_for_good() {
        let db = Database::open_in_memory().unwrap();
        let (m1, _) = seed(&db);
        add_folders(&db);
        queue(&db, std::slice::from_ref(&m1), Destination::Trash).unwrap();
        // Moved to the trash: there is nothing to delete twice until it arrives there.
        assert_eq!(queued(&db)[0].1, "move");
        assert!(
            queue(&db, std::slice::from_ref(&m1), Destination::Trash)
                .unwrap()
                .moved
                .is_empty()
        );

        db.conn()
            .execute_batch(
                "INSERT INTO messages (id, account_id, folder_id, uid, subject, fetched_at)
                 VALUES ('t1', 'acc', 'trash', 7, 'Old', 0);",
            )
            .unwrap();
        let outcome = delete_messages(&db, &["t1".into()]).unwrap();
        assert_eq!(outcome.deleted, ["t1"]);
        assert!(listed(&db, "trash").is_empty());
        let last = queued(&db).pop().unwrap();
        assert_eq!((last.1.as_str(), last.3), ("delete", false));
        // Deleting for good cannot be undone.
        assert!(undo_moves(&db, &["t1".into()]).unwrap().is_empty());
    }

    #[test]
    fn nothing_moves_where_it_does_not_belong() {
        let db = Database::open_in_memory().unwrap();
        let (m1, _) = seed(&db);
        add_folders(&db);
        // Another account's folder, the folder it is in, and an archive it is already in.
        assert!(
            move_messages(&db, std::slice::from_ref(&m1), "other-inbox")
                .unwrap()
                .accounts
                .is_empty()
        );
        assert!(
            move_messages(&db, std::slice::from_ref(&m1), "inbox")
                .unwrap()
                .moved
                .is_empty()
        );
        db.conn()
            .execute(
                "UPDATE messages SET folder_id = 'archive' WHERE id = ?1",
                [&m1],
            )
            .unwrap();
        assert!(
            queue(&db, std::slice::from_ref(&m1), Destination::Archive)
                .unwrap()
                .moved
                .is_empty()
        );
        assert!(queued(&db).is_empty());
    }

    #[test]
    fn gmail_archives_to_all_mail_and_others_need_a_folder() {
        let db = Database::open_in_memory().unwrap();
        let (m1, _) = seed(&db);
        assert_eq!(
            accounts_without_archive(&db.conn(), std::slice::from_ref(&m1)).unwrap(),
            ["acc"]
        );
        db.conn()
            .execute_batch(
                "INSERT INTO folders (id, account_id, path, name, role)
                 VALUES ('all', 'acc', '[Gmail]/All Mail', 'All Mail', 'all');",
            )
            .unwrap();
        assert!(
            accounts_without_archive(&db.conn(), std::slice::from_ref(&m1))
                .unwrap()
                .is_empty()
        );
        let outcome = queue(&db, std::slice::from_ref(&m1), Destination::Archive).unwrap();
        assert_eq!(outcome.moved, [m1]);
        assert_eq!(queued(&db)[0].2.as_deref(), Some(r#"{"to":"all"}"#));
    }
}
