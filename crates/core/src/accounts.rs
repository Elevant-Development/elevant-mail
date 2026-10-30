//! Adding, changing, listing and removing accounts. An account is only saved after a successful
//! login, and a new login only after it worked; passwords go to the keyring, everything else to
//! the database.

use lettre::transport::smtp::client::TlsParameters;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use tokio_rustls::TlsConnector;
use ts_rs::TS;

use crate::db::Database;
use crate::ids::{new_id, now_millis};
use crate::mail::folders::RemoteFolder;
use crate::mail::imap::{self, ConnectionReport};
use crate::mail::presets::smtp_for;
use crate::mail::settings::{ImapSettings, Security, SmtpSettings};
use crate::model::Account;
use crate::oauth::Tokens;
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

/// How an account signs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SignInMethod {
    Password,
    /// Microsoft 365 or Outlook.com, signed in through the browser (OAuth2).
    Microsoft,
}

/// What the account settings show besides name and colour.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AccountSettings {
    pub sign_in: SignInMethod,
    pub imap: ImapSettings,
    /// The outgoing server: as set ([`change_smtp`]), otherwise the one that goes with the
    /// incoming server ([`smtp_for`]).
    pub smtp: SmtpSettings,
    /// The name mail goes out under: as set ([`set_sender_name`]), otherwise the one on the
    /// account's own mail in Sent; empty if there is none.
    pub sender_name: String,
    /// HTML from the editor, put below new mail and above quotes; empty: none.
    pub signature: String,
    /// Pasted as HTML and cleaned ([`set_signature`]): shown as it is, not edited in the editor.
    pub signature_html: bool,
    /// How many days back the cache keeps mail; `None` keeps everything ([`set_keep_days`]).
    pub keep_days: Option<u32>,
}

/// What changes without a login.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AccountChanges {
    pub display_name: String,
    pub color: String,
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
    let password = normalize_password(&new.imap.host, password);
    let credential = imap::Credential::Password(password);
    let report = imap::check_login(&new.imap, &credential).await?;
    let imap::Credential::Password(password) = credential else {
        unreachable!("built above");
    };
    save(db, secrets, &new, Login::Password(&password), &report)
}

/// Adds a Microsoft account after the browser sign-in: logs in once with the access token to
/// check it and list the folders, then keeps the refresh token in the keyring. The address the
/// user signed in with is the IMAP user name.
pub async fn add_oauth_account(
    db: &Database,
    secrets: &dyn SecretStore,
    new: NewAccount,
    tokens: crate::oauth::Tokens,
) -> Result<Account> {
    let refresh_token = tokens
        .refresh_token
        .as_ref()
        .ok_or_else(|| Error::SignInFailed("no refresh token".into()))?;
    let new = NewAccount {
        display_name: new.display_name.trim().to_owned(),
        email_address: new.email_address.trim().to_owned(),
        imap: ImapSettings {
            host: new.imap.host.trim().to_owned(),
            username: tokens
                .user
                .clone()
                .unwrap_or_else(|| new.email_address.trim().to_owned()),
            ..new.imap
        },
    };
    if exists(&db.conn(), &new)? {
        return Err(Error::AccountExists);
    }
    let credential = imap::Credential::AccessToken(Secret::new(tokens.access_token.expose()));
    let report = imap::check_login(&new.imap, &credential).await?;
    let account = save(db, secrets, &new, Login::OAuth(refresh_token), &report)?;
    crate::oauth::remember(&account.id, &tokens);
    Ok(account)
}

/// What an account logs in with, as stored.
pub(crate) enum Login<'a> {
    Password(&'a Secret),
    /// Microsoft: the refresh token; access tokens are fetched with it.
    OAuth(&'a Secret),
}

impl Login<'_> {
    fn secret(&self) -> (SecretKind, &Secret) {
        match self {
            Login::Password(password) => (SecretKind::ImapPassword, password),
            Login::OAuth(token) => (SecretKind::OAuthRefreshToken, token),
        }
    }

    fn method(&self) -> (&'static str, Option<&'static str>) {
        match self {
            Login::Password(_) => ("password", None),
            Login::OAuth(_) => ("oauth2", Some("microsoft")),
        }
    }
}

