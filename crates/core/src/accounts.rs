//! Adding, listing and removing accounts. An account is only saved after a successful login;
//! its password goes to the keyring, everything else to the database.

use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use ts_rs::TS;

use crate::db::Database;
use crate::ids::{new_id, now_millis};
use crate::mail::folders::RemoteFolder;
use crate::mail::imap::{self, ConnectionReport};
use crate::mail::settings::ImapSettings;
use crate::model::Account;
use crate::secrets::{Secret, SecretKind, SecretStore};
use crate::{Error, Result};

/// Colours handed out to new accounts in turn.
const ACCOUNT_COLORS: &[&str] = &[
    "oklch(0.72 0.13 250)",
    "oklch(0.74 0.14 60)",
    "oklch(0.72 0.14 150)",
    "oklch(0.7 0.15 330)",
    "oklch(0.75 0.11 200)",
    "oklch(0.72 0.14 20)",
];

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NewAccount {
    pub display_name: String,
    pub email_address: String,
    pub imap: ImapSettings,
}

/// Checks the login, then stores the password and the account with its folders. Nothing is
/// stored if any part fails.
pub async fn add_account(
    db: &Database,
    secrets: &dyn SecretStore,
    new: NewAccount,
    password: Secret,
) -> Result<Account> {
    let new = NewAccount {
        display_name: new.display_name.trim().to_owned(),
        email_address: new.email_address.trim().to_owned(),
        imap: ImapSettings {
            host: new.imap.host.trim().to_owned(),
            username: new.imap.username.trim().to_owned(),
            ..new.imap
        },
    };
    if exists(&db.conn(), &new)? {
        return Err(Error::AccountExists);
    }
    let report = imap::check_login(&new.imap, &password).await?;
    save(db, secrets, &new, &password, &report)
}

pub(crate) fn save(
    db: &Database,
    secrets: &dyn SecretStore,
    new: &NewAccount,
    password: &Secret,
    report: &ConnectionReport,
) -> Result<Account> {
    let id = new_id();
    secrets.set(&id, SecretKind::ImapPassword, password)?;
    let saved = (|| {
        let mut conn = db.conn();
        let tx = conn.transaction()?;
        let account = insert(&tx, &id, new)?;
        store_folders(&tx, &id, &report.folders)?;
        tx.commit()?;
        Ok(account)
    })();
    if saved.is_err() {
        let _ = secrets.delete(&id, SecretKind::ImapPassword);
    }
    saved
}

fn exists(conn: &Connection, new: &NewAccount) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM accounts WHERE lower(imap_host) = lower(?1) AND lower(imap_username) = lower(?2)",
            params![new.imap.host, new.imap.username],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn insert(conn: &Connection, id: &str, new: &NewAccount) -> Result<Account> {
    let count: i64 = conn.query_row("SELECT count(*) FROM accounts", [], |row| row.get(0))?;
    let color = ACCOUNT_COLORS[usize::try_from(count).unwrap_or(0) % ACCOUNT_COLORS.len()];
    let display_name = if new.display_name.is_empty() {
        new.email_address.clone()
    } else {
        new.display_name.clone()
    };
    conn.execute(
        "INSERT INTO accounts (id, display_name, email_address, color, sort_order, auth_method,
           imap_host, imap_port, imap_security, imap_username, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'password', ?6, ?7, ?8, ?9, ?10)",
        params![
            id,
            display_name,
            new.email_address,
            color,
            count,
            new.imap.host,
            new.imap.port,
            new.imap.security.as_db_str(),
            new.imap.username,
            now_millis(),
        ],
    )?;
    Ok(Account {
        id: id.to_owned(),
        display_name,
        email_address: new.email_address.clone(),
        color: color.to_owned(),
    })
}

/// Inserts new folders and updates known ones; folders that hold no mail are skipped.
fn store_folders(conn: &Connection, account_id: &str, folders: &[RemoteFolder]) -> Result<()> {
    let mut statement = conn.prepare(
        "INSERT INTO folders (id, account_id, name, path, delimiter, role)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (account_id, path) DO UPDATE
           SET name = excluded.name, delimiter = excluded.delimiter, role = excluded.role",
    )?;
    for folder in folders.iter().filter(|folder| folder.selectable) {
        statement.execute(params![
            new_id(),
            account_id,
            folder.name,
            folder.path,
            folder.delimiter,
            folder.role.map(|role| role.as_str()),
        ])?;
    }
    Ok(())
}

