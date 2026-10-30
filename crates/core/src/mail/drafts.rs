//! Mail being written, kept across restarts and in the server's drafts folder, so it can be
//! finished later or on another device. The app saves a draft as it changes; leaving the
//! composer uploads it, replacing the copy before (found by its Message-ID). Sending or
//! discarding removes both.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use tokio_rustls::TlsConnector;
use ts_rs::TS;

use crate::db::Database;
use crate::ids::now_millis;
use crate::mail::imap::{self, COMMAND_TIMEOUT, protocol_error, with_timeout};
use crate::mail::sanitize::HtmlView;
use crate::mail::send::{self, OutgoingMail};
use crate::model::{Address, FolderRole};
use crate::secrets::SecretStore;
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ComposeMode {
    New,
    Reply,
    ReplyAll,
    Forward,
}

impl ComposeMode {
    fn as_str(self) -> &'static str {
        match self {
            ComposeMode::New => "new",
            ComposeMode::Reply => "reply",
            ComposeMode::ReplyAll => "replyAll",
            ComposeMode::Forward => "forward",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "reply" => ComposeMode::Reply,
            "replyAll" => ComposeMode::ReplyAll,
            "forward" => ComposeMode::Forward,
            _ => ComposeMode::New,
        }
    }
}

/// A mail being written, as the composer holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Draft {
    /// "new:<id>", "<mode>:<message>" for a reply or forward, "draft:<Message-ID>" for one
    /// opened from the drafts folder.
    pub key: String,
    pub mode: ComposeMode,
    /// The cached message answered or forwarded.
    pub source_id: Option<String>,
    pub account_id: String,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub bcc: Vec<Address>,
    pub subject: String,
    /// The editor's content, and its plain-text version.
    pub html: String,
    pub text: String,
}

impl Draft {
    /// Nothing written yet: not worth a copy on the server.
    fn is_empty(&self) -> bool {
        self.to.is_empty()
            && self.cc.is_empty()
            && self.bcc.is_empty()
            && self.subject.trim().is_empty()
            && self.text.trim().is_empty()
    }
}

/// Where a draft's copy on the server is: the account and the Message-ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerCopy {
    pub account_id: String,
    pub message_id: String,
}

const COLUMNS: &str = "key, mode, source_id, account_id, to_addresses, cc_addresses,
    bcc_addresses, subject, html, text";

fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Draft> {
    let addresses = |index: usize| -> rusqlite::Result<Vec<Address>> {
        let json: String = row.get(index)?;
        Ok(serde_json::from_str(&json).unwrap_or_default())
    };
    Ok(Draft {
        key: row.get(0)?,
        mode: ComposeMode::parse(&row.get::<_, String>(1)?),
        source_id: row.get(2)?,
        account_id: row.get(3)?,
        to: addresses(4)?,
        cc: addresses(5)?,
        bcc: addresses(6)?,
        subject: row.get(7)?,
        html: row.get(8)?,
        text: row.get(9)?,
    })
}

fn json(addresses: &[Address]) -> String {
    serde_json::to_string(addresses).expect("addresses are valid JSON")
}

/// Keeps the draft as it is now; a new one gets the Message-ID its server copy goes under.
pub fn save(conn: &Connection, draft: &Draft) -> Result<()> {
    let message_id = send::new_message_id(&send::account_email(conn, &draft.account_id)?);
    conn.execute(
        "INSERT INTO drafts (key, mode, source_id, account_id, to_addresses, cc_addresses,
           bcc_addresses, subject, html, text, message_id, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
         ON CONFLICT (key) DO UPDATE SET account_id = excluded.account_id,
           to_addresses = excluded.to_addresses, cc_addresses = excluded.cc_addresses,
           bcc_addresses = excluded.bcc_addresses, subject = excluded.subject,
           html = excluded.html, text = excluded.text,
           updated_at = max(excluded.updated_at, updated_at + 1)",
        params![
            draft.key,
            draft.mode.as_str(),
            draft.source_id,
            draft.account_id,
            json(&draft.to),
            json(&draft.cc),
            json(&draft.bcc),
            draft.subject,
            draft.html,
            draft.text,
            message_id,
            now_millis(),
        ],
    )?;
    Ok(())
}

