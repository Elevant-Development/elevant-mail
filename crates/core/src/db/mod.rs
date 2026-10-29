//! The local cache: one SQLite file per installation. The UI reads only from here; the sync
//! engine writes here.

mod migrations;

use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use rusqlite::Connection;

use crate::Result;

pub use migrations::{latest_version, schema_version};

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Opens (or creates) the database file and brings the schema up to date.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
            restrict_to_owner(parent, 0o700)?;
        }
        let conn = Connection::open(path)?;
        // Before WAL is switched on: SQLite gives the -wal and -shm files the database's mode.
        for file in [
            path.to_owned(),
            path.with_extension("sqlite3-wal"),
            path.with_extension("sqlite3-shm"),
        ] {
            if file.exists() {
                restrict_to_owner(&file, 0o600)?;
            }
        }
        // WAL lets readers (the UI, later the MCP server) work while the sync engine writes.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Self::init(conn)
    }

    /// Fresh database in memory, for tests.
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        migrations::migrate(&mut conn)?;
        // Mail cached before conversations existed, the first time after the update.
        let tx = conn.transaction()?;
        let placed = crate::threads::assign_pending(&tx)?;
        tx.commit()?;
        if placed > 0 {
            tracing::info!(placed, "placed cached mail in conversations");
        }
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Exclusive access to the connection. A panic in an earlier holder leaves SQLite itself
    /// consistent (open transactions roll back), so a poisoned lock is safe to reuse.
    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn schema_version(&self) -> Result<u32> {
        schema_version(&self.conn())
    }
}

