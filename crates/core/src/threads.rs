//! Conversations. Messages that answer each other share a `thread_id`: a reply names the mail it
//! answers (In-Reply-To) and the conversation before it (References). Every Message-ID a
//! conversation has seen is kept in `thread_keys`, so a reply finds its conversation even when it
//! arrives before the mail it answers, and a message that links two conversations joins them.
//! A reply without these headers ("Re: …" from programs that leave them out) joins the latest
//! conversation with the same subject from the weeks before.
//!
//! Conversations belong to one account and span its folders; the list shows them per folder.

use rusqlite::{Connection, OptionalExtension, params};

use crate::Result;
use crate::ids::new_id;

/// How far back a reply without headers looks for its conversation by subject.
const SUBJECT_WINDOW_MS: i64 = 60 * 24 * 60 * 60 * 1000;

/// Prefixes that mark a reply, in the languages mail programs use ("AW:" is German Outlook).
const REPLY_PREFIXES: &[&str] = &["re", "aw", "antw", "antwort", "sv", "vs", "odp", "rif", "r"];
/// Forwarded mail keeps the conversation's subject, but starts a conversation of its own.
const FORWARD_PREFIXES: &[&str] = &["fw", "fwd", "wg", "tr", "rv", "enc"];

struct Unthreaded {
    seq: i64,
    account_id: String,
    message_id: Option<String>,
    in_reply_to: Option<String>,
    references: Option<String>,
    subject: Option<String>,
    date: i64,
}