/// All drafts, the latest change first.
pub fn list(conn: &Connection) -> Result<Vec<Draft>> {
    Ok(conn
        .prepare_cached(&format!(
            "SELECT {COLUMNS} FROM drafts ORDER BY updated_at DESC"
        ))?
        .query_map([], from_row)?
        .collect::<rusqlite::Result<_>>()?)
}

/// Removes the draft here; returns where its server copy is, if it has one.
pub fn take(conn: &Connection, key: &str) -> Result<Option<ServerCopy>> {
    let copy = conn
        .query_row(
            "DELETE FROM drafts WHERE key = ?1 RETURNING uploaded_to, message_id",
            [key],
            |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    Ok(copy.and_then(|(account, message_id)| {
        account.map(|account_id| ServerCopy {
            account_id,
            message_id,
        })
    }))
}

/// A draft from the drafts folder (written here earlier or on another device), to write on. The
/// one kept here wins; otherwise it is taken from the cached copy, which stays its server copy.
pub fn open_cached(conn: &Connection, message_id: &str) -> Result<Draft> {
    let (account_id, header): (String, Option<String>) = conn
        .query_row(
            "SELECT account_id, message_id FROM messages WHERE id = ?1",
            [message_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or(Error::MessageNotFound)?;
    let header = format!(
        "<{}>",
        header
            .unwrap_or_else(|| message_id.to_owned())
            .trim_matches(['<', '>'])
    );
    let kept = conn
        .query_row(
            &format!("SELECT {COLUMNS} FROM drafts WHERE message_id = ?1"),
            [&header],
            from_row,
        )
        .optional()?;
    if let Some(draft) = kept {
        return Ok(draft);
    }

    let view = HtmlView {
        asset_base: "",
        remote_images: false,
    };
    let opened = crate::mailbox::get_message(conn, message_id, view)?
        .ok_or(Error::MessageNotFound)?
        .message;
    let in_reply_to: Option<String> = conn.query_row(
        "SELECT in_reply_to FROM messages WHERE id = ?1",
        [message_id],
        |row| row.get(0),
    )?;
    // The answered mail, if it is cached: the reply keeps its place in the conversation.
    let source_id = match in_reply_to {
        Some(parent) => conn
            .query_row(
                "SELECT id FROM messages WHERE account_id = ?1 AND message_id = ?2 LIMIT 1",
                params![account_id, parent.trim_matches(['<', '>'])],
                |row| row.get(0),
            )
            .optional()?,
        None => None,
    };
    let text = opened.body_text.clone().unwrap_or_default();
    let draft = Draft {
        key: format!("draft:{header}"),
        mode: if source_id.is_some() {
            ComposeMode::Reply
        } else {
            ComposeMode::New
        },
        source_id,
        account_id: account_id.clone(),
        to: opened.to,
        cc: opened.cc,
        bcc: Vec::new(),
        subject: opened.summary.subject.unwrap_or_default(),
        html: opened.body_html.unwrap_or_else(|| paragraphs(&text)),
        text,
    };
    save(conn, &draft)?;
    // Its server copy is the cached one, as it is.
    conn.execute(
        "UPDATE drafts SET message_id = ?2, uploaded_to = ?3, uploaded_at = updated_at
         WHERE key = ?1",
        params![draft.key, header, account_id],
    )?;
    Ok(draft)
}

/// Plain text as HTML paragraphs, for a draft that has no HTML.
fn paragraphs(text: &str) -> String {
    let escape = |line: &str| {
        line.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    text.split("\n\n")
        .map(|block| {
            format!(
                "<p>{}</p>",
                block.lines().map(escape).collect::<Vec<_>>().join("<br>")
            )
        })
        .collect()
}

/// Puts the draft into its account's drafts folder in place of the copy before. Returns false
/// when there was nothing to do: no change since the last upload, nothing written, or no drafts
/// folder.
pub async fn upload(db: &Database, secrets: &dyn SecretStore, key: &str) -> Result<bool> {
    upload_with(db, secrets, key, &imap::platform_connector()?).await
}

pub(crate) async fn upload_with(
    db: &Database,
    secrets: &dyn SecretStore,
    key: &str,
    connector: &TlsConnector,
) -> Result<bool> {
    let stored = {
        let conn = db.conn();
        conn.query_row(
            &format!(
                "SELECT {COLUMNS}, message_id, updated_at, uploaded_to, uploaded_at
                 FROM drafts WHERE key = ?1"
            ),
            [key],
            |row| {
                Ok((
                    from_row(row)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, i64>(11)?,
                    row.get::<_, Option<String>>(12)?,
                    row.get::<_, Option<i64>>(13)?,
                ))
            },
        )
        .optional()?
    };
    let Some((draft, message_id, updated_at, uploaded_to, uploaded_at)) = stored else {
        return Ok(false);
    };
    if uploaded_at.is_some_and(|at| at >= updated_at) || draft.is_empty() {
        return Ok(false);
    }
    let Some(path) = drafts_path(&db.conn(), &draft.account_id)? else {
        return Ok(false);
    };
    let mail = OutgoingMail {
        account_id: draft.account_id.clone(),
        to: draft.to.clone(),
        cc: draft.cc.clone(),
        bcc: draft.bcc.clone(),
        subject: draft.subject.clone(),
        html: draft.html.clone(),
        text: draft.text.clone(),
        in_reply_to: (draft.mode != ComposeMode::Forward)
            .then(|| draft.source_id.clone())
            .flatten(),
    };
    let (_, copy) = send::compose(&db.conn(), &mail, &message_id, true)?;

    let (settings, credential) =
        crate::mail::sync::credentials(db, secrets, &draft.account_id).await?;
    let mut session = imap::open_session(&settings, &credential, connector).await?;
    let uid_expunge = with_timeout(COMMAND_TIMEOUT, session.capabilities())
        .await?
        .map_err(protocol_error)?
        .has_str("UIDPLUS");
    with_timeout(COMMAND_TIMEOUT, session.select(&path))
        .await?
        .map_err(protocol_error)?;
    let before = copies(&mut session, &message_id).await?;
    with_timeout(
        COMMAND_TIMEOUT,
        session.append(&path, Some("(\\Seen \\Draft)"), None, &copy),
    )
    .await?
    .map_err(protocol_error)?;
    if !before.is_empty() {
        let set = uid_set(&before);
        if let Err(reason) = crate::mail::push::expunge(&mut session, &set, uid_expunge).await? {
            tracing::warn!(%reason, "the old copy of a draft stayed on the server");
        }
    }
    let _ = with_timeout(COMMAND_TIMEOUT, session.logout()).await;
    forget_cached(&db.conn(), &draft.account_id, &message_id)?;

    // Moved to another account: the copy there goes.
    if let Some(old) = uploaded_to.filter(|old| *old != draft.account_id) {
        let copy = ServerCopy {
            account_id: old,
            message_id: message_id.clone(),
        };
        if let Err(error) = remove_copy_with(db, secrets, &copy, connector).await {
            tracing::warn!(%error, "the draft's copy in its former account stayed");
        }
    }
    db.conn().execute(
        "UPDATE drafts SET uploaded_to = ?2, uploaded_at = ?3 WHERE key = ?1",
        params![key, draft.account_id, updated_at],
    )?;
    tracing::info!(account = %draft.account_id, "draft saved in the drafts folder");
    Ok(true)
}

/// Removes a draft's copy from the drafts folder, after sending or discarding it.
pub async fn remove_copy(
    db: &Database,
    secrets: &dyn SecretStore,
    copy: &ServerCopy,
) -> Result<()> {
    remove_copy_with(db, secrets, copy, &imap::platform_connector()?).await
}

pub(crate) async fn remove_copy_with(
    db: &Database,
    secrets: &dyn SecretStore,
    copy: &ServerCopy,
    connector: &TlsConnector,
) -> Result<()> {
    forget_cached(&db.conn(), &copy.account_id, &copy.message_id)?;
    let Some(path) = drafts_path(&db.conn(), &copy.account_id)? else {
        return Ok(());
    };
    let (settings, credential) =
        crate::mail::sync::credentials(db, secrets, &copy.account_id).await?;
    let mut session = imap::open_session(&settings, &credential, connector).await?;
    let uid_expunge = with_timeout(COMMAND_TIMEOUT, session.capabilities())
        .await?
        .map_err(protocol_error)?
        .has_str("UIDPLUS");
    with_timeout(COMMAND_TIMEOUT, session.select(&path))
        .await?
        .map_err(protocol_error)?;
    let found = copies(&mut session, &copy.message_id).await?;
    if !found.is_empty() {
        let set = uid_set(&found);
        if let Err(reason) = crate::mail::push::expunge(&mut session, &set, uid_expunge).await? {
            tracing::warn!(%reason, "a draft's copy stayed on the server");
        }
    }
    let _ = with_timeout(COMMAND_TIMEOUT, session.logout()).await;
    Ok(())
}

fn drafts_path(conn: &Connection, account_id: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT path FROM folders WHERE account_id = ?1 AND role = ?2",
            params![account_id, FolderRole::Drafts.as_str()],
            |row| row.get(0),
        )
        .optional()?)
}

async fn copies(session: &mut imap::ImapSession, message_id: &str) -> Result<Vec<u32>> {
    let mut uids: Vec<u32> = with_timeout(
        COMMAND_TIMEOUT,
        session.uid_search(format!("HEADER Message-ID \"{message_id}\"")),
    )
    .await?
    .map_err(protocol_error)?
    .into_iter()
    .collect();
    uids.sort_unstable();
    Ok(uids)
}

fn uid_set(uids: &[u32]) -> String {
    uids.iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

/// The cached copies of a draft in its drafts folder go at once; the next sync brings the new one.
fn forget_cached(conn: &Connection, account_id: &str, message_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM messages WHERE account_id = ?1 AND message_id = ?2
           AND folder_id IN (SELECT id FROM folders WHERE account_id = ?1 AND role = 'drafts')",
        params![account_id, message_id.trim_matches(['<', '>'])],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::{Login, NewAccount, save as save_account};
    use crate::mail::folders::RemoteFolder;
    use crate::mail::imap::ConnectionReport;
    use crate::mail::settings::{ImapSettings, Security};
    use crate::mail::sync::sync_with_connector;
    use crate::mail::test_server::{self, Mailbox, Mode, PASSWORD, TestServer};
    use crate::secrets::{MemoryStore, Secret};

    struct Setup {
        db: Database,
        secrets: MemoryStore,
        account_id: String,
        server: TestServer,
    }

    async fn setup() -> Setup {
        let server = test_server::start_with(Mode::Tls, Mailbox::with_messages(2)).await;
        let db = Database::open_in_memory().unwrap();
        let secrets = MemoryStore::default();
        let account = save_account(
            &db,
            &secrets,
            &NewAccount {
                display_name: "Personal".into(),
                email_address: "alex@example.org".into(),
                imap: ImapSettings {
                    host: "localhost".into(),
                    port: server.port,
                    security: Security::Tls,
                    username: "alex@example.org".into(),
                },
            },
            Login::Password(&Secret::new(PASSWORD)),
            &ConnectionReport {
                folders: vec![
                    RemoteFolder::new("INBOX", Some("/"), None, true),
                    RemoteFolder::new("Drafts", Some("/"), Some(FolderRole::Drafts), true),
                ],
            },
        )
        .unwrap();
        sync_with_connector(&db, &secrets, &account.id, &server.connector)
            .await
            .unwrap();
        Setup {
            db,
            secrets,
            account_id: account.id,
            server,
        }
    }

    fn draft(account_id: &str, subject: &str) -> Draft {
        Draft {
            key: "new:1".into(),
            mode: ComposeMode::New,
            source_id: None,
            account_id: account_id.into(),
            to: Vec::new(),
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: subject.into(),
            html: "<p>Erster Gedanke</p>".into(),
            text: "Erster Gedanke\n".into(),
        }
    }

    impl Setup {
        async fn upload(&self) -> bool {
            upload_with(&self.db, &self.secrets, "new:1", &self.server.connector)
                .await
                .unwrap()
        }

        /// Subjects of the drafts the server holds.
        fn on_server(&self) -> Vec<String> {
            self.server
                .mailbox
                .lock()
                .unwrap()
                .messages
                .iter()
                .filter(|m| !m.deleted)
                .filter_map(|m| {
                    String::from_utf8_lossy(&m.raw)
                        .lines()
                        .find_map(|line| line.strip_prefix("Subject: "))
                        .filter(|subject| subject.starts_with("Entwurf"))
                        .map(str::to_owned)
                })
                .collect()
        }
    }

    #[tokio::test]
    async fn a_draft_is_kept_here_and_replaced_on_the_server() {
        let setup = setup().await;
        // Nothing written: kept here, not uploaded.
        let empty = Draft {
            html: "<p></p>".into(),
            text: "\n".into(),
            ..draft(&setup.account_id, " ")
        };
        save(&setup.db.conn(), &empty).unwrap();
        assert!(!setup.upload().await);

        // An address still being typed does not keep it off the server.
        let typing = Draft {
            to: vec![Address {
                name: None,
                address: "len".into(),
            }],
            ..draft(&setup.account_id, "Entwurf eins")
        };
        save(&setup.db.conn(), &typing).unwrap();
        assert_eq!(list(&setup.db.conn()).unwrap()[0].subject, "Entwurf eins");
        assert!(setup.upload().await);
        assert_eq!(setup.on_server(), ["Entwurf eins"]);
        // Unchanged: nothing to do.
        assert!(!setup.upload().await);

        save(&setup.db.conn(), &draft(&setup.account_id, "Entwurf zwei")).unwrap();
        assert!(setup.upload().await);
        assert_eq!(setup.on_server(), ["Entwurf zwei"]);

        // Sent or discarded: gone here and there.
        let copy = take(&setup.db.conn(), "new:1").unwrap().unwrap();
        assert_eq!(copy.account_id, setup.account_id);
        remove_copy_with(&setup.db, &setup.secrets, &copy, &setup.server.connector)
            .await
            .unwrap();
        assert!(setup.on_server().is_empty());
        assert!(list(&setup.db.conn()).unwrap().is_empty());
        assert!(take(&setup.db.conn(), "new:1").unwrap().is_none());
    }

    #[tokio::test]
    async fn a_draft_from_the_drafts_folder_opens_to_write_on() {
        let setup = setup().await;
        save(&setup.db.conn(), &draft(&setup.account_id, "Entwurf drei")).unwrap();
        assert!(setup.upload().await);
        // The upload made it the server's; a sync brings the copy into the cache.
        sync_with_connector(
            &setup.db,
            &setup.secrets,
            &setup.account_id,
            &setup.server.connector,
        )
        .await
        .unwrap();
        let drafts_folder: String = setup
            .db
            .conn()
            .query_row("SELECT id FROM folders WHERE role = 'drafts'", [], |row| {
                row.get(0)
            })
            .unwrap();
        crate::mail::sync::sync_folder_with_connector(
            &setup.db,
            &setup.secrets,
            &drafts_folder,
            &setup.server.connector,
        )
        .await
        .unwrap();
        let cached: String = setup
            .db
            .conn()
            .query_row(
                "SELECT id FROM messages WHERE folder_id = ?1 AND subject = 'Entwurf drei'",
                [&drafts_folder],
                |row| row.get(0),
            )
            .unwrap();

        // Kept here: the local draft opens.
        assert_eq!(open_cached(&setup.db.conn(), &cached).unwrap().key, "new:1");

        // From another device: taken from the cached copy, which stays its server copy.
        take(&setup.db.conn(), "new:1").unwrap();
        let opened = open_cached(&setup.db.conn(), &cached).unwrap();
        assert!(opened.key.starts_with("draft:<"));
        assert_eq!(opened.subject, "Entwurf drei");
        assert!(opened.html.contains("Erster Gedanke"));
        assert_eq!(list(&setup.db.conn()).unwrap().len(), 1);
        let copy = take(&setup.db.conn(), &opened.key).unwrap().unwrap();
        assert_eq!(copy.message_id, opened.key["draft:".len()..]);
    }
}
