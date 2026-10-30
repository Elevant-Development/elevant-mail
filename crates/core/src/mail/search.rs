//! Searching on the server for mail the cache does not have: older than the account's period, or
//! with the words deep in a large mail of which only the start is cached. Runs when the user asks
//! for it ("search on the server"), one account at a time, over all its folders except trash,
//! spam and drafts; on Gmail only in "All Mail", which holds everything else.
//!
//! Found mail is fetched into the cache like the sync does it, marked `by_search`: it shows in
//! search results and conversations, but its folder's list leaves it out until the sync gets that
//! far (mail::sync adopts it then).

use std::collections::BTreeSet;

use rusqlite::{Connection, params};
use tokio_rustls::TlsConnector;

use crate::db::Database;
use crate::mail::imap::{self, COMMAND_TIMEOUT, ImapSession, protocol_error, with_timeout};
use crate::mail::sync::{FETCH_CHUNK, FolderState, credentials, fetch_and_store, folder_state};
use crate::query::SearchQuery;
use crate::secrets::SecretStore;
use crate::{Error, Result};

/// The newest matches fetched per folder; older ones stay on the server.
const FOUND_PER_FOLDER: usize = 50;

/// Searches the account's folders on the server and returns the matching messages, now all in
/// the cache, for [`crate::mailbox::search_messages`] to list along with what the cache found.
pub async fn search_server(
    db: &Database,
    secrets: &dyn SecretStore,
    account_id: &str,
    input: &str,
) -> Result<Vec<String>> {
    search_with(db, secrets, account_id, input, &imap::platform_connector()?).await
}

pub(crate) async fn search_with(
    db: &Database,
    secrets: &dyn SecretStore,
    account_id: &str,
    input: &str,
    connector: &TlsConnector,
) -> Result<Vec<String>> {
    let query = SearchQuery::parse(input);
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let (settings, credential) = credentials(db, secrets, account_id).await?;
    let mut session = imap::open_session(&settings, &credential, connector).await?;
    let result = search_in(db, &mut session, account_id, &query).await;
    let _ = with_timeout(COMMAND_TIMEOUT, session.logout()).await;
    result
}

async fn search_in(
    db: &Database,
    session: &mut ImapSession,
    account_id: &str,
    query: &SearchQuery,
) -> Result<Vec<String>> {
    let capabilities = with_timeout(COMMAND_TIMEOUT, session.capabilities())
        .await?
        .map_err(protocol_error)?;
    let literals = capabilities.has_str("LITERAL+") || capabilities.has_str("LITERAL-");
    let Some(criteria) = query.imap_criteria(literals) else {
        return Ok(Vec::new());
    };

    // Its own statement: a guard in the `for` head would hold the database for the whole loop.
    let folders = searched_folders(&db.conn(), account_id)?;
    let mut found = Vec::new();
    for folder_id in folders {
        let Some(folder) = folder_state(&db.conn(), &folder_id)? else {
            continue;
        };
        match search_folder(db, session, &folder, &criteria).await {
            Ok(ids) => found.extend(ids),
            // A folder the server refuses (gone since the last sync, or no search there) does
            // not stop the others; a broken connection does.
            Err(Error::Protocol(error)) => {
                tracing::info!(folder = %folder.path, %error, "server search skipped a folder");
            }
            Err(error) => return Err(error),
        }
    }
    Ok(found)
}

