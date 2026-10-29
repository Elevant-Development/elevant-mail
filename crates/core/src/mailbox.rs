//! Reading folders and messages from the cache. The sync engine fills it; this only reads.

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension, Row};

use crate::Result;
use crate::mail::sanitize::{HtmlView, sanitize};
use crate::model::{Address, Conversation, Folder, FolderRole, Message, MessageSummary};

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

/// A message with what grouping it into conversations needs.
struct Member {
    summary: MessageSummary,
    /// The conversation, or the message's own id while it is in none yet.
    thread: String,
    /// The Message-ID header: copies of one mail in several folders share it.
    header_id: Option<String>,
}

/// After [`SUMMARY_COLUMNS`], for [`member_from_row`].
const MEMBER_COLUMNS: &str = "coalesce(m.thread_id, m.id), m.message_id";

const DATE: &str = "coalesce(m.date_sent, m.date_received, m.fetched_at)";

fn member_from_row(row: &Row<'_>) -> rusqlite::Result<Member> {
    Ok(Member {
        summary: summary_from_row(row)?,
        thread: row.get(11)?,
        header_id: row.get(12)?,
    })
}

fn members(conn: &Connection, sql: &str, params: impl rusqlite::Params) -> Result<Vec<Member>> {
    Ok(conn
        .prepare_cached(sql)?
        .query_map(params, member_from_row)?
        .collect::<rusqlite::Result<_>>()?)
}

/// The user's own replies in the conversation, from the account's "Sent".
fn replies_in(conn: &Connection, account_id: &str, thread: &str) -> Result<Vec<Member>> {
    members(
        conn,
        &format!(
            "SELECT {SUMMARY_COLUMNS}, {MEMBER_COLUMNS} FROM messages m
             JOIN folders f ON f.id = m.folder_id
             WHERE m.account_id = ?1 AND m.thread_id = ?2 AND f.role = 'sent' AND m.leaving = 0"
        ),
        [account_id, thread],
    )
}

/// Conversations of a folder, or of all inboxes for [`UNIFIED_INBOX`], newest first. Without
/// `threads` every message is a row of its own.
pub fn list_messages(
    conn: &Connection,
    mailbox_id: &str,
    threads: bool,
) -> Result<Vec<Conversation>> {
    let filter = if mailbox_id == UNIFIED_INBOX {
        "m.folder_id IN (SELECT id FROM folders WHERE role = 'inbox' AND ?1 = ?1)"
    } else {
        "m.folder_id = ?1"
    };
    // Mail on its way to another folder (moves::move_messages) no longer shows here.
    let listed = members(
        conn,
        &format!(
            "SELECT {SUMMARY_COLUMNS}, {MEMBER_COLUMNS} FROM messages m
             WHERE {filter} AND m.leaving = 0
             ORDER BY {DATE} DESC, m.uid DESC LIMIT {LIST_LIMIT}"
        ),
        [mailbox_id],
    )?;
    if !threads {
        return Ok(group(listed, Vec::new(), false));
    }
    let replies = members(
        conn,
        &format!(
            "SELECT {SUMMARY_COLUMNS}, {MEMBER_COLUMNS} FROM messages m
             JOIN folders f ON f.id = m.folder_id
             WHERE f.role = 'sent' AND m.leaving = 0
               AND m.thread_id IN (SELECT m.thread_id FROM messages m WHERE {filter})"
        ),
        [mailbox_id],
    )?;
    Ok(group(listed, replies, true))
}

/// Search results shown at most; the newest come first.
const SEARCH_LIMIT: u32 = 300;

