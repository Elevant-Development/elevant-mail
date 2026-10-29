//! Reading folders and messages from the cache. The sync engine fills it; this only reads.

use rusqlite::{Connection, OptionalExtension, Row};

use crate::Result;
use crate::model::{Address, Folder, FolderRole, Message, MessageSummary};

/// Mailbox id that shows the inboxes of all accounts together (same value as in the frontend).
pub const UNIFIED_INBOX: &str = "unified-inbox";

/// Newest first; more than this is paged in later.
const LIST_LIMIT: u32 = 500;

pub fn list_folders(conn: &Connection) -> Result<Vec<Folder>> {
    let mut statement = conn.prepare(
        "SELECT f.id, f.account_id, f.name, f.role, f.unread_count, f.total_count
         FROM folders f JOIN accounts a ON a.id = f.account_id
         ORDER BY a.sort_order, a.created_at, f.path",
    )?;
    let folders = statement
        .query_map([], |row| {
            let role: Option<String> = row.get(3)?;
            Ok(Folder {
                id: row.get(0)?,
                account_id: row.get(1)?,
                name: row.get(2)?,
                role: role.as_deref().and_then(FolderRole::parse),
                unread_count: row.get(4)?,
                total_count: row.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(folders)
}

const SUMMARY_COLUMNS: &str = "m.id, m.account_id, m.folder_id, m.subject, m.from_name,
    coalesce(m.from_address, ''), coalesce(m.date_sent, m.date_received, m.fetched_at),
    coalesce(m.snippet, ''), m.is_read, m.is_flagged, m.has_attachments";

fn summary_from_row(row: &Row<'_>) -> rusqlite::Result<MessageSummary> {
    Ok(MessageSummary {
        id: row.get(0)?,
        account_id: row.get(1)?,
        folder_id: row.get(2)?,
        subject: row.get(3)?,
        from: Address {
            name: row.get(4)?,
            address: row.get(5)?,
        },
        date_sent: row.get(6)?,
        snippet: row.get(7)?,
        is_read: row.get(8)?,
        is_flagged: row.get(9)?,
        has_attachments: row.get(10)?,
    })
}

/// Messages of a folder, or of all inboxes for [`UNIFIED_INBOX`], newest first.
pub fn list_messages(conn: &Connection, mailbox_id: &str) -> Result<Vec<MessageSummary>> {
    let filter = if mailbox_id == UNIFIED_INBOX {
        "m.folder_id IN (SELECT id FROM folders WHERE role = 'inbox' AND ?1 = ?1)"
    } else {
        "m.folder_id = ?1"
    };
    let sql = format!(
        "SELECT {SUMMARY_COLUMNS} FROM messages m WHERE {filter}
         ORDER BY coalesce(m.date_sent, m.date_received, m.fetched_at) DESC LIMIT {LIST_LIMIT}"
    );
    let messages = conn
        .prepare(&sql)?
        .query_map([mailbox_id], summary_from_row)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(messages)
}

pub fn get_message(conn: &Connection, message_id: &str) -> Result<Option<Message>> {
    let sql = format!(
        "SELECT {SUMMARY_COLUMNS}, m.to_addresses, m.cc_addresses, m.body_text
         FROM messages m WHERE m.id = ?1"
    );
    let message = conn
        .query_row(&sql, [message_id], |row| {
            let addresses = |index: usize| -> rusqlite::Result<Vec<Address>> {
                let json: Option<String> = row.get(index)?;
                Ok(json
                    .and_then(|json| serde_json::from_str(&json).ok())
                    .unwrap_or_default())
            };
            Ok(Message {
                summary: summary_from_row(row)?,
                to: addresses(11)?,
                cc: addresses(12)?,
                body_text: row.get(13)?,
            })
        })
        .optional()?;
    Ok(message)
}

#[cfg(test)]
mod tests {
    use rusqlite::params;

    use super::*;
    use crate::db::Database;
    use crate::ids::new_id;

    fn seed(conn: &Connection) -> (String, String) {
        conn.execute(
            "INSERT INTO accounts (id, display_name, email_address, auth_method, imap_host, imap_port,
               imap_security, imap_username, created_at)
             VALUES ('acc', 'Personal', 'alex@example.org', 'password', 'imap.example.org', 993, 'tls',
               'alex@example.org', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO folders (id, account_id, name, path, role) VALUES ('inbox', 'acc', 'INBOX', 'INBOX', 'inbox'),
               ('sent', 'acc', 'Sent', 'Sent', 'sent')",
            [],
        )
        .unwrap();
        let insert = |folder: &str, uid: u32, date: i64, subject: &str| {
            let id = new_id();
            conn.execute(
                "INSERT INTO messages (id, account_id, folder_id, uid, subject, from_name, from_address,
                   to_addresses, date_sent, body_text, fetched_at)
                 VALUES (?1, 'acc', ?2, ?3, ?4, 'Lena', 'lena@example.org',
                   '[{\"name\":\"Alex\",\"address\":\"alex@example.org\"}]', ?5, 'Hello', 0)",
                params![id, folder, uid, subject, date],
            )
            .unwrap();
            id
        };
        let newest = insert("inbox", 2, 2_000, "Newer");
        insert("inbox", 1, 1_000, "Older");
        insert("sent", 1, 3_000, "Reply");
        (newest, "inbox".into())
    }

    #[test]
    fn lists_folders_with_roles() {
        let db = Database::open_in_memory().unwrap();
        seed(&db.conn());
        let folders = list_folders(&db.conn()).unwrap();
        assert_eq!(folders.len(), 2);
        assert_eq!(folders[0].role, Some(FolderRole::Inbox));
    }

    #[test]
    fn lists_messages_newest_first_per_folder_and_unified() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn();
        let (newest, inbox) = seed(&conn);

        let in_folder = list_messages(&conn, &inbox).unwrap();
        assert_eq!(
            in_folder
                .iter()
                .map(|m| m.subject.as_deref().unwrap())
                .collect::<Vec<_>>(),
            ["Newer", "Older"]
        );
        assert_eq!(in_folder[0].id, newest);

        // The unified inbox leaves out sent mail.
        assert_eq!(list_messages(&conn, UNIFIED_INBOX).unwrap().len(), 2);
    }

    #[test]
    fn reads_a_full_message() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn();
        let (newest, _) = seed(&conn);
        let message = get_message(&conn, &newest).unwrap().unwrap();
        assert_eq!(
            message.to,
            vec![Address {
                name: Some("Alex".into()),
                address: "alex@example.org".into()
            }]
        );
        assert_eq!(message.body_text.as_deref(), Some("Hello"));
        assert!(get_message(&conn, "missing").unwrap().is_none());
    }
}