/// The cache holds mail content, so on Unix only the owner may read it. On Windows the file
/// lives in the user's profile, which is private already.
#[cfg(unix)]
fn restrict_to_owner(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_to_owner(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use rusqlite::params;

    use super::*;
    use crate::Error;
    use crate::ids::{new_id, now_millis};

    fn insert_account(conn: &Connection) -> String {
        let id = new_id();
        conn.execute(
            "INSERT INTO accounts (id, display_name, email_address, auth_method, imap_host,
               imap_port, imap_security, imap_username, created_at)
             VALUES (?1, 'Personal', 'alex@example.org', 'password', 'imap.example.org', 993, 'tls',
               'alex@example.org', ?2)",
            params![id, now_millis()],
        )
        .unwrap();
        id
    }

    fn insert_folder(conn: &Connection, account_id: &str) -> String {
        let id = new_id();
        conn.execute(
            "INSERT INTO folders (id, account_id, name, path, role)
             VALUES (?1, ?2, 'INBOX', 'INBOX', 'inbox')",
            params![id, account_id],
        )
        .unwrap();
        id
    }

    fn insert_message(
        conn: &Connection,
        account_id: &str,
        folder_id: &str,
        uid: u32,
        subject: &str,
        body: &str,
    ) -> String {
        let id = new_id();
        conn.execute(
            "INSERT INTO messages (id, account_id, folder_id, uid, subject, from_name,
               from_address, body_text, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'Lena Müller', 'lena@example.org', ?6, ?7)",
            params![id, account_id, folder_id, uid, subject, body, now_millis()],
        )
        .unwrap();
        id
    }

    fn search(conn: &Connection, query: &str) -> Vec<String> {
        let mut statement = conn
            .prepare(
                "SELECT m.id FROM messages_fts
                 JOIN messages m ON m.seq = messages_fts.rowid
                 WHERE messages_fts MATCH ?1 ORDER BY rank",
            )
            .unwrap();
        statement
            .query_map([query], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    fn fts_rows(conn: &Connection) -> i64 {
        // The index keeps one docsize row per indexed message.
        conn.query_row("SELECT count(*) FROM messages_fts_docsize", [], |row| {
            row.get(0)
        })
        .unwrap()
    }

    #[test]
    fn migrates_a_fresh_database_to_the_latest_version() {
        let db = Database::open_in_memory().unwrap();
        assert_eq!(db.schema_version().unwrap(), latest_version());
        let foreign_keys: bool = db
            .conn()
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap();
        assert!(foreign_keys);
    }

    #[test]
    fn reopening_keeps_data_and_uses_wal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("mail.sqlite3");
        let account_id = {
            let db = Database::open(&path).unwrap();
            insert_account(&db.conn())
        };
        let db = Database::open(&path).unwrap();
        let conn = db.conn();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM accounts WHERE id = ?1",
                [&account_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
        let journal: String = conn
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        assert_eq!(journal, "wal");
    }

    #[cfg(unix)]
    #[test]
    fn database_files_are_private_to_the_owner() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("data");
        let path = folder.join("mail.sqlite3");
        let db = Database::open(&path).unwrap();
        insert_account(&db.conn());
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&folder), 0o700);
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(&path.with_extension("sqlite3-wal")), 0o600);
    }

    #[test]
    fn refuses_a_schema_from_a_newer_app() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mail.sqlite3");
        drop(Database::open(&path).unwrap());
        Connection::open(&path)
            .unwrap()
            .pragma_update(None, "user_version", latest_version() + 1)
            .unwrap();
        assert!(matches!(
            Database::open(&path),
            Err(Error::SchemaTooNew { .. })
        ));
    }

    #[test]
    fn full_text_search_ignores_diacritics_and_supports_prefixes() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn();
        let account = insert_account(&conn);
        let folder = insert_folder(&conn, &account);
        let hike = insert_message(
            &conn,
            &account,
            &folder,
            1,
            "Hiking on Saturday",
            "Meet at the station",
        );
        insert_message(
            &conn,
            &account,
            &folder,
            2,
            "Invoice 2026-1042",
            "Coffee at Café Noir",
        );

        // Both test messages come from "Lena Müller".
        assert_eq!(search(&conn, "muller").len(), 2);
        assert_eq!(search(&conn, "hik*"), vec![hike.clone()]);
        assert_eq!(search(&conn, "station"), vec![hike]);
        assert_eq!(search(&conn, "cafe").len(), 1);
    }

    #[test]
    fn full_text_index_follows_updates_and_deletes() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn();
        let account = insert_account(&conn);
        let folder = insert_folder(&conn, &account);
        let id = insert_message(&conn, &account, &folder, 1, "Old subject", "Body");

        conn.execute(
            "UPDATE messages SET subject = 'New subject' WHERE id = ?1",
            [&id],
        )
        .unwrap();
        assert!(search(&conn, "old").is_empty());
        assert_eq!(search(&conn, "new"), vec![id.clone()]);

        // Flag changes must not touch the index.
        conn.execute("UPDATE messages SET is_read = 1 WHERE id = ?1", [&id])
            .unwrap();
        assert_eq!(fts_rows(&conn), 1);

        conn.execute("DELETE FROM messages WHERE id = ?1", [&id])
            .unwrap();
        assert!(search(&conn, "new").is_empty());
        assert_eq!(fts_rows(&conn), 0);
    }

    #[test]
    fn removing_an_account_removes_its_data_and_index_entries() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn();
        let account = insert_account(&conn);
        let folder = insert_folder(&conn, &account);
        let message = insert_message(&conn, &account, &folder, 1, "Agenda", "Topics");
        conn.execute(
            "INSERT INTO pending_actions (account_id, message_id, action, created_at)
             VALUES (?1, ?2, 'set_read', ?3)",
            params![account, message, now_millis()],
        )
        .unwrap();

        conn.execute("DELETE FROM accounts WHERE id = ?1", [&account])
            .unwrap();

        for table in ["folders", "messages", "pending_actions"] {
            let count: i64 = conn
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0, "{table} should be empty");
        }
        assert_eq!(fts_rows(&conn), 0);
    }

    #[test]
    fn a_message_uid_is_unique_per_folder() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn();
        let account = insert_account(&conn);
        let folder = insert_folder(&conn, &account);
        insert_message(&conn, &account, &folder, 7, "One", "");
        let duplicate = conn.execute(
            "INSERT INTO messages (id, account_id, folder_id, uid, fetched_at) VALUES (?1, ?2, ?3, 7, 0)",
            params![new_id(), account, folder],
        );
        assert!(duplicate.is_err());
    }
}