pub(crate) fn save(
    db: &Database,
    secrets: &dyn SecretStore,
    new: &NewAccount,
    login: Login<'_>,
    report: &ConnectionReport,
) -> Result<Account> {
    let id = new_id();
    let (kind, secret) = login.secret();
    secrets.set(&id, kind, secret)?;
    let saved = (|| {
        let mut conn = db.conn();
        let tx = conn.transaction()?;
        let account = insert(&tx, &id, new, login.method())?;
        store_folders(&tx, &id, &report.folders)?;
        tx.commit()?;
        Ok(account)
    })();
    if saved.is_err() {
        let _ = secrets.delete(&id, kind);
    }
    saved
}

/// Google shows app passwords in groups of four ("abcd efgh ijkl mnop") and expects them without
/// the spaces; pasted as shown, the login would fail. Other passwords may contain spaces.
fn normalize_password(host: &str, password: Secret) -> Secret {
    let gmail = ["imap.gmail.com", "smtp.gmail.com"]
        .iter()
        .any(|gmail| host.eq_ignore_ascii_case(gmail));
    if gmail && password.expose().contains(' ') {
        Secret::new(password.expose().split_whitespace().collect::<String>())
    } else {
        password
    }
}

fn exists(conn: &Connection, new: &NewAccount) -> Result<bool> {
    login_taken(conn, &new.imap, None)
}

/// Another account (other than `except`) already logs in as this user on this server.
fn login_taken(conn: &Connection, server: &ImapSettings, except: Option<&str>) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM accounts
             WHERE lower(imap_host) = lower(?1) AND lower(imap_username) = lower(?2)
               AND (?3 IS NULL OR id <> ?3)",
            params![server.host, server.username, except],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn insert(
    conn: &Connection,
    id: &str,
    new: &NewAccount,
    (auth_method, oauth_provider): (&str, Option<&str>),
) -> Result<Account> {
    let count: i64 = conn.query_row("SELECT count(*) FROM accounts", [], |row| row.get(0))?;
    let color = ACCOUNT_COLORS[usize::try_from(count).unwrap_or(0) % ACCOUNT_COLORS.len()];
    let display_name = if new.display_name.is_empty() {
        new.email_address.clone()
    } else {
        new.display_name.clone()
    };
    conn.execute(
        "INSERT INTO accounts (id, display_name, email_address, color, sort_order, auth_method,
           oauth_provider, imap_host, imap_port, imap_security, imap_username, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?11, ?12, ?6, ?7, ?8, ?9, ?10)",
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
            auth_method,
            oauth_provider,
        ],
    )?;
    Ok(Account {
        id: id.to_owned(),
        display_name,
        email_address: new.email_address.clone(),
        color: color.to_owned(),
        in_unified_inbox: true,
    })
}

