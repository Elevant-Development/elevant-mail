//! Reading folders and messages from the cache. The sync engine fills it; this only reads.

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, Row};

use crate::Result;
use crate::mail::sanitize::{HtmlView, sanitize};
use crate::model::{
    Address, Conversation, Folder, FolderRole, ListCursor, ListFilter, ListPage, Message,
    MessageSummary,
};
use crate::query::SearchQuery;

/// Mailbox id that shows the inboxes of all accounts together (same value as in the frontend).
pub const UNIFIED_INBOX: &str = "unified-inbox";

/// Rows per page of the list. The next page loads while scrolling towards the end, so a folder
/// of 50,000 mails opens as fast as one of 50.
const PAGE_SIZE: usize = 300;

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

/// How to page through the list, see [`list_messages`].
#[derive(Debug, Clone, Copy, Default)]
pub struct ListOptions<'a> {
    /// A row per conversation; without, every message is a row of its own.
    pub threads: bool,
    /// Each has to match.
    pub filters: &'a [ListFilter],
    /// Rows that stay although they no longer match the filters: the selected ones, so reading
    /// an unread mail does not pull it away under "unread".
    pub keep: &'a [String],
    /// The page after this one; the first page without.
    pub after: Option<&'a ListCursor>,
}

/// Leaves out every message but the newest of its conversation in the folder.
const NEWEST_IN_FOLDER: &str = "AND NOT EXISTS (
    SELECT 1 FROM messages n INDEXED BY idx_messages_thread
    WHERE n.account_id = m.account_id AND n.thread_id = m.thread_id AND n.folder_id = m.folder_id
      AND n.leaving = 0 AND n.by_search = 0
      AND (coalesce(n.date_sent, n.date_received, n.fetched_at), n.uid, n.id)
        > (coalesce(m.date_sent, m.date_received, m.fetched_at), m.uid, m.id))";

/// Ids as a JSON array, for `IN (SELECT value FROM json_each(?))`.
fn json_list(ids: &[impl AsRef<str>]) -> String {
    let ids: Vec<&str> = ids.iter().map(AsRef::as_ref).collect();
    serde_json::to_string(&ids).expect("a list of strings is valid JSON")
}

fn filter_condition(filter: ListFilter, alias: &str) -> String {
    let condition = match filter {
        ListFilter::Unread => "is_read = 0",
        ListFilter::Flagged => "is_flagged = 1",
        ListFilter::Attachment => "has_attachments = 1",
    };
    format!("{alias}.{condition}")
}

/// The rows of one folder in list order: with `threads` only each conversation's newest message.
/// Parameters: the folder, the rows to `keep` (with filters), the cursor's date twice, uid and id
/// (with `after`).
fn rows_sql(threads: bool, filters: &[ListFilter], after: bool) -> String {
    let newest = if threads { NEWEST_IN_FOLDER } else { "" };
    let mut matching = String::new();
    if !filters.is_empty() {
        // A conversation matches while one of its messages in the folder does.
        let each: Vec<String> = filters
            .iter()
            .map(|&filter| {
                let on_row = filter_condition(filter, "m");
                if !threads {
                    return on_row;
                }
                format!(
                    "({on_row} OR m.thread_id IS NOT NULL AND EXISTS (
                       SELECT 1 FROM messages x INDEXED BY idx_messages_thread
                       WHERE x.account_id = m.account_id AND x.thread_id = m.thread_id
                         AND x.folder_id = m.folder_id AND x.leaving = 0 AND x.by_search = 0
                         AND {}))",
                    filter_condition(filter, "x")
                )
            })
            .collect();
        matching = format!(
            "AND (m.id IN (SELECT value FROM json_each(?)) OR {})",
            each.join(" AND ")
        );
    }
    // The bound on the date alone lets the index start at the cursor.
    let position = if after {
        format!("AND {DATE} <= ? AND ({DATE}, m.uid, m.id) < (?, ?, ?)")
    } else {
        String::new()
    };
    // Mail on its way to another folder (moves::move_messages) no longer shows here, nor older
    // mail a search on the server fetched (`by_search`), until the sync gets that far. One row
    // more than the page tells whether another page follows.
    format!(
        "SELECT {SUMMARY_COLUMNS}, {MEMBER_COLUMNS}, m.uid FROM messages m
         WHERE m.folder_id = ? AND m.leaving = 0 AND m.by_search = 0 {newest} {matching} {position}
         ORDER BY {DATE} DESC, m.uid DESC, m.id DESC
         LIMIT {}",
        PAGE_SIZE + 1
    )
}