/// Gmail's "All Mail" where there is one, otherwise every folder but trash, spam and drafts.
fn searched_folders(conn: &Connection, account_id: &str) -> Result<Vec<String>> {
    let all: Option<String> = conn
        .query_row(
            "SELECT id FROM folders WHERE account_id = ?1 AND role = 'all'",
            [account_id],
            |row| row.get(0),
        )
        .ok();
    if let Some(all) = all {
        return Ok(vec![all]);
    }
    let folders = conn
        .prepare(
            "SELECT id FROM folders
             WHERE account_id = ?1 AND coalesce(role, '') NOT IN ('trash', 'junk', 'drafts')
             ORDER BY role = 'inbox' DESC, path",
        )?
        .query_map([account_id], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(folders)
}

async fn search_folder(
    db: &Database,
    session: &mut ImapSession,
    folder: &FolderState,
    criteria: &str,
) -> Result<Vec<String>> {
    let mailbox = with_timeout(COMMAND_TIMEOUT, session.examine(&folder.path))
        .await?
        .map_err(protocol_error)?;
    match folder.uid_validity {
        // The server numbered the folder anew: the next sync rebuilds it first.
        Some(known) if mailbox.uid_validity != Some(known) => return Ok(Vec::new()),
        Some(_) => {}
        // Never synced: remember the numbering, so the first sync can trust what is cached.
        None => {
            db.conn().execute(
                "UPDATE folders SET uid_validity = ?2 WHERE id = ?1",
                params![folder.id, mailbox.uid_validity],
            )?;
        }
    }

    let matches: BTreeSet<u32> = with_timeout(COMMAND_TIMEOUT, session.uid_search(criteria))
        .await?
        .map_err(protocol_error)?
        .into_iter()
        .collect();
    let newest: Vec<u32> = matches
        .iter()
        .rev()
        .take(FOUND_PER_FOLDER)
        .copied()
        .collect();
    let cached = cached_uids(&db.conn(), &folder.id, &newest)?;
    let missing: Vec<u32> = newest
        .iter()
        .filter(|uid| !cached.contains(uid))
        .copied()
        .collect();
    let folder = FolderState {
        uid_validity: mailbox.uid_validity,
        ..folder.clone()
    };
    for chunk in missing.chunks(FETCH_CHUNK) {
        fetch_and_store(db, session, &folder, chunk, true).await?;
    }
    ids_of(&db.conn(), &folder.id, &newest)
}

fn cached_uids(conn: &Connection, folder_id: &str, uids: &[u32]) -> Result<BTreeSet<u32>> {
    let mut statement =
        conn.prepare_cached("SELECT 1 FROM messages WHERE folder_id = ?1 AND uid = ?2")?;
    let mut cached = BTreeSet::new();
    for uid in uids {
        if statement.exists(params![folder_id, uid])? {
            cached.insert(*uid);
        }
    }
    Ok(cached)
}

fn ids_of(conn: &Connection, folder_id: &str, uids: &[u32]) -> Result<Vec<String>> {
    let mut statement = conn.prepare_cached(
        "SELECT id FROM messages WHERE folder_id = ?1 AND uid = ?2 AND leaving = 0",
    )?;
    let mut ids = Vec::new();
    for uid in uids {
        let mut rows = statement.query(params![folder_id, uid])?;
        if let Some(row) = rows.next()? {
            ids.push(row.get(0)?);
        }
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::{Login, NewAccount, save};
    use crate::mail::folders::RemoteFolder;
    use crate::mail::imap::ConnectionReport;
    use crate::mail::settings::{ImapSettings, Security};
    use crate::mail::sync::{load_older_with_connector, sync_with_connector};
    use crate::mail::test_server::{self, Mailbox, Mode, PASSWORD, TestMessage, TestServer};
    use crate::mailbox::{list_messages, search_messages};
    use crate::model::FolderRole;
    use crate::secrets::{MemoryStore, Secret};

    struct Setup {
        db: Database,
        secrets: MemoryStore,
        account_id: String,
        inbox_id: String,
        server: TestServer,
    }

    /// 130 messages, the newest 100 synced; an old one and a new one about a garden shed. The
    /// sync brings the server's folders: INBOX, Sent and drafts, all showing the same mail.
    async fn setup() -> Setup {
        let mut mailbox = Mailbox::with_messages(130);
        mailbox.messages[2] = TestMessage::new(3).with_content(
            "Jörg Müller <joerg@example.org>",
            "Angebot Gartenhaus",
            "Der Preis für das Gartenhaus steht unten.",
        );
        mailbox.messages[124] = TestMessage::new(125).with_content(
            "Lena Fischer <lena@example.org>",
            "Gartenhaus Termin",
            "Passt dir Samstag?",
        );
        let server = test_server::start_with(Mode::Tls, mailbox).await;
        let db = Database::open_in_memory().unwrap();
        let secrets = MemoryStore::default();
        let account = save(
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
                folders: vec![RemoteFolder::new("INBOX", Some("/"), None, true)],
            },
        )
        .unwrap();
        sync_with_connector(&db, &secrets, &account.id, &server.connector)
            .await
            .unwrap();
        let inbox_id = crate::mailbox::list_folders(&db.conn())
            .unwrap()
            .into_iter()
            .find(|folder| folder.role == Some(FolderRole::Inbox))
            .unwrap()
            .id;
        Setup {
            db,
            secrets,
            account_id: account.id,
            inbox_id,
            server,
        }
    }

    impl Setup {
        async fn search(&self, input: &str) -> Vec<String> {
            search_with(
                &self.db,
                &self.secrets,
                &self.account_id,
                input,
                &self.server.connector,
            )
            .await
            .unwrap()
        }

        /// Subjects of the found mail, read or not.
        fn subjects(&self, ids: &[String]) -> Vec<String> {
            let conn = self.db.conn();
            let mut subjects: Vec<String> = ["is:read", "is:unread"]
                .into_iter()
                .flat_map(|filter| search_messages(&conn, filter, false, ids).unwrap())
                .filter(|row| ids.contains(&row.latest.id))
                .filter_map(|row| row.latest.subject)
                .collect();
            subjects.sort();
            subjects.dedup();
            subjects
        }

        fn listed(&self) -> usize {
            list_messages(&self.db.conn(), &self.inbox_id, false)
                .unwrap()
                .len()
        }
    }

    #[tokio::test]
    async fn finds_old_mail_and_keeps_it_out_of_the_folder_list() {
        let setup = setup().await;
        assert_eq!(setup.listed(), 100);

        // In the inbox and in "Sent", which on the test server hold the same mail.
        let found = setup.search("gartenhaus").await;
        assert_eq!(found.len(), 4);
        // The old one is cached now and found by the local search too, but its folder's list
        // still ends where the sync stopped.
        let local: Vec<String> = search_messages(&setup.db.conn(), "gartenhaus", false, &found)
            .unwrap()
            .into_iter()
            .filter_map(|row| row.latest.subject)
            .collect();
        assert_eq!(local, ["Gartenhaus Termin", "Angebot Gartenhaus"]);
        assert_eq!(setup.listed(), 100);
        // Read-only, like the sync, and not in the drafts.
        let commands = setup.server.commands();
        assert!(
            commands
                .iter()
                .any(|c| c.starts_with("UID SEARCH TEXT \"gartenhaus\""))
        );
        assert!(
            !commands
                .iter()
                .any(|c| c.starts_with("EXAMINE") && c.contains("Entw"))
        );
        assert!(!commands.iter().any(|c| c.starts_with("SELECT")));

        // Loading older mail reaches it: the batch below the synced ones, not below the found one.
        let report = load_older_with_connector(
            &setup.db,
            &setup.secrets,
            &setup.inbox_id,
            &setup.server.connector,
        )
        .await
        .unwrap();
        assert_eq!(report.added, 29);
        assert_eq!(setup.listed(), 130);
    }

    #[tokio::test]
    async fn sender_umlauts_and_flags_reach_the_server() {
        let setup = setup().await;
        let found = setup.search("von:müller").await;
        assert_eq!(setup.subjects(&found), ["Angebot Gartenhaus"]);
        // Sent as a literal, which the server reads as the same text.
        assert!(
            setup
                .server
                .commands()
                .iter()
                .any(|c| c.contains("CHARSET UTF-8 FROM \"müller\""))
        );

        assert!(setup.search("gartenhaus ist:markiert").await.is_empty());
        // Nothing a server can check: no connection at all.
        let before = setup.server.commands().len();
        assert!(setup.search("hat:anhang").await.is_empty());
        assert!(setup.search("   ").await.is_empty());
        assert!(
            setup.server.commands()[before..]
                .iter()
                .all(|c| !c.starts_with("UID SEARCH"))
        );
    }
}