/// Inserts new folders and updates known ones; folders that hold no mail are skipped.
/// The account's folders as the server lists them now: new ones added, roles and names updated,
/// vanished ones removed together with their cached mail.
pub(crate) fn replace_folders(
    conn: &Connection,
    account_id: &str,
    folders: &[RemoteFolder],
) -> Result<()> {
    store_folders(conn, account_id, folders)?;
    let listed: Vec<&str> = folders
        .iter()
        .filter(|folder| folder.selectable)
        .map(|folder| folder.path.as_str())
        .collect();
    let mut statement = conn.prepare("SELECT id, path FROM folders WHERE account_id = ?1")?;
    let stored: Vec<(String, String)> = statement
        .query_map([account_id], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (id, path) in stored {
        if !listed.contains(&path.as_str()) {
            conn.execute("DELETE FROM folders WHERE id = ?1", [id])?;
        }
    }
    Ok(())
}

pub(crate) fn store_folders(
    conn: &Connection,
    account_id: &str,
    folders: &[RemoteFolder],
) -> Result<()> {
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
        "SELECT id, display_name, email_address, coalesce(color, ?1), in_unified_inbox
         FROM accounts ORDER BY sort_order, created_at",
    )?;
    let accounts = statement
        .query_map([ACCOUNT_COLORS[0]], |row| {
            Ok(Account {
                id: row.get(0)?,
                display_name: row.get(1)?,
                email_address: row.get(2)?,
                color: row.get(3)?,
                in_unified_inbox: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(accounts)
}

pub fn settings(conn: &Connection, account_id: &str) -> Result<AccountSettings> {
    let row = conn
        .query_row(
            "SELECT imap_host, imap_port, imap_security, imap_username, auth_method, keep_days,
               email_address, smtp_host, smtp_port, smtp_security, smtp_username, sender_name,
               signature, signature_html
             FROM accounts WHERE id = ?1",
            [account_id],
            |row| {
                let security: String = row.get(2)?;
                let method: String = row.get(4)?;
                let imap = ImapSettings {
                    host: row.get(0)?,
                    port: row.get(1)?,
                    security: Security::from_db_str(&security),
                    username: row.get(3)?,
                };
                let email: String = row.get(6)?;
                let smtp_host: Option<String> = row.get(7)?;
                let smtp = match smtp_host {
                    Some(host) => SmtpSettings {
                        host,
                        port: row.get(8)?,
                        security: Security::from_db_str(
                            &row.get::<_, Option<String>>(9)?.unwrap_or_default(),
                        ),
                        username: row
                            .get::<_, Option<String>>(10)?
                            .unwrap_or_else(|| imap.username.clone()),
                    },
                    None => smtp_for(&imap, &email),
                };
                let settings = AccountSettings {
                    sign_in: if method == "oauth2" {
                        SignInMethod::Microsoft
                    } else {
                        SignInMethod::Password
                    },
                    imap,
                    smtp,
                    sender_name: row.get::<_, Option<String>>(11)?.unwrap_or_default(),
                    signature: row.get::<_, Option<String>>(12)?.unwrap_or_default(),
                    signature_html: row.get(13)?,
                    keep_days: row.get(5)?,
                };
                Ok((settings, email))
            },
        )
        .optional()?;
    let (mut settings, email) = row.ok_or(Error::AccountNotFound)?;
    if settings.sender_name.is_empty() {
        settings.sender_name = name_in_sent(conn, account_id, &email)?.unwrap_or_default();
    }
    Ok(settings)
}

/// The name the account's own mail in Sent goes out under, the latest first.
fn name_in_sent(conn: &Connection, account_id: &str, email: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT m.from_name FROM messages m JOIN folders f ON f.id = m.folder_id
             WHERE m.account_id = ?1 AND f.role = 'sent' AND lower(m.from_address) = lower(?2)
               AND trim(coalesce(m.from_name, '')) <> ''
             ORDER BY m.date_sent DESC LIMIT 1",
            [account_id, email],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .map(|name| name.trim().to_owned()))
}

/// New outgoing server settings, and a password for it where it differs from the IMAP one:
/// signs in with them first and keeps the old ones if that fails.
pub async fn change_smtp(
    db: &Database,
    secrets: &dyn SecretStore,
    account_id: &str,
    smtp: SmtpSettings,
    password: Option<Secret>,
) -> Result<()> {
    let tls = crate::mail::smtp::platform_tls(&smtp.host)?;
    change_smtp_with(db, secrets, account_id, smtp, password, tls).await
}

pub(crate) async fn change_smtp_with(
    db: &Database,
    secrets: &dyn SecretStore,
    account_id: &str,
    smtp: SmtpSettings,
    password: Option<Secret>,
    tls: TlsParameters,
) -> Result<()> {
    let smtp = SmtpSettings {
        host: smtp.host.trim().to_owned(),
        username: smtp.username.trim().to_owned(),
        ..smtp
    };
    if smtp.host.is_empty() {
        return Err(Error::InvalidHost);
    }
    let sign_in = settings(&db.conn(), account_id)?.sign_in;
    let password = password.map(|password| normalize_password(&smtp.host, password));
    let credential = match (&password, sign_in) {
        (Some(password), SignInMethod::Password) => {
            imap::Credential::Password(Secret::new(password.expose()))
        }
        (Some(_), SignInMethod::Microsoft) => return Err(Error::WrongSignInMethod),
        (None, sign_in) => crate::mail::send::credential(secrets, account_id, sign_in).await?,
    };
    crate::mail::smtp::check_login_with(&smtp, &credential, tls).await?;
    db.conn().execute(
        "UPDATE accounts SET smtp_host = ?2, smtp_port = ?3, smtp_security = ?4, smtp_username = ?5
         WHERE id = ?1",
        params![
            account_id,
            smtp.host,
            smtp.port,
            smtp.security.as_db_str(),
            smtp.username
        ],
    )?;
    if let Some(password) = &password {
        secrets.set(account_id, SecretKind::SmtpPassword, password)?;
    }
    Ok(())
}

/// The signature: HTML from the editor, or `pasted` HTML, which is cleaned of scripts and the like
/// first. Empty removes it.
pub fn set_signature(conn: &Connection, account_id: &str, html: &str, pasted: bool) -> Result<()> {
    let html = if pasted {
        crate::mail::sanitize::clean_signature(html)
    } else {
        html.trim().to_owned()
    };
    // The editor's empty document is no signature.
    let empty = html.is_empty() || html == "<p></p>";
    let updated = conn.execute(
        "UPDATE accounts SET signature = ?2, signature_html = ?3 WHERE id = ?1",
        params![account_id, (!empty).then_some(html), pasted && !empty],
    )?;
    if updated == 0 {
        return Err(Error::AccountNotFound);
    }
    Ok(())
}

/// The name mail goes out under; empty takes the one from Sent again.
pub fn set_sender_name(conn: &Connection, account_id: &str, name: &str) -> Result<()> {
    let name = name.trim();
    let updated = conn.execute(
        "UPDATE accounts SET sender_name = ?2 WHERE id = ?1",
        params![account_id, (!name.is_empty()).then_some(name)],
    )?;
    if updated == 0 {
        return Err(Error::AccountNotFound);
    }
    Ok(())
}

/// How far back the account keeps mail in the cache, in days; `None` keeps everything. Every
/// folder keeps at least its newest 100 anyway. A longer period is fetched at the next sync, a
/// shorter one keeps what is cached.
pub fn set_keep_days(conn: &Connection, account_id: &str, days: Option<u32>) -> Result<()> {
    let updated = conn.execute(
        "UPDATE accounts SET keep_days = ?2 WHERE id = ?1",
        params![account_id, days.map(|days| days.clamp(1, 36_500))],
    )?;
    if updated == 0 {
        return Err(Error::AccountNotFound);
    }
    Ok(())
}

/// Takes the account's inbox into the unified inbox or out of it.
pub fn set_in_unified_inbox(conn: &Connection, account_id: &str, included: bool) -> Result<()> {
    let updated = conn.execute(
        "UPDATE accounts SET in_unified_inbox = ?2 WHERE id = ?1",
        params![account_id, included],
    )?;
    if updated == 0 {
        return Err(Error::AccountNotFound);
    }
    Ok(())
}

/// Renames the account and sets its colour. An empty name falls back to the address, as when
/// the account was added.
pub fn update(conn: &Connection, account_id: &str, changes: &AccountChanges) -> Result<Account> {
    let color = changes.color.trim();
    if !is_color(color) {
        return Err(Error::InvalidColor);
    }
    let updated = conn.execute(
        "UPDATE accounts
         SET display_name = CASE WHEN ?2 = '' THEN email_address ELSE ?2 END, color = ?3
         WHERE id = ?1",
        params![account_id, changes.display_name.trim(), color],
    )?;
    if updated == 0 {
        return Err(Error::AccountNotFound);
    }
    list(conn)?
        .into_iter()
        .find(|account| account.id == account_id)
        .ok_or(Error::AccountNotFound)
}

/// The interface offers a palette; this keeps anything but a plain colour value out of the
/// style attribute it ends up in.
fn is_color(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 48
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " .,%()#-".contains(c))
}

/// New server settings or a new password (or both) for an account that signs in with a
/// password. Logs in with them first and changes nothing if that fails. `password: None` keeps
/// the stored one.
pub async fn change_login(
    db: &Database,
    secrets: &dyn SecretStore,
    account_id: &str,
    server: ImapSettings,
    password: Option<Secret>,
) -> Result<()> {
    let connector = imap::platform_connector()?;
    change_login_with(db, secrets, account_id, server, password, &connector).await
}

async fn change_login_with(
    db: &Database,
    secrets: &dyn SecretStore,
    account_id: &str,
    server: ImapSettings,
    password: Option<Secret>,
    connector: &TlsConnector,
) -> Result<()> {
    if settings(&db.conn(), account_id)?.sign_in != SignInMethod::Password {
        return Err(Error::WrongSignInMethod);
    }
    let server = ImapSettings {
        host: server.host.trim().to_owned(),
        username: server.username.trim().to_owned(),
        ..server
    };
    if login_taken(&db.conn(), &server, Some(account_id))? {
        return Err(Error::AccountExists);
    }
    let password = password.map(|password| normalize_password(&server.host, password));
    let credential = imap::Credential::Password(match &password {
        Some(password) => Secret::new(password.expose()),
        None => secrets.get(account_id, SecretKind::ImapPassword)?,
    });
    let report = imap::check_login_with(&server, &credential, connector).await?;
    apply_login(db, secrets, account_id, &server, password.as_ref(), &report)
}

/// Stores a login that worked, together with the folders the server listed for it.
fn apply_login(
    db: &Database,
    secrets: &dyn SecretStore,
    account_id: &str,
    server: &ImapSettings,
    password: Option<&Secret>,
    report: &ConnectionReport,
) -> Result<()> {
    let previous = match password {
        Some(password) => {
            let previous = secrets.get(account_id, SecretKind::ImapPassword).ok();
            secrets.set(account_id, SecretKind::ImapPassword, password)?;
            previous
        }
        None => None,
    };
    let saved = (|| {
        let mut conn = db.conn();
        let tx = conn.transaction()?;
        let updated = tx.execute(
            "UPDATE accounts SET imap_host = ?2, imap_port = ?3, imap_security = ?4,
               imap_username = ?5
             WHERE id = ?1",
            params![
                account_id,
                server.host,
                server.port,
                server.security.as_db_str(),
                server.username,
            ],
        )?;
        if updated == 0 {
            return Err(Error::AccountNotFound);
        }
        replace_folders(&tx, account_id, &report.folders)?;
        tx.commit()?;
        Ok(())
    })();
    if saved.is_err()
        && let Some(previous) = previous
    {
        let _ = secrets.set(account_id, SecretKind::ImapPassword, &previous);
    }
    saved
}

/// The address to sign in with again: the one Microsoft confirmed the first time.
pub fn microsoft_login(conn: &Connection, account_id: &str) -> Result<String> {
    let current = settings(conn, account_id)?;
    if current.sign_in != SignInMethod::Microsoft {
        return Err(Error::WrongSignInMethod);
    }
    Ok(current.imap.username)
}

/// After signing in with Microsoft again: checks that it was the same account, logs in once with
/// the new access token and keeps the new refresh token.
pub async fn replace_oauth_tokens(
    db: &Database,
    secrets: &dyn SecretStore,
    account_id: &str,
    tokens: Tokens,
) -> Result<()> {
    let connector = imap::platform_connector()?;
    replace_oauth_tokens_with(db, secrets, account_id, tokens, &connector).await
}

async fn replace_oauth_tokens_with(
    db: &Database,
    secrets: &dyn SecretStore,
    account_id: &str,
    tokens: Tokens,
    connector: &TlsConnector,
) -> Result<()> {
    let current = settings(&db.conn(), account_id)?;
    check_same_account(&current, &tokens)?;
    let refresh_token = tokens
        .refresh_token
        .as_ref()
        .ok_or_else(|| Error::SignInFailed("no refresh token".into()))?;
    let credential = imap::Credential::AccessToken(Secret::new(tokens.access_token.expose()));
    imap::check_login_with(&current.imap, &credential, connector).await?;
    secrets.set(account_id, SecretKind::OAuthRefreshToken, refresh_token)?;
    crate::oauth::remember(account_id, &tokens);
    Ok(())
}

fn check_same_account(current: &AccountSettings, tokens: &Tokens) -> Result<()> {
    if current.sign_in != SignInMethod::Microsoft {
        return Err(Error::WrongSignInMethod);
    }
    match &tokens.user {
        Some(user) if !user.eq_ignore_ascii_case(&current.imap.username) => {
            Err(Error::DifferentAccount)
        }
        _ => Ok(()),
    }
}

/// Removes the account, its cached mail and all of its credentials. The server keeps everything.
pub fn remove(db: &Database, secrets: &dyn SecretStore, account_id: &str) -> Result<()> {
    let removed = db
        .conn()
        .execute("DELETE FROM accounts WHERE id = ?1", [account_id])?;
    if removed == 0 {
        return Err(Error::AccountNotFound);
    }
    crate::oauth::forget(account_id);
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
    use crate::mail::test_server::{self, ACCESS_TOKEN, Mode, PASSWORD};
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
        let account = save(
            &db,
            &secrets,
            &new_account(),
            Login::Password(&Secret::new("pw")),
            &report(),
        )
        .unwrap();

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
        save(
            &db,
            &secrets,
            &new_account(),
            Login::Password(&Secret::new("pw")),
            &report(),
        )
        .unwrap();
        let mut again = new_account();
        again.imap.host = "IMAP.Example.org".into();
        assert!(exists(&db.conn(), &again).unwrap());
    }

    #[test]
    fn accounts_get_different_colours() {
        let db = Database::open_in_memory().unwrap();
        let secrets = MemoryStore::default();
        let first = save(
            &db,
            &secrets,
            &new_account(),
            Login::Password(&Secret::new("a")),
            &report(),
        )
        .unwrap();
        let mut other = new_account();
        other.imap.username = "work@example.org".into();
        let second = save(
            &db,
            &secrets,
            &other,
            Login::Password(&Secret::new("b")),
            &report(),
        )
        .unwrap();
        assert_ne!(first.color, second.color);
    }

    #[test]
    fn removing_deletes_data_and_credentials() {
        let db = Database::open_in_memory().unwrap();
        let secrets = MemoryStore::default();
        let account = save(
            &db,
            &secrets,
            &new_account(),
            Login::Password(&Secret::new("pw")),
            &report(),
        )
        .unwrap();

        remove(&db, &secrets, &account.id).unwrap();

        assert!(list(&db.conn()).unwrap().is_empty());
        assert!(secrets.get(&account.id, SecretKind::ImapPassword).is_err());
        assert!(matches!(
            remove(&db, &secrets, &account.id),
            Err(Error::AccountNotFound)
        ));
    }

    /// `new_account()` on the local test server.
    fn on_test_server(port: u16) -> NewAccount {
        let mut new = new_account();
        new.imap.host = "localhost".into();
        new.imap.port = port;
        new
    }

    fn folder_paths(db: &Database, account_id: &str) -> Vec<String> {
        db.conn()
            .prepare("SELECT path FROM folders WHERE account_id = ?1 ORDER BY path")
            .unwrap()
            .query_map([account_id], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    fn tokens(user: Option<&str>) -> Tokens {
        Tokens {
            access_token: Secret::new(ACCESS_TOKEN),
            refresh_token: Some(Secret::new("new-refresh-token")),
            expires_at: now_millis() + 3_600_000,
            user: user.map(Into::into),
        }
    }

    #[test]
    fn settings_tell_how_an_account_signs_in() {
        let db = Database::open_in_memory().unwrap();
        let secrets = MemoryStore::default();
        let password = save(
            &db,
            &secrets,
            &new_account(),
            Login::Password(&Secret::new("pw")),
            &report(),
        )
        .unwrap();
        let mut work = new_account();
        work.imap.username = "alex@contoso.example".into();
        let microsoft = save(
            &db,
            &secrets,
            &work,
            Login::OAuth(&Secret::new("refresh")),
            &report(),
        )
        .unwrap();

        assert_eq!(
            settings(&db.conn(), &password.id).unwrap(),
            AccountSettings {
                sign_in: SignInMethod::Password,
                imap: new_account().imap,
                smtp: SmtpSettings {
                    host: "smtp.example.org".into(),
                    port: 465,
                    security: Security::Tls,
                    username: "alex@example.org".into(),
                },
                sender_name: String::new(),
                signature: String::new(),
                signature_html: false,
                keep_days: Some(90),
            }
        );

        // The sender name: as set, otherwise the one on the account's mail in Sent.
        db.conn()
            .execute_batch(&format!(
                "INSERT INTO folders (id, account_id, name, path, role)
                 VALUES ('sent', '{id}', 'Sent Items', 'Sent Items', 'sent');
                 INSERT INTO messages (id, account_id, folder_id, uid, from_name, from_address,
                   date_sent, fetched_at)
                 VALUES ('old', '{id}', 'sent', 1, 'Alex', 'alex@example.org', 1, 0),
                        ('new', '{id}', 'sent', 2, 'Alex Berger', 'Alex@Example.org', 2, 0);",
                id = password.id
            ))
            .unwrap();
        let sender = || settings(&db.conn(), &password.id).unwrap().sender_name;
        assert_eq!(sender(), "Alex Berger");
        set_sender_name(&db.conn(), &password.id, "  A. Berger ").unwrap();
        assert_eq!(sender(), "A. Berger");
        set_sender_name(&db.conn(), &password.id, "").unwrap();
        assert_eq!(sender(), "Alex Berger");

        let signature = || settings(&db.conn(), &password.id).unwrap().signature;
        set_signature(
            &db.conn(),
            &password.id,
            "<p>Viele Grüße<br>Alex</p>",
            false,
        )
        .unwrap();
        assert_eq!(signature(), "<p>Viele Grüße<br>Alex</p>");
        set_signature(&db.conn(), &password.id, "<p></p>", false).unwrap();
        assert_eq!(signature(), "");
        // Pasted HTML is cleaned and marked as such.
        set_signature(
            &db.conn(),
            &password.id,
            "<table><tr><td onclick=\"x()\">Alex</td></tr></table>",
            true,
        )
        .unwrap();
        let saved = settings(&db.conn(), &password.id).unwrap();
        assert!(saved.signature_html);
        assert!(saved.signature.contains("<td>Alex</td>"));
        set_keep_days(&db.conn(), &password.id, None).unwrap();
        assert_eq!(settings(&db.conn(), &password.id).unwrap().keep_days, None);
        set_keep_days(&db.conn(), &password.id, Some(0)).unwrap();
        assert_eq!(
            settings(&db.conn(), &password.id).unwrap().keep_days,
            Some(1)
        );
        assert!(matches!(
            set_keep_days(&db.conn(), "missing", Some(30)),
            Err(Error::AccountNotFound)
        ));

        // In the unified inbox until taken out.
        assert!(password.in_unified_inbox);
        set_in_unified_inbox(&db.conn(), &password.id, false).unwrap();
        let listed = list(&db.conn()).unwrap();
        let included = |id: &str| listed.iter().find(|a| a.id == id).unwrap().in_unified_inbox;
        assert!(!included(&password.id));
        assert!(included(&microsoft.id));
        assert!(matches!(
            set_in_unified_inbox(&db.conn(), "missing", true),
            Err(Error::AccountNotFound)
        ));
        assert_eq!(
            settings(&db.conn(), &microsoft.id).unwrap().sign_in,
            SignInMethod::Microsoft
        );
        assert_eq!(
            microsoft_login(&db.conn(), &microsoft.id).unwrap(),
            "alex@contoso.example"
        );
        assert!(matches!(
            microsoft_login(&db.conn(), &password.id),
            Err(Error::WrongSignInMethod)
        ));
        assert!(matches!(
            settings(&db.conn(), "missing"),
            Err(Error::AccountNotFound)
        ));
    }

    #[test]
    fn renaming_and_recolouring() {
        let db = Database::open_in_memory().unwrap();
        let secrets = MemoryStore::default();
        let account = save(
            &db,
            &secrets,
            &new_account(),
            Login::Password(&Secret::new("pw")),
            &report(),
        )
        .unwrap();
        let changes = |name: &str, color: &str| AccountChanges {
            display_name: name.into(),
            color: color.into(),
        };

        let renamed = update(
            &db.conn(),
            &account.id,
            &changes("  Mail  ", "oklch(0.7 0.15 330)"),
        )
        .unwrap();
        assert_eq!(renamed.display_name, "Mail");
        assert_eq!(renamed.color, "oklch(0.7 0.15 330)");
        assert_eq!(list(&db.conn()).unwrap(), vec![renamed]);

        let unnamed = update(&db.conn(), &account.id, &changes(" ", "#4a90d9")).unwrap();
        assert_eq!(unnamed.display_name, "alex@example.org");

        assert!(matches!(
            update(
                &db.conn(),
                &account.id,
                &changes("Mail", "red; background: url(x)")
            ),
            Err(Error::InvalidColor)
        ));
        assert!(matches!(
            update(&db.conn(), "missing", &changes("Mail", "#4a90d9")),
            Err(Error::AccountNotFound)
        ));
    }

    #[tokio::test]
    async fn a_new_password_is_kept_once_it_worked() {
        let server = test_server::start(Mode::Tls).await;
        let db = Database::open_in_memory().unwrap();
        let secrets = MemoryStore::default();
        let account = save(
            &db,
            &secrets,
            &on_test_server(server.port),
            Login::Password(&Secret::new("old-password")),
            &report(),
        )
        .unwrap();

        change_login_with(
            &db,
            &secrets,
            &account.id,
            on_test_server(server.port).imap,
            Some(Secret::new(PASSWORD)),
            &server.connector,
        )
        .await
        .unwrap();

        assert_eq!(
            secrets
                .get(&account.id, SecretKind::ImapPassword)
                .unwrap()
                .expose(),
            PASSWORD
        );
        // The folders come from the server's listing now.
        assert_eq!(
            folder_paths(&db, &account.id),
            vec!["Entw&APw-rfe", "INBOX", "Sent"]
        );
    }

    #[tokio::test]
    async fn new_server_settings_use_the_stored_password() {
        let server = test_server::start(Mode::StartTls).await;
        let db = Database::open_in_memory().unwrap();
        let secrets = MemoryStore::default();
        let mut before = on_test_server(server.port);
        before.imap.port = 1;
        let account = save(
            &db,
            &secrets,
            &before,
            Login::Password(&Secret::new(PASSWORD)),
            &report(),
        )
        .unwrap();

        let mut fixed = on_test_server(server.port).imap;
        fixed.security = Security::StartTls;
        fixed.host = " localhost ".into();
        change_login_with(&db, &secrets, &account.id, fixed, None, &server.connector)
            .await
            .unwrap();

        let stored = settings(&db.conn(), &account.id).unwrap().imap;
        assert_eq!(stored.host, "localhost");
        assert_eq!(stored.port, server.port);
        assert_eq!(stored.security, Security::StartTls);
    }

    #[tokio::test]
    async fn a_failed_login_changes_nothing() {
        let server = test_server::start(Mode::Tls).await;
        let db = Database::open_in_memory().unwrap();
        let secrets = MemoryStore::default();
        let account = save(
            &db,
            &secrets,
            &on_test_server(server.port),
            Login::Password(&Secret::new(PASSWORD)),
            &report(),
        )
        .unwrap();

        let mut changed = on_test_server(server.port).imap;
        changed.username = "alex".into();
        let error = change_login_with(
            &db,
            &secrets,
            &account.id,
            changed,
            Some(Secret::new("wrong")),
            &server.connector,
        )
        .await
        .unwrap_err();

        assert!(matches!(error, Error::AuthenticationFailed { .. }));
        assert_eq!(
            secrets
                .get(&account.id, SecretKind::ImapPassword)
                .unwrap()
                .expose(),
            PASSWORD
        );
        assert_eq!(
            settings(&db.conn(), &account.id).unwrap().imap,
            on_test_server(server.port).imap
        );
    }

    #[tokio::test]
    async fn a_login_of_another_account_is_refused() {
        let db = Database::open_in_memory().unwrap();
        let secrets = MemoryStore::default();
        let first = save(
            &db,
            &secrets,
            &new_account(),
            Login::Password(&Secret::new("a")),
            &report(),
        )
        .unwrap();
        let mut other = new_account();
        other.imap.username = "work@example.org".into();
        let second = save(
            &db,
            &secrets,
            &other,
            Login::Password(&Secret::new("b")),
            &report(),
        )
        .unwrap();
        let connector = imap::platform_connector().unwrap();

        let mut taken = new_account().imap;
        taken.username = "ALEX@example.org".into();
        let error = change_login_with(&db, &secrets, &second.id, taken, None, &connector)
            .await
            .unwrap_err();
        assert!(matches!(error, Error::AccountExists));

        // The account's own login is not "taken".
        assert!(!login_taken(&db.conn(), &new_account().imap, Some(&first.id)).unwrap());
    }

    #[tokio::test]
    async fn signing_in_again_needs_the_same_microsoft_account() {
        let server = test_server::start(Mode::Tls).await;
        let db = Database::open_in_memory().unwrap();
        let secrets = MemoryStore::default();
        let account = save(
            &db,
            &secrets,
            &on_test_server(server.port),
            Login::OAuth(&Secret::new("old-refresh-token")),
            &report(),
        )
        .unwrap();
        let refresh_token = || {
            secrets
                .get(&account.id, SecretKind::OAuthRefreshToken)
                .unwrap()
                .expose()
                .to_owned()
        };

        let error = replace_oauth_tokens_with(
            &db,
            &secrets,
            &account.id,
            tokens(Some("someone@example.org")),
            &server.connector,
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::DifferentAccount));
        assert_eq!(refresh_token(), "old-refresh-token");

        replace_oauth_tokens_with(
            &db,
            &secrets,
            &account.id,
            tokens(Some("Alex@Example.org")),
            &server.connector,
        )
        .await
        .unwrap();
        assert_eq!(refresh_token(), "new-refresh-token");
        crate::oauth::forget(&account.id);

        // A password account neither signs in with Microsoft nor the other way round.
        let error = change_login_with(
            &db,
            &secrets,
            &account.id,
            on_test_server(server.port).imap,
            Some(Secret::new(PASSWORD)),
            &server.connector,
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::WrongSignInMethod));
    }

    #[test]
    fn gmail_app_passwords_lose_their_spaces() {
        let pasted = Secret::new("abcd efgh ijkl mnop");
        assert_eq!(
            normalize_password("imap.gmail.com", pasted).expose(),
            "abcdefghijklmnop"
        );
        let own = Secret::new("mein geheimes Passwort");
        assert_eq!(
            normalize_password("imap.example.org", own).expose(),
            "mein geheimes Passwort"
        );
    }
}