/// A page of the conversations of a folder, or of the unified inbox ([`UNIFIED_INBOX`]: the
/// inboxes of the accounts in it), newest first. A row is a conversation's newest message in the
/// folder; the page brings along the rest of each conversation, however old.
pub fn list_messages(
    conn: &Connection,
    mailbox_id: &str,
    options: ListOptions<'_>,
) -> Result<ListPage> {
    let ListOptions {
        threads,
        filters,
        keep,
        after,
    } = options;
    let folders: Vec<String> = if mailbox_id == UNIFIED_INBOX {
        conn.prepare_cached(
            "SELECT f.id FROM folders f JOIN accounts a ON a.id = f.account_id
             WHERE f.role = 'inbox' AND a.in_unified_inbox = 1",
        )?
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?
    } else {
        vec![mailbox_id.to_owned()]
    };
    // Each folder along its own index; the unified inbox merges them.
    let sql = rows_sql(threads, filters, after.is_some());
    let mut heads: Vec<(Member, u32)> = Vec::new();
    for folder_id in folders {
        let mut params: Vec<Value> = vec![folder_id.into()];
        if !filters.is_empty() {
            params.push(json_list(keep).into());
        }
        if let Some(after) = after {
            params.extend([
                after.date.into(),
                after.date.into(),
                after.uid.into(),
                after.id.clone().into(),
            ]);
        }
        let rows = conn
            .prepare_cached(&sql)?
            .query_map(rusqlite::params_from_iter(params), |row| {
                Ok((member_from_row(row)?, row.get(13)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        heads.extend(rows);
    }
    let order = |(member, uid): &(Member, u32)| {
        Reverse((member.summary.date_sent, *uid, member.summary.id.clone()))
    };
    heads.sort_by_cached_key(order);
    let next = if heads.len() > PAGE_SIZE {
        heads.truncate(PAGE_SIZE);
        heads.last().map(|(head, uid)| ListCursor {
            date: head.summary.date_sent,
            uid: *uid,
            id: head.summary.id.clone(),
        })
    } else {
        None
    };
    let heads: Vec<Member> = heads.into_iter().map(|(head, _)| head).collect();
    if !threads {
        return Ok(ListPage {
            rows: group(heads, Vec::new(), false),
            next,
        });
    }

    // The rest of each conversation in its folder, and the user's replies from "Sent".
    let mut threads_by_folder: HashMap<(&str, &str), Vec<&str>> = HashMap::new();
    let mut threads_by_account: HashMap<&str, Vec<&str>> = HashMap::new();
    for head in &heads {
        let account = head.summary.account_id.as_str();
        threads_by_folder
            .entry((account, &head.summary.folder_id))
            .or_default()
            .push(&head.thread);
        threads_by_account
            .entry(account)
            .or_default()
            .push(&head.thread);
    }
    let mut members_by_row: HashMap<(String, String), Vec<Member>> = HashMap::new();
    for ((account_id, folder_id), thread_ids) in threads_by_folder {
        let found = members(
            conn,
            &format!(
                "SELECT {SUMMARY_COLUMNS}, {MEMBER_COLUMNS} FROM messages m
                   INDEXED BY idx_messages_thread
                 WHERE m.account_id = ?1 AND m.thread_id IN (SELECT value FROM json_each(?2))
                   AND m.folder_id = ?3 AND m.leaving = 0 AND m.by_search = 0
                 ORDER BY {DATE} DESC, m.uid DESC, m.id DESC"
            ),
            [account_id, &json_list(&thread_ids), folder_id],
        )?;
        for member in found {
            let row = (member.summary.folder_id.clone(), member.thread.clone());
            members_by_row.entry(row).or_default().push(member);
        }
    }
    let mut replies = Vec::new();
    for (account_id, thread_ids) in threads_by_account {
        replies.extend(members(
            conn,
            &format!(
                "SELECT {SUMMARY_COLUMNS}, {MEMBER_COLUMNS} FROM messages m
                 JOIN folders f ON f.id = m.folder_id
                 WHERE m.account_id = ?1 AND m.thread_id IN (SELECT value FROM json_each(?2))
                   AND f.role = 'sent' AND m.leaving = 0"
            ),
            [account_id, &json_list(&thread_ids)],
        )?);
    }
    // In the order of the page, each conversation's newest message first: it stands for it.
    let mut listed = Vec::new();
    for head in heads {
        let row = (head.summary.folder_id.clone(), head.thread.clone());
        match members_by_row.remove(&row) {
            Some(members) => listed.extend(members),
            // Not in a conversation yet.
            None => listed.push(head),
        }
    }
    Ok(ListPage {
        rows: group(listed, replies, true),
        next,
    })
}

/// Search results shown at most; the newest come first.
const SEARCH_LIMIT: u32 = 300;

/// Full-text search over subject, sender and text of all cached mail, newest first. Every word
/// must match, as a word or the start of one ("rech" finds "Rechnung"); filters like `from:` or
/// `is:unread` narrow it down ([`SearchQuery`]). `also`: messages a search on the servers found,
/// e.g. by text deep in large mail that the cache only has the start of; they are listed too.
///
/// A message that sits in several folders of an account (Gmail shows labels and "All Mail" as
/// folders, other servers keep copies) is listed once, preferably from the inbox. With
/// `threads`, a result is the whole conversation in that folder, like in the list.
pub fn search_messages(
    conn: &Connection,
    input: &str,
    threads: bool,
    also: &[String],
) -> Result<Vec<Conversation>> {
    let query = SearchQuery::parse(input);
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let mut conditions = query.conditions();
    conditions.push("m.leaving = 0");
    let filters = conditions.join(" AND ");
    let (source, text, fts) = match query.fts() {
        Some(fts) => (
            "messages_fts JOIN messages m ON m.seq = messages_fts.rowid",
            "messages_fts MATCH ?1 AND",
            vec![fts],
        ),
        None => ("messages m", "", Vec::new()),
    };
    let mut hits = members(
        conn,
        &format!(
            "SELECT {SUMMARY_COLUMNS}, {MEMBER_COLUMNS} FROM (
               SELECT m.*, row_number() OVER (
                 PARTITION BY m.account_id, coalesce(m.message_id, m.id)
                 ORDER BY f.role = 'inbox' DESC, f.role IS NOT NULL AND f.role <> 'all' DESC
               ) AS copy
               FROM {source}
               JOIN folders f ON f.id = m.folder_id
               WHERE {text} {filters}
             ) m
             WHERE m.copy = 1
             ORDER BY {DATE} DESC, m.uid DESC
             LIMIT {SEARCH_LIMIT}"
        ),
        rusqlite::params_from_iter(fts),
    )?;
    if !also.is_empty() {
        // Each mail once, also where the cache holds a copy in another folder.
        let copy = |member: &Member| {
            let account = member.summary.account_id.clone();
            member.header_id.clone().map(|id| (account, id))
        };
        let mut ids: HashSet<String> = hits.iter().map(|hit| hit.summary.id.clone()).collect();
        let mut copies: HashSet<(String, String)> = hits.iter().filter_map(copy).collect();
        let placeholders = vec!["?"; also.len()].join(", ");
        let found = members(
            conn,
            &format!(
                "SELECT {SUMMARY_COLUMNS}, {MEMBER_COLUMNS} FROM messages m
                 WHERE m.id IN ({placeholders}) AND {filters}"
            ),
            rusqlite::params_from_iter(also),
        )?;
        for member in found {
            let new_copy = copy(&member).is_none_or(|key| copies.insert(key));
            if ids.insert(member.summary.id.clone()) && new_copy {
                hits.push(member);
            }
        }
        hits.sort_by_key(|hit| Reverse(hit.summary.date_sent));
    }
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
                remote_images_loaded: false,
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
        message.remote_images_loaded = view.remote_images;
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

    /// The first page of the list.
    fn rows(conn: &Connection, mailbox_id: &str, threads: bool) -> Vec<Conversation> {
        let options = ListOptions {
            threads,
            ..Default::default()
        };
        list_messages(conn, mailbox_id, options).unwrap().rows
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

        let in_folder = rows(&conn, &inbox, true);
        assert_eq!(
            in_folder
                .iter()
                .map(|row| row.latest.subject.as_deref().unwrap())
                .collect::<Vec<_>>(),
            ["Newer", "Older"]
        );
        assert_eq!(in_folder[0].latest.id, newest);

        // The unified inbox leaves out sent mail.
        assert_eq!(rows(&conn, UNIFIED_INBOX, true).len(), 2);
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
            search_messages(&conn, query, true, &[])
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
        let mut ids: Vec<String> = search_messages(&conn, "quartal", false, &[])
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

        let listed = rows(&conn, "inbox", true);
        let hike = &listed[0];
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
        let note = listed.iter().find(|row| row.latest.id == "note").unwrap();
        assert_eq!(note.count, 1);
        assert_eq!(listed.len(), 4);

        // In "Sent" the replies are rows of their own, without the inbox's mail.
        let sent = rows(&conn, "sent", true);
        let reply = sent.iter().find(|row| row.latest.id == "reply").unwrap();
        assert_eq!((reply.message_ids.len(), reply.count), (1, 1));

        // Without conversations every message is a row.
        let single = rows(&conn, "inbox", false);
        assert_eq!(single.len(), 6);
        assert!(single.iter().all(|row| row.count == 1));
    }

    /// Every page in turn, until the last.
    fn all_pages(conn: &Connection, mailbox_id: &str, options: ListOptions<'_>) -> Vec<ListPage> {
        let mut pages = vec![list_messages(conn, mailbox_id, options).unwrap()];
        while let Some(after) = pages.last().unwrap().next.clone() {
            let next = ListOptions {
                after: Some(&after),
                ..options
            };
            pages.push(list_messages(conn, mailbox_id, next).unwrap());
        }
        pages
    }

    /// `count` messages in the inbox, one a minute, every three a conversation; the third of
    /// each (the oldest) unread.
    fn seed_many(conn: &Connection, count: u32) {
        seed(conn);
        conn.execute("DELETE FROM messages", []).unwrap();
        for uid in 1..=count {
            conn.execute(
                "INSERT INTO messages (id, account_id, folder_id, uid, thread_id, subject, from_name,
                   from_address, date_sent, is_read, fetched_at)
                 VALUES (?1, 'acc', 'inbox', ?2, ?3, 'Hallo', 'Lena', 'lena@example.org', ?4, ?5, 0)",
                params![
                    format!("m{uid}"),
                    uid,
                    format!("t{}", (uid - 1) / 3),
                    i64::from(uid) * 60_000,
                    (uid - 1) % 3 != 0
                ],
            )
            .unwrap();
        }
    }

    #[test]
    fn pages_through_a_large_folder_newest_first() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn();
        seed_many(&conn, 1000);

        for threads in [false, true] {
            let options = ListOptions {
                threads,
                ..Default::default()
            };
            let pages = all_pages(&conn, "inbox", options);
            let sizes: Vec<usize> = pages.iter().map(|page| page.rows.len()).collect();
            let listed: Vec<&Conversation> = pages.iter().flat_map(|page| &page.rows).collect();
            let dates: Vec<i64> = listed.iter().map(|row| row.latest.date_sent).collect();
            assert!(dates.is_sorted_by(|a, b| a > b), "newest first");
            // Every message once, a conversation in one row even across a page's end.
            let mut ids: Vec<&String> = listed.iter().flat_map(|row| &row.message_ids).collect();
            assert_eq!(ids.len(), 1000);
            ids.sort();
            ids.dedup();
            assert_eq!(ids.len(), 1000);
            if threads {
                assert_eq!(sizes, [300, 34]);
                assert_eq!(listed[0].message_ids, ["m1000"]);
                assert_eq!(listed[1].message_ids, ["m999", "m998", "m997"]);
            } else {
                assert_eq!(sizes, [300, 300, 300, 100]);
            }
        }

        // The unified inbox pages the same way.
        let unified = all_pages(&conn, UNIFIED_INBOX, ListOptions::default());
        assert_eq!(
            unified.iter().map(|page| page.rows.len()).sum::<usize>(),
            1000
        );
    }

    #[test]
    fn the_unified_inbox_merges_the_accounts_newest_first() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn();
        seed_many(&conn, 400);
        conn.execute_batch(
            "INSERT INTO accounts (id, display_name, email_address, auth_method, imap_host,
               imap_port, imap_security, imap_username, created_at)
             VALUES ('work', 'Work', 'alex@work.example', 'password', 'imap.work.example', 993,
               'tls', 'alex', 0);
             INSERT INTO folders (id, account_id, name, path, role)
             VALUES ('work-inbox', 'work', 'INBOX', 'INBOX', 'inbox');",
        )
        .unwrap();
        for uid in 1..=400u32 {
            conn.execute(
                "INSERT INTO messages (id, account_id, folder_id, uid, subject, from_name,
                   from_address, date_sent, fetched_at)
                 VALUES (?1, 'work', 'work-inbox', ?2, 'Bericht', 'Jana', 'jana@work.example', ?3, 0)",
                params![format!("w{uid}"), uid, i64::from(uid) * 60_000 + 30_000],
            )
            .unwrap();
        }

        let pages = all_pages(&conn, UNIFIED_INBOX, ListOptions::default());
        let listed: Vec<&str> = pages
            .iter()
            .flat_map(|page| &page.rows)
            .map(|row| row.latest.id.as_str())
            .collect();
        assert_eq!(listed.len(), 800);
        assert_eq!(listed[..4], ["w400", "m400", "w399", "m399"]);
        assert_eq!(listed[listed.len() - 2..], ["w1", "m1"]);

        // An account taken out of the unified inbox keeps only its own inbox.
        crate::accounts::set_in_unified_inbox(&conn, "work", false).unwrap();
        let first = list_messages(&conn, UNIFIED_INBOX, ListOptions::default()).unwrap();
        assert!(first.rows.iter().all(|row| row.latest.account_id == "acc"));
        assert_eq!(rows(&conn, "work-inbox", false).len(), 300);
    }

    #[test]
    fn filters_match_any_message_of_a_conversation_and_keep_the_selected() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn();
        seed_many(&conn, 9);
        conn.execute("UPDATE messages SET is_flagged = 1 WHERE id = 'm5'", [])
            .unwrap();

        let listed = |threads: bool, filters: &[ListFilter], keep: &[String]| -> Vec<String> {
            let options = ListOptions {
                threads,
                filters,
                keep,
                after: None,
            };
            list_messages(&conn, "inbox", options)
                .unwrap()
                .rows
                .into_iter()
                .map(|row| row.latest.id)
                .collect()
        };
        // Each conversation has an unread message, its oldest.
        assert_eq!(listed(true, &[ListFilter::Unread], &[]), ["m9", "m6", "m3"]);
        assert_eq!(
            listed(false, &[ListFilter::Unread], &[]),
            ["m7", "m4", "m1"]
        );
        assert_eq!(listed(true, &[ListFilter::Flagged], &[]), ["m6"]);
        assert_eq!(
            listed(true, &[ListFilter::Unread, ListFilter::Flagged], &[]),
            ["m6"]
        );
        assert!(listed(true, &[ListFilter::Attachment], &[]).is_empty());

        // Read now, but still selected: it stays.
        conn.execute("UPDATE messages SET is_read = 1 WHERE id = 'm7'", [])
            .unwrap();
        assert_eq!(listed(true, &[ListFilter::Unread], &[]), ["m6", "m3"]);
        assert_eq!(
            listed(true, &[ListFilter::Unread], &["m9".into()]),
            ["m9", "m6", "m3"]
        );
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
        let rows = search_messages(&conn, "treffpunkt", true, &[]).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].latest.id, "lena-again");
        assert_eq!(rows[0].message_ids, ["lena-again", "answer", "plan"]);
        assert_eq!(rows[0].count, 4);

        let single = search_messages(&conn, "treffpunkt", false, &[]).unwrap();
        assert_eq!(single[0].latest.id, "plan");
        assert_eq!(single[0].message_ids, ["plan"]);

        // Found in the user's own reply: the conversation in the inbox, which shows it.
        let rows = search_messages(&conn, "dabei", true, &[]).unwrap();
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
        let rows = search_messages(&conn, "niemand", true, &[]).unwrap();
        assert_eq!(rows[0].latest.id, "lonely");
    }

    #[test]
    fn search_filters_by_sender_state_and_attachment() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn();
        seed_conversation(&conn);
        // Only the conversation and the note, without the unread mail from the first seed.
        conn.execute_batch(
            "DELETE FROM messages WHERE thread_id IS NULL;
             UPDATE messages SET has_attachments = 1 WHERE id = 'plan';",
        )
        .unwrap();

        let ids = |query: &str, also: &[&str]| -> Vec<String> {
            let also: Vec<String> = also.iter().map(|id| (*id).to_owned()).collect();
            let mut ids: Vec<String> = search_messages(&conn, query, false, &also)
                .unwrap()
                .into_iter()
                .map(|row| row.latest.id)
                .collect();
            ids.sort();
            ids
        };
        // Filters alone, without a word.
        assert_eq!(ids("ist:ungelesen", &[]), ["answer"]);
        assert_eq!(ids("is:flagged", &[]), ["answer"]);
        assert_eq!(ids("hat:anhang", &[]), ["plan"]);
        // A sender by name or address, upper or lower case.
        assert_eq!(ids("von:lena", &[]), ["lena-again", "plan"]);
        assert_eq!(ids("from:lena@example.org schön", &[]), ["lena-again"]);
        assert_eq!(ids("von:ben wanderung", &[]), ["answer"]);
        assert!(ids("von:ben treffpunkt", &[]).is_empty());

        // What the servers found is listed too, if it passes the filters, and once.
        assert_eq!(ids("nirgends", &["plan"]), ["plan"]);
        assert!(ids("is:unread", &["plan"]).contains(&"answer".to_owned()));
        assert!(!ids("is:unread", &["plan"]).contains(&"plan".to_owned()));
        assert_eq!(ids("milch", &["note-sent"]), ["note"]);
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