/// Full-text search over subject, sender and text of all cached mail, newest first. Every word
/// must match, as a word or the start of one ("rech" finds "Rechnung").
///
/// A message that sits in several folders of an account (Gmail shows labels and "All Mail" as
/// folders, other servers keep copies) is listed once, preferably from the inbox. With
/// `threads`, a result is the whole conversation in that folder, like in the list.
pub fn search_messages(conn: &Connection, query: &str, threads: bool) -> Result<Vec<Conversation>> {
    let Some(fts_query) = fts_query(query) else {
        return Ok(Vec::new());
    };
    let hits = members(
        conn,
        &format!(
            "SELECT {SUMMARY_COLUMNS}, {MEMBER_COLUMNS} FROM (
               SELECT m.*, row_number() OVER (
                 PARTITION BY m.account_id, coalesce(m.message_id, m.id)
                 ORDER BY f.role = 'inbox' DESC, f.role IS NOT NULL AND f.role <> 'all' DESC
               ) AS copy
               FROM messages_fts
               JOIN messages m ON m.seq = messages_fts.rowid
               JOIN folders f ON f.id = m.folder_id
               WHERE messages_fts MATCH ?1 AND m.leaving = 0
             ) m
             WHERE m.copy = 1
             ORDER BY {DATE} DESC, m.uid DESC
             LIMIT {SEARCH_LIMIT}"
        ),
        [fts_query],
    )?;
    if !threads {
        return Ok(group(hits, Vec::new(), false));
    }

    let mut listed = Vec::new();
    let mut replies = Vec::new();
    let mut seen = HashSet::new();
    for hit in &hits {
        let folder_id = answered_in(conn, hit)?.unwrap_or_else(|| hit.summary.folder_id.clone());
        if !seen.insert((folder_id.clone(), hit.thread.clone())) {
            continue;
        }
        listed.extend(members(
            conn,
            &format!(
                "SELECT {SUMMARY_COLUMNS}, {MEMBER_COLUMNS} FROM messages m
                 WHERE m.folder_id = ?1 AND (m.thread_id = ?2 OR m.id = ?2) AND m.leaving = 0"
            ),
            [&folder_id, &hit.thread],
        )?);
        replies.extend(replies_in(conn, &hit.summary.account_id, &hit.thread)?);
    }
    listed.sort_by_key(|member| Reverse(member.summary.date_sent));
    Ok(group(listed, replies, true))
}

/// For a hit in "Sent": the folder that holds the rest of its conversation, the inbox first. The
/// reply shows there along with the mail it answers, as in the list.
fn answered_in(conn: &Connection, hit: &Member) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            &format!(
                "SELECT m.folder_id FROM messages m JOIN folders f ON f.id = m.folder_id
                 WHERE m.account_id = ?1 AND m.thread_id = ?2 AND m.leaving = 0
                   AND coalesce(f.role, '') NOT IN ('sent', 'drafts', 'trash', 'junk')
                   AND EXISTS (SELECT 1 FROM folders s WHERE s.id = ?3 AND s.role = 'sent')
                 ORDER BY coalesce(f.role, '') = 'inbox' DESC, coalesce(f.role, '') = 'all',
                   {DATE} DESC
                 LIMIT 1"
            ),
            [&hit.summary.account_id, &hit.thread, &hit.summary.folder_id],
            |row| row.get(0),
        )
        .optional()?)
}