pub fn list(conn: &Connection) -> Result<Vec<Account>> {
    let mut statement = conn.prepare(
        "SELECT id, display_name, email_address, coalesce(color, ?1) FROM accounts
         ORDER BY sort_order, created_at",
    )?;
    let accounts = statement
        .query_map([ACCOUNT_COLORS[0]], |row| {
            Ok(Account {
                id: row.get(0)?,
                display_name: row.get(1)?,
                email_address: row.get(2)?,
                color: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(accounts)
}

/// Removes the account, its cached mail and all of its credentials. The server keeps everything.
pub fn remove(db: &Database, secrets: &dyn SecretStore, account_id: &str) -> Result<()> {
    let removed = db
        .conn()
        .execute("DELETE FROM accounts WHERE id = ?1", [account_id])?;
    if removed == 0 {
        return Err(Error::AccountNotFound);
    }
    for kind in [
        SecretKind::ImapPassword,
        SecretKind::SmtpPassword,
        SecretKind::OAuthRefreshToken,
    ] {
        if let Err(error) = secrets.delete(account_id, kind) {
            tracing::warn!(%error, ?kind, "could not remove credential of a removed account");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mail::settings::Security;
    use crate::model::FolderRole;
    use crate::secrets::MemoryStore;

    fn new_account() -> NewAccount {
        NewAccount {
            display_name: "Personal".into(),
            email_address: "alex@example.org".into(),
            imap: ImapSettings {
                host: "imap.example.org".into(),
                port: 993,
                security: Security::Tls,
                username: "alex@example.org".into(),
            },
        }
    }

    fn report() -> ConnectionReport {
        ConnectionReport {
            folders: vec![
                RemoteFolder::new("INBOX", Some("/"), None, true),
                RemoteFolder::new("Sent", Some("/"), Some(FolderRole::Sent), true),
                RemoteFolder::new("Shared", Some("/"), None, false),
            ],
        }
    }

    #[test]
    fn saving_stores_account_folders_and_password() {
        let db = Database::open_in_memory().unwrap();
        let secrets = MemoryStore::default();
        let account = save(&db, &secrets, &new_account(), &Secret::new("pw"), &report()).unwrap();

        assert_eq!(list(&db.conn()).unwrap(), vec![account.clone()]);
        assert_eq!(
            secrets
                .get(&account.id, SecretKind::ImapPassword)
                .unwrap()
                .expose(),
            "pw"
        );

        let roles: Vec<Option<String>> = db
            .conn()
            .prepare("SELECT role FROM folders ORDER BY path")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        // The \Noselect folder is not stored.
        assert_eq!(roles, vec![Some("inbox".into()), Some("sent".into())]);

        let password_in_db: i64 = db
            .conn()
            .query_row(
                "SELECT count(*) FROM accounts WHERE imap_username LIKE '%pw%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(password_in_db, 0);
    }

    #[test]
    fn duplicates_are_detected_case_insensitively() {
        let db = Database::open_in_memory().unwrap();
        let secrets = MemoryStore::default();
        save(&db, &secrets, &new_account(), &Secret::new("pw"), &report()).unwrap();
        let mut again = new_account();
        again.imap.host = "IMAP.Example.org".into();
        assert!(exists(&db.conn(), &again).unwrap());
    }

    #[test]
    fn accounts_get_different_colours() {
        let db = Database::open_in_memory().unwrap();
        let secrets = MemoryStore::default();
        let first = save(&db, &secrets, &new_account(), &Secret::new("a"), &report()).unwrap();
        let mut other = new_account();
        other.imap.username = "work@example.org".into();
        let second = save(&db, &secrets, &other, &Secret::new("b"), &report()).unwrap();
        assert_ne!(first.color, second.color);
    }

    #[test]
    fn removing_deletes_data_and_credentials() {
        let db = Database::open_in_memory().unwrap();
        let secrets = MemoryStore::default();
        let account = save(&db, &secrets, &new_account(), &Secret::new("pw"), &report()).unwrap();

        remove(&db, &secrets, &account.id).unwrap();

        assert!(list(&db.conn()).unwrap().is_empty());
        assert!(secrets.get(&account.id, SecretKind::ImapPassword).is_err());
        assert!(matches!(
            remove(&db, &secrets, &account.id),
            Err(Error::AccountNotFound)
        ));
    }
}
