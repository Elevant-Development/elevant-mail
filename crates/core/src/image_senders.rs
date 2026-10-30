//! Senders whose mail shows its external images when opened: "always load images from …". By
//! address in lower case. Loading the images of a single mail lasts only for the session and is
//! not kept here.

use rusqlite::{Connection, params};

use crate::ids::now_millis;
use crate::{Error, Result};

/// The addresses, alphabetically.
pub fn list(conn: &Connection) -> Result<Vec<String>> {
    Ok(conn
        .prepare_cached("SELECT address FROM image_senders ORDER BY address")?
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

pub fn allow(conn: &Connection, address: &str) -> Result<()> {
    let address = address.trim().to_lowercase();
    let valid = address
        .split_once('@')
        .is_some_and(|(local, domain)| !local.is_empty() && !domain.is_empty());
    if !valid {
        return Err(Error::InvalidEmail);
    }
    conn.execute(
        "INSERT INTO image_senders (address, added_at) VALUES (?1, ?2)
         ON CONFLICT (address) DO NOTHING",
        params![address, now_millis()],
    )?;
    Ok(())
}

pub fn forbid(conn: &Connection, address: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM image_senders WHERE address = ?1",
        [address.trim().to_lowercase()],
    )?;
    Ok(())
}

/// Whether the message comes from a sender whose images always load.
pub fn allowed_for(conn: &Connection, message_id: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS (
           SELECT 1 FROM messages m JOIN image_senders s ON s.address = lower(m.from_address)
           WHERE m.id = ?1
         )",
        [message_id],
        |row| row.get(0),
    )?)
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
               'tls', 'alex@example.org', 0);
             INSERT INTO folders (id, account_id, name, path, role)
             VALUES ('inbox', 'acc', 'INBOX', 'INBOX', 'inbox');
             INSERT INTO messages (id, account_id, folder_id, uid, from_address, fetched_at)
             VALUES ('news', 'acc', 'inbox', 1, 'Shop@Example.net', 0),
                    ('other', 'acc', 'inbox', 2, 'lena@example.org', 0);",
        )
        .unwrap();
    }

    #[test]
    fn a_sender_once_allowed_shows_images_in_all_its_mail() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn();
        seed(&conn);
        assert!(!allowed_for(&conn, "news").unwrap());

        allow(&conn, " shop@EXAMPLE.net ").unwrap();
        allow(&conn, "shop@example.net").unwrap();
        assert_eq!(list(&conn).unwrap(), ["shop@example.net"]);
        assert!(allowed_for(&conn, "news").unwrap());
        assert!(!allowed_for(&conn, "other").unwrap());
        assert!(!allowed_for(&conn, "missing").unwrap());

        forbid(&conn, "Shop@example.net").unwrap();
        assert!(list(&conn).unwrap().is_empty());
        assert!(!allowed_for(&conn, "news").unwrap());
    }

    #[test]
    fn only_addresses_are_kept() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn();
        for address in ["", "shop", "@example.net", "shop@"] {
            assert!(matches!(allow(&conn, address), Err(Error::InvalidEmail)));
        }
    }
}
