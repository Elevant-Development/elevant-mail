//! Reading folders and messages from the cache. The sync engine fills it; this only reads.

use rusqlite::{Connection, OptionalExtension, Row};

use crate::Result;
use crate::mail::sanitize::{HtmlView, sanitize};
use crate::model::{Address, Folder, FolderRole, Message, MessageSummary};

/// Mailbox id that shows the inboxes of all accounts together (same value as in the frontend).
pub const UNIFIED_INBOX: &str = "unified-inbox";

/// Newest first. The list is virtualized, so a few thousand rows stay cheap; paging comes later.
const LIST_LIMIT: u32 = 5000;

/// The account a folder belongs to.
pub fn folder_account(conn: &Connection, folder_id: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT account_id FROM folders WHERE id = ?1",
            [folder_id],
            |row| row.get(0),
        )
        .optional()?)
}

pub fn message_account(conn: &Connection, message_id: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT account_id FROM messages WHERE id = ?1",
            [message_id],
            |row| row.get(0),
        )
        .optional()?)
}

pub fn list_folders(conn: &Connection) -> Result<Vec<Folder>> {
    let mut statement = conn.prepare(
        "SELECT f.id, f.account_id, f.name, f.role, f.unread_count, f.total_count, f.path,
           f.delimiter
         FROM folders f JOIN accounts a ON a.id = f.account_id
         ORDER BY a.sort_order, a.created_at, f.path",
    )?;
    let folders = statement
        .query_map([], |row| {
            let role: Option<String> = row.get(3)?;
            let path: String = row.get(6)?;
            let delimiter: Option<String> = row.get(7)?;
            Ok(Folder {
                id: row.get(0)?,
                account_id: row.get(1)?,
                name: row.get(2)?,
                path_names: path_names(&path, delimiter.as_deref()),
                path,
                role: role.as_deref().and_then(FolderRole::parse),
                unread_count: row.get(4)?,
                total_count: row.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(folders)
}

/// "INBOX.Projekte.2026" → ["INBOX", "Projekte", "2026"], with modified UTF-7 decoded.
fn path_names(path: &str, delimiter: Option<&str>) -> Vec<String> {
    let segments: Vec<&str> = match delimiter.filter(|delimiter| !delimiter.is_empty()) {
        Some(delimiter) => path.split(delimiter).collect(),
        None => vec![path],
    };
    segments
        .into_iter()
        .map(|segment| utf7_imap::decode_utf7_imap(segment.to_owned()))
        .collect()
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
         ORDER BY coalesce(m.date_sent, m.date_received, m.fetched_at) DESC, m.uid DESC LIMIT {LIST_LIMIT}"
    );
    let messages = conn
        .prepare(&sql)?
        .query_map([mailbox_id], summary_from_row)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(messages)
}

/// Search results shown at most; the newest come first.
const SEARCH_LIMIT: u32 = 300;

/// Full-text search over subject, sender and text of all cached mail, newest first. Every word
/// must match, as a word or the start of one ("rech" finds "Rechnung").
///
/// A message that sits in several folders of an account (Gmail shows labels and "All Mail" as
/// folders, other servers keep copies) is listed once, preferably from the inbox.
pub fn search_messages(conn: &Connection, query: &str) -> Result<Vec<MessageSummary>> {
    let Some(fts_query) = fts_query(query) else {
        return Ok(Vec::new());
    };
    let sql = format!(
        "SELECT {SUMMARY_COLUMNS} FROM (
           SELECT m.*, row_number() OVER (
             PARTITION BY m.account_id, coalesce(m.message_id, m.id)
             ORDER BY f.role = 'inbox' DESC, f.role IS NOT NULL AND f.role <> 'all' DESC
           ) AS copy
           FROM messages_fts
           JOIN messages m ON m.seq = messages_fts.rowid
           JOIN folders f ON f.id = m.folder_id
           WHERE messages_fts MATCH ?1
         ) m
         WHERE m.copy = 1
         ORDER BY coalesce(m.date_sent, m.date_received, m.fetched_at) DESC, m.uid DESC
         LIMIT {SEARCH_LIMIT}"
    );
    let messages = conn
        .prepare(&sql)?
        .query_map([fts_query], summary_from_row)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(messages)
}

/// User input as an FTS5 query: each word quoted (so `-`, `:` or `AND` are just text) and
/// matched as a prefix.
fn fts_query(input: &str) -> Option<String> {
    let terms: Vec<String> = input
        .split_whitespace()
        .map(|word| word.replace('"', ""))
        .filter(|word| word.chars().any(char::is_alphanumeric))
        .map(|word| format!("\"{word}\"*"))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

/// A message ready to show, plus what the asset protocol needs to serve its external images.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenedMessage {
    pub message: Message,
    /// External image URLs; `<asset_base>/remote/<message>/<n>` serves the n-th once allowed.
    pub remote_images: Vec<String>,
}

pub fn get_message(
    conn: &Connection,
    message_id: &str,
    view: HtmlView<'_>,
) -> Result<Option<OpenedMessage>> {
    let sql = format!(
        "SELECT {SUMMARY_COLUMNS}, m.to_addresses, m.cc_addresses, m.body_text, m.body_state,
           m.body_html
         FROM messages m WHERE m.id = ?1"
    );
    let row = conn
        .query_row(&sql, [message_id], |row| {
            let addresses = |index: usize| -> rusqlite::Result<Vec<Address>> {
                let json: Option<String> = row.get(index)?;
                Ok(json
                    .and_then(|json| serde_json::from_str(&json).ok())
                    .unwrap_or_default())
            };
            let message = Message {
                summary: summary_from_row(row)?,
                to: addresses(11)?,
                cc: addresses(12)?,
                body_text: row.get(13)?,
                body_complete: row.get::<_, String>(14)? == "full",
                body_html: None,
                html_styled: false,
                remote_image_count: 0,
                attachments: Vec::new(),
            };
            Ok((message, row.get::<_, Option<String>>(15)?))
        })
        .optional()?;
    let Some((mut message, raw_html)) = row else {
        return Ok(None);
    };

    message.attachments = crate::attachments::list(conn, message_id)?;
    let mut remote_images = Vec::new();
    if let Some(raw_html) = raw_html {
        let safe = sanitize(&raw_html, message_id, view);
        message.body_html = Some(safe.html);
        message.html_styled = safe.styled;
        message.remote_image_count = u32::try_from(safe.remote_images.len()).unwrap_or(u32::MAX);
        remote_images = safe.remote_images;
    }
    Ok(Some(OpenedMessage {
        message,
        remote_images,
    }))
}

/// An image the HTML of a message refers to by `cid:`: its type and bytes.
pub fn inline_image(
    conn: &Connection,
    message_id: &str,
    content_id: &str,
) -> Result<Option<(String, Vec<u8>)>> {
    Ok(conn
        .query_row(
            "SELECT mime_type, data FROM attachments
             WHERE message_id = ?1 AND content_id = ?2 AND data IS NOT NULL",
            [message_id, content_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?)
}

#[cfg(test)]
mod tests {
    use rusqlite::params;

    use super::*;
    use crate::db::Database;
    use crate::ids::new_id;

    const VIEW: HtmlView<'static> = HtmlView {
        asset_base: "mail-asset://localhost",
        remote_images: false,
    };

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
        let message = get_message(&conn, &newest, VIEW).unwrap().unwrap().message;
        assert_eq!(
            message.to,
            vec![Address {
                name: Some("Alex".into()),
                address: "alex@example.org".into()
            }]
        );
        assert_eq!(message.body_text.as_deref(), Some("Hello"));
        assert!(get_message(&conn, "missing", VIEW).unwrap().is_none());
        assert!(message.body_html.is_none());
    }

    #[test]
    fn html_is_sanitized_when_read() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn();
        let (newest, _) = seed(&conn);
        conn.execute(
            "UPDATE messages SET body_html = ?2 WHERE id = ?1",
            params![
                newest,
                r#"<p onclick="x()">Hi</p><script>x()</script><img src="https://t.example/p.gif">"#
            ],
        )
        .unwrap();

        let opened = get_message(&conn, &newest, VIEW).unwrap().unwrap();
        let html = opened.message.body_html.unwrap();
        assert_eq!(html, "<p>Hi</p><img>");
        assert_eq!(opened.message.remote_image_count, 1);
        assert_eq!(opened.remote_images, ["https://t.example/p.gif"]);
    }

    #[test]
    fn serves_cached_inline_images() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn();
        let (newest, _) = seed(&conn);
        conn.execute(
            "INSERT INTO attachments (id, message_id, part_id, mime_type, content_id, is_inline, data)
             VALUES ('a1', ?1, '2', 'image/png', 'logo@example.org', 1, x'89504E47')",
            [&newest],
        )
        .unwrap();
        let (mime, data) = inline_image(&conn, &newest, "logo@example.org")
            .unwrap()
            .unwrap();
        assert_eq!(mime, "image/png");
        assert_eq!(data, [0x89, b'P', b'N', b'G']);
        assert!(inline_image(&conn, &newest, "other").unwrap().is_none());
    }

    #[test]
    fn builds_safe_search_queries() {
        assert_eq!(
            fts_query("  Rech  Müller "),
            Some(r#""Rech"* "Müller"*"#.into())
        );
        assert_eq!(
            fts_query(r#"a"b OR -x"#),
            Some(r#""ab"* "OR"* "-x"*"#.into())
        );
        assert_eq!(fts_query(" \" - "), None);
    }

    #[test]
    fn searches_subject_sender_and_text() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn();
        let (target, _) = seed(&conn);
        conn.execute(
            "UPDATE messages SET subject = 'Rechnung Oktober', body_text = 'Grüße aus Köln'
             WHERE id = ?1",
            [&target],
        )
        .unwrap();

        let ids = |query: &str| -> Vec<String> {
            search_messages(&conn, query)
                .unwrap()
                .into_iter()
                .map(|m| m.id)
                .collect()
        };
        assert_eq!(ids("rech"), [target.as_str()]);
        // Diacritics do not matter (ß is its own letter), and every word has to match.
        assert_eq!(ids("koln grüße"), [target.as_str()]);
        assert_eq!(ids("Köln Oktober"), [target.as_str()]);
        assert!(ids("Oktober nirgends").is_empty());
        assert!(ids("").is_empty());
        // Sender addresses, newest first, across folders.
        assert_eq!(ids("lena@example").len(), 3);
    }

    #[test]
    fn search_lists_a_message_in_several_folders_once() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn();
        seed(&conn);
        conn.execute_batch(
            "INSERT INTO folders (id, account_id, name, path, role)
               VALUES ('all', 'acc', 'Alle Nachrichten', '[Gmail]/All Mail', 'all'),
                      ('label', 'acc', 'Projekt', 'Projekt', NULL);
             INSERT INTO messages (id, account_id, folder_id, uid, message_id, subject, fetched_at)
               VALUES ('in-all', 'acc', 'all', 7, '<x@example.org>', 'Quartalszahlen', 0),
                      ('in-label', 'acc', 'label', 3, '<x@example.org>', 'Quartalszahlen', 0),
                      ('in-inbox', 'acc', 'inbox', 9, '<x@example.org>', 'Quartalszahlen', 0),
                      ('other', 'acc', 'all', 8, '<y@example.org>', 'Quartalszahlen alt', 0);",
        )
        .unwrap();
        let mut ids: Vec<String> = search_messages(&conn, "quartal")
            .unwrap()
            .into_iter()
            .map(|m| m.id)
            .collect();
        ids.sort();
        assert_eq!(ids, ["in-inbox", "other"]);
    }

    #[test]
    fn splits_and_decodes_folder_paths() {
        assert_eq!(
            path_names("INBOX.Projekte.2026", Some(".")),
            ["INBOX", "Projekte", "2026"]
        );
        assert_eq!(
            path_names("[Gmail]/Entw&APw-rfe", Some("/")),
            ["[Gmail]", "Entwürfe"]
        );
        assert_eq!(path_names("Archiv", None), ["Archiv"]);
    }
}