/// Puts every message without a conversation into one: new mail after a sync, the whole cache
/// once after the update that brought conversations. Oldest first, so a reply comes after the
/// mail it answers. Returns how many messages were placed.
pub(crate) fn assign_pending(conn: &Connection) -> Result<usize> {
    let pending = conn
        .prepare(
            "SELECT seq, account_id, message_id, in_reply_to, references_header, subject,
               coalesce(date_sent, date_received, fetched_at)
             FROM messages WHERE thread_id IS NULL
             ORDER BY coalesce(date_sent, date_received, fetched_at), seq",
        )?
        .query_map([], |row| {
            Ok(Unthreaded {
                seq: row.get(0)?,
                account_id: row.get(1)?,
                message_id: row.get(2)?,
                in_reply_to: row.get(3)?,
                references: row.get(4)?,
                subject: row.get(5)?,
                date: row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for message in &pending {
        assign(conn, message)?;
    }
    Ok(pending.len())
}

fn assign(conn: &Connection, message: &Unthreaded) -> Result<()> {
    let keys = header_keys(message);
    let mut found: Vec<String> = Vec::new();
    {
        let mut lookup = conn.prepare_cached(
            "SELECT thread_id FROM thread_keys WHERE account_id = ?1 AND key = ?2",
        )?;
        for key in &keys {
            let thread: Option<String> = lookup
                .query_row(params![message.account_id, key], |row| row.get(0))
                .optional()?;
            if let Some(thread) = thread.filter(|thread| !found.contains(thread)) {
                found.push(thread);
            }
        }
    }

    let (subject, reply) = message
        .subject
        .as_deref()
        .map(normalize_subject)
        .unwrap_or_default();
    let subject = Some(subject).filter(|subject| !subject.is_empty());
    if found.is_empty()
        && reply
        && let Some(subject) = &subject
    {
        let thread: Option<String> = conn
            .query_row(
                "SELECT thread_id FROM messages
                 WHERE account_id = ?1 AND thread_subject = ?2 AND thread_id IS NOT NULL
                   AND coalesce(date_sent, date_received, fetched_at) BETWEEN ?3 - ?4 AND ?3
                 ORDER BY coalesce(date_sent, date_received, fetched_at) DESC LIMIT 1",
                params![message.account_id, subject, message.date, SUBJECT_WINDOW_MS],
                |row| row.get(0),
            )
            .optional()?;
        found.extend(thread);
    }

    // The oldest conversation takes in the others, so ids stay put where they can.
    found.sort();
    let thread = found.first().cloned().unwrap_or_else(new_id);
    for joined in found.iter().skip(1) {
        conn.execute(
            "UPDATE messages SET thread_id = ?3 WHERE account_id = ?1 AND thread_id = ?2",
            params![message.account_id, joined, thread],
        )?;
        conn.execute(
            "UPDATE thread_keys SET thread_id = ?3 WHERE account_id = ?1 AND thread_id = ?2",
            params![message.account_id, joined, thread],
        )?;
    }
    conn.execute(
        "UPDATE messages SET thread_id = ?2, thread_subject = ?3 WHERE seq = ?1",
        params![message.seq, thread, subject],
    )?;
    let mut remember = conn.prepare_cached(
        "INSERT INTO thread_keys (account_id, key, thread_id) VALUES (?1, ?2, ?3)
         ON CONFLICT (account_id, key) DO UPDATE SET thread_id = excluded.thread_id",
    )?;
    for key in &keys {
        remember.execute(params![message.account_id, key, thread])?;
    }
    Ok(())
}

/// The message's own Message-ID and the ones it answers, each once, without angle brackets.
fn header_keys(message: &Unthreaded) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    let headers = [
        &message.message_id,
        &message.in_reply_to,
        &message.references,
    ];
    for id in headers
        .into_iter()
        .flatten()
        .flat_map(|header| header.split_whitespace())
    {
        let id = id.trim_matches(|c| c == '<' || c == '>' || c == ',');
        if !id.is_empty() && !keys.iter().any(|key| key == id) {
            keys.push(id.to_owned());
        }
    }
    keys
}

/// "AW: Re[2]: WG:  Wanderung am Samstag" → ("wanderung am samstag", true): the subject without
/// reply and forward prefixes, in lower case, and whether the first prefix marked a reply.
pub(crate) fn normalize_subject(subject: &str) -> (String, bool) {
    let mut rest = subject.trim();
    let mut reply = None;
    while let Some((prefix, after)) = rest.split_once(':') {
        // "Re", "Re[2]" or "Re (2)", but not "Reise: Plan".
        let word = prefix
            .trim_end()
            .trim_end_matches(|c: char| c.is_ascii_digit() || "[]() ".contains(c))
            .to_lowercase();
        let is_reply = REPLY_PREFIXES.contains(&word.as_str());
        if !is_reply && !FORWARD_PREFIXES.contains(&word.as_str()) {
            break;
        }
        reply.get_or_insert(is_reply);
        rest = after.trim_start();
    }
    let normalized = rest
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    (normalized, reply.unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn seed(conn: &Connection) {
        conn.execute_batch(
            "INSERT INTO accounts (id, display_name, email_address, auth_method, imap_host,
               imap_port, imap_security, imap_username, created_at)
             VALUES ('acc', 'Personal', 'alex@example.org', 'password', 'imap.example.org', 993,
               'tls', 'alex@example.org', 0),
               ('other', 'Work', 'alex@example.com', 'password', 'imap.example.com', 993,
               'tls', 'alex@example.com', 0);
             INSERT INTO folders (id, account_id, name, path, role)
             VALUES ('inbox', 'acc', 'INBOX', 'INBOX', 'inbox'),
               ('sent', 'acc', 'Sent', 'Sent', 'sent'),
               ('other-inbox', 'other', 'INBOX', 'INBOX', 'inbox');",
        )
        .unwrap();
    }

    struct Mail<'a> {
        id: &'a str,
        folder: &'a str,
        message_id: Option<&'a str>,
        in_reply_to: Option<&'a str>,
        references: Option<&'a str>,
        subject: &'a str,
        day: i64,
    }

    impl<'a> Mail<'a> {
        fn new(id: &'a str, subject: &'a str, day: i64) -> Self {
            Self {
                id,
                folder: "inbox",
                message_id: None,
                in_reply_to: None,
                references: None,
                subject,
                day,
            }
        }
        fn message_id(mut self, value: &'a str) -> Self {
            self.message_id = Some(value);
            self
        }
        fn answers(mut self, value: &'a str) -> Self {
            self.in_reply_to = Some(value);
            self
        }
        fn references(mut self, value: &'a str) -> Self {
            self.references = Some(value);
            self
        }
        fn folder(mut self, value: &'a str) -> Self {
            self.folder = value;
            self
        }
    }

    const DAY: i64 = 24 * 60 * 60 * 1000;

    /// Stores the mail and places it, as a sync does.
    fn deliver(conn: &Connection, mail: Mail<'_>) {
        let account = if mail.folder.starts_with("other") {
            "other"
        } else {
            "acc"
        };
        conn.execute(
            "INSERT INTO messages (id, account_id, folder_id, uid, message_id, in_reply_to,
               references_header, subject, date_sent, fetched_at)
             VALUES (?1, ?2, ?3, (SELECT count(*) + 1 FROM messages), ?4, ?5, ?6, ?7, ?8, 0)",
            params![
                mail.id,
                account,
                mail.folder,
                mail.message_id,
                mail.in_reply_to,
                mail.references,
                mail.subject,
                mail.day * DAY
            ],
        )
        .unwrap();
        assign_pending(conn).unwrap();
    }

    fn thread_of(conn: &Connection, id: &str) -> String {
        conn.query_row(
            "SELECT thread_id FROM messages WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .unwrap()
    }

    fn database() -> Database {
        let db = Database::open_in_memory().unwrap();
        seed(&db.conn());
        db
    }

    #[test]
    fn replies_join_the_mail_they_answer() {
        let db = database();
        let conn = db.conn();
        deliver(
            &conn,
            Mail::new("plan", "Wanderung", 1).message_id("plan@x"),
        );
        deliver(
            &conn,
            Mail::new("reply", "Re: Wanderung", 2)
                .message_id("reply@x")
                .answers("plan@x")
                .references("plan@x")
                .folder("sent"),
        );
        // Answers the reply only, and changes the subject: the headers still tell.
        deliver(
            &conn,
            Mail::new("later", "Treffpunkt", 3)
                .message_id("later@x")
                .answers("<reply@x>"),
        );
        deliver(
            &conn,
            Mail::new("other", "Wanderung", 4).message_id("other@x"),
        );

        let thread = thread_of(&conn, "plan");
        assert_eq!(thread_of(&conn, "reply"), thread);
        assert_eq!(thread_of(&conn, "later"), thread);
        assert_ne!(thread_of(&conn, "other"), thread);
    }

    #[test]
    fn a_reply_before_its_mail_is_joined_later() {
        let db = database();
        let conn = db.conn();
        // Two replies to a mail not in the cache yet, then the mail itself.
        deliver(
            &conn,
            Mail::new("first", "Re: Budget", 2)
                .message_id("first@x")
                .answers("budget@x"),
        );
        deliver(
            &conn,
            Mail::new("second", "Re: Budget", 3)
                .message_id("second@x")
                .references("budget@x first@x"),
        );
        deliver(
            &conn,
            Mail::new("budget", "Budget", 1).message_id("budget@x"),
        );

        let thread = thread_of(&conn, "first");
        assert_eq!(thread_of(&conn, "second"), thread);
        assert_eq!(thread_of(&conn, "budget"), thread);
    }

    #[test]
    fn a_message_linking_two_conversations_joins_them() {
        let db = database();
        let conn = db.conn();
        deliver(&conn, Mail::new("a", "Plan A", 1).message_id("a@x"));
        deliver(&conn, Mail::new("b", "Plan B", 2).message_id("b@x"));
        assert_ne!(thread_of(&conn, "a"), thread_of(&conn, "b"));

        deliver(
            &conn,
            Mail::new("both", "Re: Pläne", 3)
                .message_id("both@x")
                .references("a@x b@x"),
        );
        let thread = thread_of(&conn, "a");
        assert_eq!(thread_of(&conn, "b"), thread);
        assert_eq!(thread_of(&conn, "both"), thread);
        // Later mail answering either one lands there too.
        deliver(
            &conn,
            Mail::new("next", "Re: Plan B", 4)
                .message_id("next@x")
                .answers("b@x"),
        );
        assert_eq!(thread_of(&conn, "next"), thread);
    }

    #[test]
    fn replies_without_headers_join_by_subject() {
        let db = database();
        let conn = db.conn();
        deliver(&conn, Mail::new("old", "Rechnung", 1).message_id("old@x"));
        deliver(
            &conn,
            Mail::new("invoice", "Rechnung", 40).message_id("invoice@x"),
        );
        deliver(
            &conn,
            Mail::new("reply", "AW: Rechnung", 41).message_id("reply@x"),
        );
        // The same subject without "AW:" is a new mail, not a reply.
        deliver(
            &conn,
            Mail::new("next", "Rechnung", 42).message_id("next@x"),
        );
        // Too long after the last one.
        deliver(
            &conn,
            Mail::new("late", "Re: Rechnung", 200).message_id("late@x"),
        );
        // Another account never shares conversations.
        deliver(
            &conn,
            Mail::new("elsewhere", "Re: Rechnung", 41)
                .message_id("elsewhere@x")
                .folder("other-inbox"),
        );

        assert_ne!(thread_of(&conn, "old"), thread_of(&conn, "invoice"));
        assert_eq!(thread_of(&conn, "reply"), thread_of(&conn, "invoice"));
        assert_ne!(thread_of(&conn, "next"), thread_of(&conn, "invoice"));
        assert_ne!(thread_of(&conn, "late"), thread_of(&conn, "next"));
        assert_ne!(thread_of(&conn, "elsewhere"), thread_of(&conn, "invoice"));
    }

    #[test]
    fn copies_in_several_folders_share_the_conversation() {
        let db = database();
        let conn = db.conn();
        deliver(
            &conn,
            Mail::new("inbox-copy", "Notiz", 1).message_id("note@x"),
        );
        deliver(
            &conn,
            Mail::new("sent-copy", "Notiz", 1)
                .message_id("note@x")
                .folder("sent"),
        );
        assert_eq!(
            thread_of(&conn, "inbox-copy"),
            thread_of(&conn, "sent-copy")
        );
    }

    #[test]
    fn the_cache_from_before_is_placed_oldest_first() {
        let db = database();
        let conn = db.conn();
        // As left by an older version: nothing placed yet, the reply stored first.
        conn.execute_batch(
            "INSERT INTO messages (id, account_id, folder_id, uid, subject, date_sent, fetched_at)
             VALUES ('reply', 'acc', 'inbox', 2, 'Re: Urlaub', 86400000, 0),
                    ('mail', 'acc', 'inbox', 1, 'Urlaub', 0, 0);",
        )
        .unwrap();
        assert_eq!(assign_pending(&conn).unwrap(), 2);
        assert_eq!(thread_of(&conn, "reply"), thread_of(&conn, "mail"));
        assert_eq!(assign_pending(&conn).unwrap(), 0);
    }

    #[test]
    fn subjects_lose_reply_and_forward_prefixes() {
        assert_eq!(
            normalize_subject("AW: Re[2]: WG:  Wanderung am  Samstag "),
            ("wanderung am samstag".into(), true)
        );
        assert_eq!(
            normalize_subject("WG: Rechnung"),
            ("rechnung".into(), false)
        );
        assert_eq!(normalize_subject("Re (3): Plan"), ("plan".into(), true));
        assert_eq!(normalize_subject("Rechnung"), ("rechnung".into(), false));
        assert_eq!(
            normalize_subject("Reise: Plan"),
            ("reise: plan".into(), false)
        );
        assert_eq!(normalize_subject("RE:"), (String::new(), true));
    }
}
