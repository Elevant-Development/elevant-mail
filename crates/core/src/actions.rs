//! Read and flagged state set by the user. It changes the cache at once (the UI reads only from
//! there) and queues the change for the server, so it also works offline. `mail::push` sends
//! the queue; until then the sync leaves these messages' flags alone.

use rusqlite::{Connection, OptionalExtension, params};

use crate::Result;
use crate::db::Database;
use crate::ids::now_millis;

/// Which flag an action sets, as stored in `pending_actions.action`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagAction {
    Read,
    Flagged,
}

impl FlagAction {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            FlagAction::Read => "set_read",
            FlagAction::Flagged => "set_flagged",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "set_read" => Some(FlagAction::Read),
            "set_flagged" => Some(FlagAction::Flagged),
            _ => None,
        }
    }

    fn column(self) -> &'static str {
        match self {
            FlagAction::Read => "is_read",
            FlagAction::Flagged => "is_flagged",
        }
    }
}

/// Sets (not toggles) a flag, so a double click or a repeated request stays correct.
/// Returns the accounts whose queue got new entries, for sending them right away.
pub fn set_flag(
    db: &Database,
    message_ids: &[String],
    action: FlagAction,
    value: bool,
) -> Result<Vec<String>> {
    let mut conn = db.conn();
    let tx = conn.transaction()?;
    let mut accounts = Vec::new();
    for message_id in message_ids {
        let Some((account_id, folder_id, current)) = state_of(&tx, message_id, action)? else {
            continue;
        };
        if current == value {
            continue;
        }
        tx.execute(
            &format!("UPDATE messages SET {} = ?2 WHERE id = ?1", action.column()),
            params![message_id, value],
        )?;
        if action == FlagAction::Read {
            // The server's count replaces this at the next sync.
            tx.execute(
                "UPDATE folders SET unread_count = max(unread_count + ?2, 0) WHERE id = ?1",
                params![folder_id, if value { -1 } else { 1 }],
            )?;
        }
        // Only the latest wish per message and flag goes to the server.
        tx.execute(
            "DELETE FROM pending_actions WHERE message_id = ?1 AND action = ?2",
            params![message_id, action.as_str()],
        )?;
        tx.execute(
            "INSERT INTO pending_actions (account_id, message_id, action, payload, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                account_id,
                message_id,
                action.as_str(),
                serde_json::json!({ "value": value }).to_string(),
                now_millis()
            ],
        )?;
        if !accounts.contains(&account_id) {
            accounts.push(account_id);
        }
    }
    tx.commit()?;
    Ok(accounts)
}

fn state_of(
    conn: &Connection,
    message_id: &str,
    action: FlagAction,
) -> Result<Option<(String, String, bool)>> {
    Ok(conn
        .query_row(
            &format!(
                "SELECT account_id, folder_id, {} FROM messages WHERE id = ?1",
                action.column()
            ),
            [message_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?)
}

/// Accounts with changes still waiting for the server.
pub fn accounts_with_pending(conn: &Connection) -> Result<Vec<String>> {
    let mut statement = conn.prepare("SELECT DISTINCT account_id FROM pending_actions")?;
    let accounts = statement
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(accounts)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// An account, an inbox and two unread messages; returns their ids.
    pub(crate) fn seed(db: &Database) -> (String, String) {
        let conn = db.conn();
        conn.execute_batch(
            "INSERT INTO accounts (id, display_name, email_address, auth_method, imap_host,
               imap_port, imap_security, imap_username, created_at)
             VALUES ('acc', 'Personal', 'alex@example.org', 'password', 'imap.example.org', 993,
               'tls', 'alex@example.org', 0);
             INSERT INTO folders (id, account_id, path, name, role, unread_count, total_count)
             VALUES ('inbox', 'acc', 'INBOX', 'Posteingang', 'inbox', 2, 2);
             INSERT INTO messages (id, account_id, folder_id, uid, subject, fetched_at)
             VALUES ('m1', 'acc', 'inbox', 1, 'One', 0), ('m2', 'acc', 'inbox', 2, 'Two', 0);",
        )
        .unwrap();
        ("m1".into(), "m2".into())
    }

    fn pending(db: &Database) -> Vec<(String, String, String)> {
        let conn = db.conn();
        let mut statement = conn
            .prepare("SELECT message_id, action, payload FROM pending_actions ORDER BY id")
            .unwrap();
        statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    fn unread(db: &Database) -> u32 {
        db.conn()
            .query_row("SELECT unread_count FROM folders", [], |row| row.get(0))
            .unwrap()
    }

    #[test]
    fn marks_read_at_once_and_queues_it() {
        let db = Database::open_in_memory().unwrap();
        let (m1, m2) = seed(&db);
        let accounts = set_flag(&db, &[m1.clone(), m2], FlagAction::Read, true).unwrap();
        assert_eq!(accounts, ["acc"]);
        let read: bool = db
            .conn()
            .query_row("SELECT is_read FROM messages WHERE id = ?1", [&m1], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(read);
        assert_eq!(unread(&db), 0);
        assert_eq!(pending(&db).len(), 2);
        assert_eq!(pending(&db)[0].2, r#"{"value":true}"#);
    }

    #[test]
    fn sets_instead_of_toggling() {
        let db = Database::open_in_memory().unwrap();
        let (m1, _) = seed(&db);
        set_flag(&db, std::slice::from_ref(&m1), FlagAction::Read, true).unwrap();
        // The same wish again changes nothing and queues nothing new.
        assert!(
            set_flag(&db, std::slice::from_ref(&m1), FlagAction::Read, true)
                .unwrap()
                .is_empty()
        );
        assert_eq!(unread(&db), 1);
        assert_eq!(pending(&db).len(), 1);
    }

    #[test]
    fn keeps_only_the_latest_wish_per_flag() {
        let db = Database::open_in_memory().unwrap();
        let (m1, _) = seed(&db);
        set_flag(&db, std::slice::from_ref(&m1), FlagAction::Flagged, true).unwrap();
        set_flag(&db, std::slice::from_ref(&m1), FlagAction::Read, true).unwrap();
        set_flag(&db, std::slice::from_ref(&m1), FlagAction::Flagged, false).unwrap();
        let queued = pending(&db);
        assert_eq!(queued.len(), 2);
        assert_eq!(queued[0].1, "set_read");
        assert_eq!(
            (queued[1].1.as_str(), queued[1].2.as_str()),
            ("set_flagged", r#"{"value":false}"#)
        );
        assert_eq!(accounts_with_pending(&db.conn()).unwrap(), ["acc"]);
    }

    #[test]
    fn ignores_unknown_messages() {
        let db = Database::open_in_memory().unwrap();
        seed(&db);
        assert!(
            set_flag(&db, &["gone".into()], FlagAction::Read, true)
                .unwrap()
                .is_empty()
        );
        assert!(pending(&db).is_empty());
    }
}