/// What the reader shows for a message: its conversation in the message's folder plus the
/// user's own replies from "Sent", newest first. Without `threads` the message alone.
pub fn get_conversation(
    conn: &Connection,
    message_id: &str,
    threads: bool,
) -> Result<Vec<MessageSummary>> {
    let opened: Option<(String, String, Option<String>)> = conn
        .query_row(
            "SELECT account_id, folder_id, thread_id FROM messages WHERE id = ?1",
            [message_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((account_id, folder_id, thread)) = opened else {
        return Ok(Vec::new());
    };
    let Some(thread) = thread.filter(|_| threads) else {
        let alone = members(
            conn,
            &format!("SELECT {SUMMARY_COLUMNS}, {MEMBER_COLUMNS} FROM messages m WHERE m.id = ?1"),
            [message_id],
        )?;
        return Ok(alone.into_iter().map(|member| member.summary).collect());
    };

    let mut shown = members(
        conn,
        &format!(
            "SELECT {SUMMARY_COLUMNS}, {MEMBER_COLUMNS} FROM messages m
             WHERE m.folder_id = ?1 AND m.thread_id = ?2 AND m.leaving = 0"
        ),
        [&folder_id, &thread],
    )?;
    let known: HashSet<String> = shown.iter().filter_map(|m| m.header_id.clone()).collect();
    shown.extend(
        replies_in(conn, &account_id, &thread)?
            .into_iter()
            .filter(|reply| reply.summary.folder_id != folder_id)
            .filter(|reply| {
                reply
                    .header_id
                    .as_ref()
                    .is_none_or(|id| !known.contains(id))
            }),
    );
    shown.sort_by_key(|member| Reverse(member.summary.date_sent));
    Ok(shown.into_iter().map(|member| member.summary).collect())
}

/// Rows from messages that come newest first: one per folder and conversation (per message
/// without `threads`), with the replies from "Sent" counted in.
fn group(listed: Vec<Member>, replies: Vec<Member>, threads: bool) -> Vec<Conversation> {
    struct Row {
        conversation: Conversation,
        thread: String,
        header_ids: HashSet<String>,
        writers: Vec<(i64, Address)>,
    }

    let mut rows: Vec<Row> = Vec::new();
    let mut index: HashMap<(String, String), usize> = HashMap::new();
    for member in listed {
        let Member {
            summary,
            thread,
            header_id,
        } = member;
        let thread = if threads { thread } else { summary.id.clone() };
        let key = (summary.folder_id.clone(), thread.clone());
        let writer = (summary.date_sent, summary.from.clone());
        let row = match index.get(&key) {
            Some(&at) => {
                let row = &mut rows[at];
                let c = &mut row.conversation;
                c.message_ids.push(summary.id.clone());
                c.count += 1;
                c.latest.is_read &= summary.is_read;
                c.latest.is_flagged |= summary.is_flagged;
                c.latest.has_attachments |= summary.has_attachments;
                row
            }
            None => {
                index.insert(key, rows.len());
                rows.push(Row {
                    conversation: Conversation {
                        message_ids: vec![summary.id.clone()],
                        count: 1,
                        senders: Vec::new(),
                        latest: summary,
                    },
                    thread,
                    header_ids: HashSet::new(),
                    writers: Vec::new(),
                });
                rows.last_mut().expect("just pushed")
            }
        };
        row.writers.push(writer);
        row.header_ids.extend(header_id);
    }

    let mut replies_by_thread: HashMap<&str, Vec<&Member>> = HashMap::new();
    for reply in &replies {
        replies_by_thread
            .entry(reply.thread.as_str())
            .or_default()
            .push(reply);
    }
    rows.into_iter()
        .map(|mut row| {
            let mut counted: HashSet<&str> = HashSet::new();
            for reply in replies_by_thread
                .get(row.thread.as_str())
                .into_iter()
                .flatten()
            {
                // In "Sent" itself they are rows of the list already; copies count once.
                let copy = reply
                    .header_id
                    .as_deref()
                    .is_some_and(|id| row.header_ids.contains(id) || !counted.insert(id));
                if reply.summary.folder_id == row.conversation.latest.folder_id || copy {
                    continue;
                }
                row.conversation.count += 1;
                row.writers
                    .push((reply.summary.date_sent, reply.summary.from.clone()));
            }
            row.writers.sort_by_key(|(date, _)| *date);
            let mut seen = HashSet::new();
            row.conversation.senders = row
                .writers
                .into_iter()
                .filter(|(_, from)| seen.insert(from.address.to_lowercase()))
                .map(|(_, from)| from)
                .collect();
            row.conversation
        })
        .collect()
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

        let in_folder = list_messages(&conn, &inbox, true).unwrap();
        assert_eq!(
            in_folder
                .iter()
                .map(|row| row.latest.subject.as_deref().unwrap())
                .collect::<Vec<_>>(),
            ["Newer", "Older"]
        );
        assert_eq!(in_folder[0].latest.id, newest);

        // The unified inbox leaves out sent mail.
        assert_eq!(list_messages(&conn, UNIFIED_INBOX, true).unwrap().len(), 2);
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
            search_messages(&conn, query, true)
                .unwrap()
                .into_iter()
                .map(|row| row.latest.id)
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
        let mut ids: Vec<String> = search_messages(&conn, "quartal", false)
            .unwrap()
            .into_iter()
            .map(|row| row.latest.id)
            .collect();
        ids.sort();
        assert_eq!(ids, ["in-inbox", "other"]);
    }

    /// A conversation in the inbox with a reply from "Sent", a mail to oneself (one copy in each
    /// folder) and a mail on its own.
    fn seed_conversation(conn: &Connection) {
        seed(conn);
        conn.execute_batch(
            "INSERT INTO messages (id, account_id, folder_id, uid, message_id, thread_id, subject,
               from_name, from_address, date_sent, is_read, is_flagged, body_text, fetched_at)
             VALUES
               ('plan', 'acc', 'inbox', 10, 'plan@x', 't1', 'Wanderung', 'Lena', 'lena@example.org',
                 10000, 1, 0, 'Treffpunkt am Bahnhof', 0),
               ('reply', 'acc', 'sent', 10, 'reply@x', 't1', 'Re: Wanderung', 'Alex',
                 'alex@example.org', 20000, 1, 0, 'Bin dabei', 0),
               ('answer', 'acc', 'inbox', 11, 'answer@x', 't1', 'Re: Wanderung', 'Ben',
                 'ben@example.org', 30000, 0, 1, 'Ich auch', 0),
               ('lena-again', 'acc', 'inbox', 12, 'again@x', 't1', 'Re: Wanderung', 'Lena',
                 'LENA@example.org', 40000, 1, 0, 'Schön', 0),
               ('note', 'acc', 'inbox', 13, 'note@x', 't2', 'Notiz', 'Alex', 'alex@example.org',
                 5000, 1, 0, 'Milch kaufen', 0),
               ('note-sent', 'acc', 'sent', 11, 'note@x', 't2', 'Notiz', 'Alex',
                 'alex@example.org', 5000, 1, 0, 'Milch kaufen', 0);",
        )
        .unwrap();
    }

    #[test]
    fn conversations_are_rows_with_the_replies_from_sent() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn();
        seed_conversation(&conn);

        let rows = list_messages(&conn, "inbox", true).unwrap();
        let hike = &rows[0];
        assert_eq!(hike.latest.id, "lena-again");
        assert_eq!(hike.message_ids, ["lena-again", "answer", "plan"]);
        // Three in the inbox and the reply from "Sent".
        assert_eq!(hike.count, 4);
        let senders: Vec<&str> = hike.senders.iter().map(|s| s.address.as_str()).collect();
        assert_eq!(
            senders,
            ["lena@example.org", "alex@example.org", "ben@example.org"]
        );
        assert!(!hike.latest.is_read, "one message is unread");
        assert!(hike.latest.is_flagged, "one message is flagged");
        // The mail to oneself counts once; the two older seeded mails stand alone.
        let note = rows.iter().find(|row| row.latest.id == "note").unwrap();
        assert_eq!(note.count, 1);
        assert_eq!(rows.len(), 4);

        // In "Sent" the replies are rows of their own, without the inbox's mail.
        let sent = list_messages(&conn, "sent", true).unwrap();
        let reply = sent.iter().find(|row| row.latest.id == "reply").unwrap();
        assert_eq!((reply.message_ids.len(), reply.count), (1, 1));

        // Without conversations every message is a row.
        let single = list_messages(&conn, "inbox", false).unwrap();
        assert_eq!(single.len(), 6);
        assert!(single.iter().all(|row| row.count == 1));
    }

    #[test]
    fn a_conversation_shows_its_folder_and_the_replies_newest_first() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn();
        seed_conversation(&conn);

        let ids = |message_id: &str, threads: bool| -> Vec<String> {
            get_conversation(&conn, message_id, threads)
                .unwrap()
                .into_iter()
                .map(|m| m.id)
                .collect()
        };
        assert_eq!(ids("plan", true), ["lena-again", "answer", "reply", "plan"]);
        assert_eq!(ids("plan", false), ["plan"]);
        // The copy in "Sent" is the same mail.
        assert_eq!(ids("note", true), ["note"]);
        // Mail on its way out leaves the conversation.
        conn.execute("UPDATE messages SET leaving = 1 WHERE id = 'answer'", [])
            .unwrap();
        assert_eq!(ids("plan", true), ["lena-again", "reply", "plan"]);
        assert!(ids("missing", true).is_empty());
    }

    #[test]
    fn search_results_are_whole_conversations() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn();
        seed_conversation(&conn);

        // Only the first mail mentions the meeting point; the row is the conversation.
        let rows = search_messages(&conn, "treffpunkt", true).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].latest.id, "lena-again");
        assert_eq!(rows[0].message_ids, ["lena-again", "answer", "plan"]);
        assert_eq!(rows[0].count, 4);

        let single = search_messages(&conn, "treffpunkt", false).unwrap();
        assert_eq!(single[0].latest.id, "plan");
        assert_eq!(single[0].message_ids, ["plan"]);

        // Found in the user's own reply: the conversation in the inbox, which shows it.
        let rows = search_messages(&conn, "dabei", true).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].latest.id, "lena-again");
        assert_eq!(rows[0].count, 4);
        // A reply in "Sent" alone stays there.
        conn.execute(
            "INSERT INTO messages (id, account_id, folder_id, uid, thread_id, subject, body_text,
               fetched_at)
             VALUES ('lonely', 'acc', 'sent', 20, 't9', 'Hallo', 'Niemand antwortet', 0)",
            [],
        )
        .unwrap();
        let rows = search_messages(&conn, "niemand", true).unwrap();
        assert_eq!(rows[0].latest.id, "lonely");
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
