//! Syncing an account's address books: over CardDAV (RFC 6352) for accounts with a password, over
//! Microsoft Graph for Microsoft accounts. They are switched on and off with the account's
//! calendars. The changes made here go first, then the server's state comes in. The server wins:
//! a change made here to a contact someone changed on the server since is dropped for the
//! server's version.
//!
//! The address books are looked for from the calendars' address (the same principal names both
//! homes), then where calendars would be looked for, at `/.well-known/carddav`.

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use ts_rs::TS;
use url::Url;

use super::graph::{self as contacts_graph, DEFAULT_FOLDER};
use crate::accounts::{self, SignInMethod};
use crate::calendar::caldav::{Condition, Dav, Kind, RemoteAddressBook, Written};
use crate::calendar::graph::{self, Graph};
use crate::calendar::sync::{candidates, look_for_home};
use crate::db::Database;
use crate::ids::{new_id, now_millis};
use crate::secrets::{SecretKind, SecretStore};
use crate::{Error, Result};

/// Characters kept as they are in the file name of a new contact: the UID, made safe for a path.
const FILE_NAME: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'@');

/// Looking again for address books that were not found, at most this often.
const LOOK_AGAIN_MS: i64 = 24 * 60 * 60 * 1000;

/// What a sync did, for the interface to update its views.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ContactsSyncReport {
    /// Address books or contacts changed in the cache.
    pub changed: bool,
}

struct Account {
    sign_in: SignInMethod,
    username: String,
    imap_host: String,
    email: String,
    enabled: bool,
    calendars: Option<String>,
    url: Option<String>,
    checked_at: Option<i64>,
}

fn account(conn: &Connection, account_id: &str) -> Result<Account> {
    let settings = accounts::settings(conn, account_id)?;
    let (email, enabled, calendars, url, checked_at) = conn
        .query_row(
            "SELECT email_address, caldav_enabled, caldav_url, carddav_url, carddav_checked_at
             FROM accounts WHERE id = ?1",
            [account_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, bool>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                ))
            },
        )
        .optional()?
        .ok_or(Error::AccountNotFound)?;
    Ok(Account {
        sign_in: settings.sign_in,
        username: settings.imap.username,
        imap_host: settings.imap.host,
        email,
        enabled,
        calendars,
        url,
        checked_at,
    })
}

/// Syncs the account's address books: finds them (once, or again a day after finding none),
/// sends what changed here, takes what changed there.
pub async fn sync_account(
    db: &Database,
    secrets: &dyn SecretStore,
    account_id: &str,
) -> Result<ContactsSyncReport> {
    let account = account(&db.conn(), account_id)?;
    if !account.enabled {
        return Ok(ContactsSyncReport::default());
    }
    match account.sign_in {
        SignInMethod::Password => {}
        SignInMethod::Microsoft => {
            let token = crate::oauth::graph_contacts_token(secrets, account_id).await?;
            let graph = Graph::new(graph::GRAPH, token)?;
            return sync_graph(db, &graph, account_id).await;
        }
    }
    let password = secrets.get(account_id, SecretKind::ImapPassword)?;
    let dav = Dav::new(&account.username, password)?;
    let home = match account.url.as_deref().and_then(|url| Url::parse(url).ok()) {
        Some(home) => home,
        None => {
            if account
                .checked_at
                .is_some_and(|checked| now_millis() - checked < LOOK_AGAIN_MS)
            {
                return Ok(ContactsSyncReport::default());
            }
            let mut addresses: Vec<Url> = account
                .calendars
                .as_deref()
                .and_then(|url| Url::parse(url).ok())
                .into_iter()
                .collect();
            addresses.extend(candidates(
                &account.imap_host,
                &account.email,
                Kind::Contacts,
            ));
            let found = look_for_home(&dav, &addresses, true, Kind::Contacts).await?;
            let conn = db.conn();
            match found {
                Some(home) => {
                    conn.execute(
                        "UPDATE accounts SET carddav_url = ?2, carddav_checked_at = NULL
                         WHERE id = ?1",
                        params![account_id, home.as_str()],
                    )?;
                    tracing::info!(account = %account_id, "address books found");
                    home
                }
                None => {
                    conn.execute(
                        "UPDATE accounts SET carddav_checked_at = ?2 WHERE id = ?1",
                        params![account_id, now_millis()],
                    )?;
                    tracing::info!(account = %account_id, "no address books found");
                    return Ok(ContactsSyncReport::default());
                }
            }
        }
    };

    let remote = dav.address_books(&home).await?;
    let listed: Vec<ListedBook> = remote
        .iter()
        .map(|book| ListedBook {
            remote_id: book.url.to_string(),
            name: book.name.clone(),
            read_only: book.read_only,
        })
        .collect();
    let mut changed = store_address_books(&db.conn(), account_id, &listed)?;
    for book in &remote {
        changed |= sync_address_book(db, &dav, account_id, book).await?;
    }
    Ok(ContactsSyncReport { changed })
}

/// An address book as a server lists it: a CardDAV collection by its address, a Graph contact
/// folder by its id.
struct ListedBook {
    remote_id: String,
    name: String,
    read_only: bool,
}

/// The cached address book of the account with this id on the server, and its version.
fn book_of(
    conn: &Connection,
    account_id: &str,
    remote_id: &str,
) -> Result<(String, Option<String>)> {
    Ok(conn.query_row(
        "SELECT id, sync_token FROM address_books WHERE account_id = ?1 AND remote_id = ?2",
        params![account_id, remote_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?)
}

/// The changes made here in an address book that wait for its server, oldest first.
fn pending_changes(conn: &Connection, book_id: &str) -> Result<Vec<Pending>> {
    Ok(conn
        .prepare(
            "SELECT id, uid, vcard, remote_id, etag, pending FROM contacts
             WHERE address_book_id = ?1 AND pending IS NOT NULL ORDER BY changed_at",
        )?
        .query_map([book_id], |row| {
            Ok(Pending {
                id: row.get(0)?,
                uid: row.get(1)?,
                vcard: row.get(2)?,
                remote_id: row.get(3)?,
                etag: row.get(4)?,
                kind: row.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?)
}

/// The cached contacts of an address book that are on the server, with their versions.
fn cached_versions(conn: &Connection, book_id: &str) -> Result<Vec<(String, Option<String>)>> {
    Ok(conn
        .prepare(
            "SELECT remote_id, etag FROM contacts
             WHERE address_book_id = ?1 AND remote_id IS NOT NULL",
        )?
        .query_map([book_id], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?)
}

/// Takes what was fetched (address, version, vCard) and drops what the server no longer lists,
/// unless it waits to go there. Returns how many were removed.
fn take_remote(
    db: &Database,
    book_id: &str,
    fetched: &[(String, Option<String>, String)],
    listed: &[&str],
    cached: &[(String, Option<String>)],
    version: Option<&str>,
) -> Result<usize> {
    let mut conn = db.conn();
    let tx = conn.transaction()?;
    for (remote_id, etag, text) in fetched {
        super::store_remote(&tx, book_id, remote_id, etag.as_deref(), text)?;
    }
    let mut removed = 0;
    for (remote_id, _) in cached {
        if !listed.contains(&remote_id.as_str()) {
            removed += tx.execute(
                "DELETE FROM contacts
                 WHERE address_book_id = ?1 AND remote_id = ?2 AND pending IS NULL",
                params![book_id, remote_id],
            )?;
        }
    }
    tx.execute(
        "UPDATE address_books SET sync_token = ?2 WHERE id = ?1",
        params![book_id, version],
    )?;
    tx.commit()?;
    Ok(removed)
}

/// The contact folders of a Microsoft account over Graph: the default one and those in it.
pub(crate) async fn sync_graph(
    db: &Database,
    graph: &Graph,
    account_id: &str,
) -> Result<ContactsSyncReport> {
    let mut listed = vec![ListedBook {
        remote_id: DEFAULT_FOLDER.to_owned(),
        // The interface names the default folder.
        name: String::new(),
        read_only: false,
    }];
    listed.extend(
        graph
            .contact_folders()
            .await?
            .into_iter()
            .map(|folder| ListedBook {
                remote_id: folder.id,
                name: folder.display_name,
                read_only: false,
            }),
    );
    let mut changed = store_address_books(&db.conn(), account_id, &listed)?;
    for book in &listed {
        changed |= sync_graph_folder(db, graph, account_id, &book.remote_id).await?;
    }
    Ok(ContactsSyncReport { changed })
}

/// Graph keeps no version of a folder: its contacts are listed each time and the changed ones
/// (by `changeKey`) taken.
async fn sync_graph_folder(
    db: &Database,
    graph: &Graph,
    account_id: &str,
    folder: &str,
) -> Result<bool> {
    let (book_id, _) = book_of(&db.conn(), account_id, folder)?;
    let pending = pending_changes(&db.conn(), &book_id)?;
    for change in &pending {
        send_graph_change(db, graph, folder, change).await?;
    }
    let contacts = graph.contacts(folder).await?;
    let cached = cached_versions(&db.conn(), &book_id)?;
    let fetched: Vec<(String, Option<String>, String)> = contacts
        .iter()
        .filter(|contact| {
            contact.change_key.is_none()
                || !cached
                    .iter()
                    .any(|(id, etag)| *id == contact.id && *etag == contact.change_key)
        })
        .map(|contact| {
            (
                contact.id.clone(),
                contact.change_key.clone(),
                contacts_graph::to_vcard(contact),
            )
        })
        .collect();
    let listed: Vec<&str> = contacts.iter().map(|contact| contact.id.as_str()).collect();
    let removed = take_remote(db, &book_id, &fetched, &listed, &cached, None)?;
    tracing::info!(
        fetched = fetched.len(),
        removed,
        sent = pending.len(),
        "contact folder synced"
    );
    Ok(!fetched.is_empty() || removed > 0 || !pending.is_empty())
}

/// One change to Graph. The contact comes back with the next listing, as Graph keeps it.
async fn send_graph_change(
    db: &Database,
    graph: &Graph,
    folder: &str,
    change: &Pending,
) -> Result<()> {
    let conn = || db.conn();
    let written = match (change.kind.as_str(), change.remote_id.as_deref()) {
        ("delete", Some(id)) => graph.delete_contact(id, change.etag.as_deref()).await?,
        ("delete", None) => graph::Written::Done(String::new()),
        (kind, remote_id) => {
            let Some(body) = contacts_graph::to_graph(&change.vcard) else {
                conn().execute(
                    "UPDATE contacts SET pending = NULL WHERE id = ?1",
                    [&change.id],
                )?;
                return Ok(());
            };
            match (kind, remote_id) {
                ("update", Some(id)) => {
                    graph
                        .update_contact(id, change.etag.as_deref(), &body)
                        .await?
                }
                _ => graph::Written::Done(graph.create_contact(folder, &body).await?),
            }
        }
    };
    match (change.kind.as_str(), written) {
        ("delete", graph::Written::Done(_)) => {
            conn().execute("DELETE FROM contacts WHERE id = ?1", [&change.id])?;
        }
        (_, graph::Written::Done(id)) => {
            conn().execute(
                "UPDATE contacts SET pending = NULL, remote_id = ?2, etag = NULL WHERE id = ?1",
                params![change.id, id],
            )?;
        }
        // Changed there since: the server's version comes with the listing.
        (_, graph::Written::Conflict) => {
            conn().execute(
                "UPDATE contacts SET pending = NULL, etag = NULL WHERE id = ?1",
                [&change.id],
            )?;
        }
    }
    Ok(())
}

/// Takes the server's list of address books: gone ones go with their contacts.
fn store_address_books(conn: &Connection, account_id: &str, remote: &[ListedBook]) -> Result<bool> {
    let mut changed = false;
    let known: Vec<(String, String)> = conn
        .prepare("SELECT id, remote_id FROM address_books WHERE account_id = ?1")?
        .query_map([account_id], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (position, book) in (0_i64..).zip(remote) {
        let url = book.remote_id.as_str();
        match known.iter().find(|(_, remote_id)| remote_id == url) {
            Some((id, _)) => {
                changed |= conn.execute(
                    "UPDATE address_books SET name = ?2, read_only = ?3, position = ?4
                     WHERE id = ?1 AND (name, read_only, position) IS NOT (?2, ?3, ?4)",
                    params![id, book.name, book.read_only, position],
                )? > 0;
            }
            None => {
                conn.execute(
                    "INSERT INTO address_books (id, account_id, remote_id, name, read_only, position)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        new_id(),
                        account_id,
                        url,
                        book.name,
                        book.read_only,
                        position
                    ],
                )?;
                changed = true;
            }
        }
    }
    for (id, remote_id) in &known {
        if !remote.iter().any(|book| book.remote_id == *remote_id) {
            conn.execute("DELETE FROM address_books WHERE id = ?1", [id])?;
            changed = true;
        }
    }
    Ok(changed)
}

/// A change made here that waits for the server.
struct Pending {
    id: String,
    uid: String,
    vcard: String,
    remote_id: Option<String>,
    etag: Option<String>,
    kind: String,
}

/// Sends the changes made in one address book, then takes what changed on the server.
async fn sync_address_book(
    db: &Database,
    dav: &Dav,
    account_id: &str,
    book: &RemoteAddressBook,
) -> Result<bool> {
    let (book_id, version) = book_of(&db.conn(), account_id, book.url.as_str())?;
    let pending = pending_changes(&db.conn(), &book_id)?;
    for change in &pending {
        send_change(db, dav, book, change).await?;
    }
    // Nothing changed there, and nothing went from here: the cache is current.
    if pending.is_empty() && version.is_some() && version == book.version {
        return Ok(false);
    }

    let listed = dav.etags(&book.url).await?;
    let cached = cached_versions(&db.conn(), &book_id)?;
    let wanted: Vec<Url> = listed
        .iter()
        .filter(|(url, etag)| {
            etag.is_none()
                || !cached
                    .iter()
                    .any(|(id, cached)| id == url.as_str() && cached == etag)
        })
        .map(|(url, _)| url.clone())
        .collect();
    let fetched: Vec<(String, Option<String>, String)> = dav
        .fetch(&book.url, &wanted, Kind::Contacts)
        .await?
        .into_iter()
        .map(|contact| (contact.url.to_string(), contact.etag, contact.data))
        .collect();
    let listed: Vec<&str> = listed.iter().map(|(url, _)| url.as_str()).collect();
    let removed = take_remote(
        db,
        &book_id,
        &fetched,
        &listed,
        &cached,
        book.version.as_deref(),
    )?;
    tracing::info!(
        fetched = fetched.len(),
        removed,
        sent = pending.len(),
        "address book synced"
    );
    Ok(!fetched.is_empty() || removed > 0 || !pending.is_empty())
}

async fn send_change(
    db: &Database,
    dav: &Dav,
    book: &RemoteAddressBook,
    change: &Pending,
) -> Result<()> {
    let target = match &change.remote_id {
        Some(url) => Url::parse(url).map_err(|_| Error::InvalidLink)?,
        None => book
            .url
            .join(&format!(
                "{}.vcf",
                percent_encoding::utf8_percent_encode(&change.uid, FILE_NAME)
            ))
            .map_err(|_| Error::InvalidLink)?,
    };
    let conn = || db.conn();
    match change.kind.as_str() {
        "delete" => match dav
            .delete(&target, change.etag.as_deref(), Kind::Contacts)
            .await?
        {
            Written::Done(_) => {
                conn().execute("DELETE FROM contacts WHERE id = ?1", [&change.id])?;
            }
            // Changed there since: the server's version comes back with the sync.
            Written::Conflict => {
                conn().execute(
                    "UPDATE contacts SET pending = NULL, etag = NULL WHERE id = ?1",
                    [&change.id],
                )?;
            }
        },
        kind => {
            let condition = match (kind, change.etag.as_deref()) {
                ("create", _) => Condition::New,
                (_, Some(etag)) => Condition::Version(etag),
                (_, None) => Condition::Any,
            };
            let written = dav
                .put(&target, &change.vcard, condition, Kind::Contacts)
                .await?;
            let etag = match &written {
                Written::Done(etag) => etag.clone(),
                // Someone else's version wins; it is fetched below.
                Written::Conflict => None,
            };
            conn().execute(
                "UPDATE contacts SET pending = NULL, remote_id = ?2, etag = ?3 WHERE id = ?1",
                params![change.id, target.as_str(), etag],
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::{Login, NewAccount, save};
    use crate::calendar::caldav_test_server::{self, CaldavServer, PASSWORD};
    use crate::calendar::sync::set_caldav;
    use crate::contacts::{self, ContactInput, ContactLabel, Labeled};
    use crate::mail::imap::ConnectionReport;
    use crate::mail::settings::{ImapSettings, Security};
    use crate::secrets::{MemoryStore, Secret};

    const JONAS: &str = "BEGIN:VCARD\r\nVERSION:3.0\r\nUID:jonas-1\r\nFN:Jonas Weber\r\n\
N:Weber;Jonas;;;\r\nEMAIL;TYPE=INTERNET,WORK:jonas@example.com\r\nTEL;TYPE=CELL:+49 170 1\r\n\
X-SOGO-CUSTOM:bleibt\r\nEND:VCARD\r\n";
    const DIRECTORY: &str = "BEGIN:VCARD\r\nVERSION:3.0\r\nUID:boss\r\nFN:Thomas Brandt\r\n\
EMAIL:thomas@example.com\r\nEND:VCARD\r\n";

    struct Setup {
        db: Database,
        secrets: MemoryStore,
        account_id: String,
        server: CaldavServer,
    }

    /// An account whose calendars were found at the server's `/.well-known/caldav`; the address
    /// books `contacts` (with Jonas) and a read-only `directory` are on the same server.
    async fn setup() -> Setup {
        let server = caldav_test_server::start().await;
        {
            let mut store = server.store();
            store.add_calendar("work", "Arbeit", false);
            store.add_address_book("contacts", "Kontakte", false);
            store.add_address_book("directory", "Firmenverzeichnis", true);
            store.put("contacts", "jonas-1.vcf", JONAS);
            store.put("directory", "boss.vcf", DIRECTORY);
        }
        let db = Database::open_in_memory().unwrap();
        let secrets = MemoryStore::default();
        let account = save(
            &db,
            &secrets,
            &NewAccount {
                display_name: "Privat".into(),
                email_address: "alex@example.org".into(),
                imap: ImapSettings {
                    host: "127.0.0.1".into(),
                    port: 993,
                    security: Security::Tls,
                    username: caldav_test_server::USER.into(),
                },
            },
            Login::Password(&Secret::new(PASSWORD)),
            &ConnectionReport { folders: vec![] },
        )
        .unwrap();
        set_caldav(
            &db,
            &secrets,
            &account.id,
            true,
            Some(&server.url("/.well-known/caldav")),
        )
        .await
        .unwrap();
        Setup {
            db,
            secrets,
            account_id: account.id,
            server,
        }
    }

    impl Setup {
        async fn sync(&self) -> ContactsSyncReport {
            sync_account(&self.db, &self.secrets, &self.account_id)
                .await
                .unwrap()
        }

        fn book(&self, name: &str) -> contacts::AddressBook {
            contacts::list_address_books(&self.db.conn())
                .unwrap()
                .into_iter()
                .find(|book| book.name == name)
                .unwrap()
        }

        fn names(&self) -> Vec<String> {
            contacts::list_contacts(&self.db.conn(), None, "")
                .unwrap()
                .into_iter()
                .map(|contact| contact.name)
                .collect()
        }

        fn id_of(&self, name: &str) -> String {
            contacts::list_contacts(&self.db.conn(), None, name)
                .unwrap()
                .remove(0)
                .id
        }

        fn on_server(&self, slug: &str) -> Vec<String> {
            self.server.store().address_books[slug]
                .events
                .values()
                .map(|(_, text)| text.clone())
                .collect()
        }
    }

    fn input(details: contacts::ContactDetails) -> ContactInput {
        ContactInput {
            id: Some(details.id),
            address_book_id: details.address_book_id,
            given_name: details.given_name,
            family_name: details.family_name,
            company: details.company,
            emails: details.emails,
            phones: details.phones,
            addresses: details.addresses,
            birthday: details.birthday,
            note: details.note,
        }
    }

    #[tokio::test]
    async fn finds_the_address_books_beside_the_calendars() {
        let setup = setup().await;
        assert!(setup.sync().await.changed);
        assert!(setup.book("Firmenverzeichnis").read_only);
        assert!(!setup.book("Kontakte").read_only);
        assert_eq!(setup.names(), ["Jonas Weber", "Thomas Brandt"]);
        let found: Option<String> = setup
            .db
            .conn()
            .query_row(
                "SELECT carddav_url FROM accounts WHERE id = ?1",
                [&setup.account_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(found, Some(setup.server.url("/dav/addressbooks/alex/")));
        // Nothing changed: nothing fetched.
        assert!(!setup.sync().await.changed);
    }

    #[tokio::test]
    async fn changes_go_both_ways_and_keep_what_the_app_does_not_edit() {
        let setup = setup().await;
        setup.sync().await;
        let book = setup.book("Kontakte");

        // New here: on the server as a vCard of its own.
        let new = ContactInput {
            id: None,
            address_book_id: book.id.clone(),
            given_name: "Clara".into(),
            family_name: "Öztürk".into(),
            company: String::new(),
            emails: vec![Labeled {
                label: ContactLabel::Home,
                value: "clara@example.org".into(),
            }],
            phones: Vec::new(),
            addresses: Vec::new(),
            birthday: None,
            note: String::new(),
        };
        contacts::save_contact(&setup.db.conn(), &new).unwrap();
        // Changed here: only the number, the server's own property stays.
        let jonas = setup.id_of("Jonas");
        let mut changed = input(contacts::get_contact(&setup.db.conn(), &jonas).unwrap());
        changed.phones[0].value = "+49 170 2".into();
        contacts::save_contact(&setup.db.conn(), &changed).unwrap();
        setup.sync().await;

        let cards = setup.on_server("contacts");
        assert_eq!(cards.len(), 2);
        let jonas_card = cards
            .iter()
            .find(|card| card.contains("UID:jonas-1"))
            .unwrap();
        assert!(
            jonas_card.contains("TEL;TYPE=CELL:+49 170 2"),
            "{jonas_card}"
        );
        assert!(jonas_card.contains("X-SOGO-CUSTOM:bleibt"), "{jonas_card}");
        assert!(cards.iter().any(|card| card.contains("FN:Clara Öztürk")));
        let waiting: i64 = setup
            .db
            .conn()
            .query_row(
                "SELECT count(*) FROM contacts WHERE pending IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(waiting, 0);
        // Once only: the new contact is not fetched as a second one.
        assert_eq!(
            setup.names(),
            ["Clara Öztürk", "Jonas Weber", "Thomas Brandt"]
        );

        // Changed and deleted elsewhere.
        setup.server.store().put(
            "contacts",
            "jonas-1.vcf",
            &JONAS.replace("Jonas Weber", "Jonas Weber-Lang"),
        );
        let clara_file = setup.server.store().address_books["contacts"]
            .events
            .keys()
            .find(|file| *file != "jonas-1.vcf")
            .cloned()
            .unwrap();
        setup.server.store().remove("contacts", &clara_file);
        setup.sync().await;
        assert_eq!(setup.names(), ["Jonas Weber-Lang", "Thomas Brandt"]);

        // Deleted here.
        let jonas = setup.id_of("Jonas");

        contacts::delete_contact(&setup.db.conn(), &jonas).unwrap();
        setup.sync().await;
        assert!(setup.on_server("contacts").is_empty());
        assert_eq!(setup.names(), ["Thomas Brandt"]);
    }

    #[tokio::test]
    async fn the_server_wins_a_conflict() {
        let setup = setup().await;
        setup.sync().await;
        let jonas = setup.id_of("Jonas");
        let mut changed = input(contacts::get_contact(&setup.db.conn(), &jonas).unwrap());
        changed.company = "Hier geändert".into();
        contacts::save_contact(&setup.db.conn(), &changed).unwrap();
        setup.server.store().put(
            "contacts",
            "jonas-1.vcf",
            &JONAS.replace("Jonas Weber", "Jonas (dort)"),
        );
        setup.sync().await;
        let jonas = setup.id_of("Jonas");

        let details = contacts::get_contact(&setup.db.conn(), &jonas).unwrap();
        assert_eq!(details.name, "Jonas (dort)");
        assert_eq!(details.company, "");
    }

    #[tokio::test]
    async fn switching_the_calendars_off_takes_the_address_books_along() {
        let setup = setup().await;
        setup.sync().await;
        set_caldav(&setup.db, &setup.secrets, &setup.account_id, false, None)
            .await
            .unwrap();
        let books = contacts::list_address_books(&setup.db.conn()).unwrap();
        assert!(books.iter().all(|book| book.account_id.is_none()));
        assert!(!setup.sync().await.changed);
        assert!(setup.names().is_empty());
    }

    mod microsoft {
        use super::*;
        use crate::calendar::graph_test_server::{self, GraphServer, TOKEN};
        use crate::contacts::graph::tests::mira_json;
        use serde_json::json;

        async fn graph_setup() -> (Setup, GraphServer, Graph) {
            let setup = setup().await;
            let server = graph_test_server::start().await;
            {
                let mut store = server.store();
                store
                    .contact_folders
                    .push(("folder-team".into(), "Team".into()));
                store.put_contact("contacts", mira_json());
                store.put_contact(
                    "contacts",
                    json!({ "givenName": "Paul", "surname": "Becker", "displayName": "Paul Becker",
                            "emailAddresses": [{ "address": "paul@example.org" }] }),
                );
                store.put_contact(
                    "folder-team",
                    json!({ "givenName": "Sophie", "surname": "Wagner", "displayName": "Sophie Wagner" }),
                );
            }
            let graph = Graph::new(&server.base(), Secret::new(TOKEN)).unwrap();
            (setup, server, graph)
        }

        async fn sync(setup: &Setup, graph: &Graph) -> ContactsSyncReport {
            sync_graph(&setup.db, graph, &setup.account_id)
                .await
                .unwrap()
        }

        #[tokio::test]
        async fn takes_the_folders_and_their_contacts() {
            let (setup, _server, graph) = graph_setup().await;
            assert!(sync(&setup, &graph).await.changed);
            let books: Vec<String> = contacts::list_address_books(&setup.db.conn())
                .unwrap()
                .into_iter()
                .filter(|book| book.account_id.is_some())
                .map(|book| book.name)
                .collect();
            // The default folder has no name of its own; the interface gives it one.
            assert!(books.contains(&String::new()) && books.contains(&"Team".to_owned()));
            let names = setup.names();
            for name in ["Mira Schulz", "Paul Becker", "Sophie Wagner"] {
                assert!(names.contains(&name.to_owned()), "{names:?}");
            }
            let mira = setup.id_of("Mira");
            let details = contacts::get_contact(&setup.db.conn(), &mira).unwrap();
            assert_eq!(details.birthday.as_deref(), Some("--07-31"));
            assert_eq!(details.company, "Pretorix");
            // Nothing changed: nothing taken.
            assert!(!sync(&setup, &graph).await.changed);
        }

        #[tokio::test]
        async fn changes_made_here_reach_graph() {
            let (setup, server, graph) = graph_setup().await;
            sync(&setup, &graph).await;
            let team = contacts::list_address_books(&setup.db.conn())
                .unwrap()
                .into_iter()
                .find(|book| book.name == "Team")
                .unwrap();
            contacts::save_contact(
                &setup.db.conn(),
                &ContactInput {
                    id: None,
                    address_book_id: team.id.clone(),
                    given_name: "Clara".into(),
                    family_name: "Öztürk".into(),
                    company: String::new(),
                    emails: vec![Labeled {
                        label: ContactLabel::Home,
                        value: "clara@example.org".into(),
                    }],
                    phones: vec![Labeled {
                        label: ContactLabel::Mobile,
                        value: "+49 160 1".into(),
                    }],
                    addresses: Vec::new(),
                    birthday: Some("1990-03-14".into()),
                    note: String::new(),
                },
            )
            .unwrap();
            let mira = setup.id_of("Mira");
            let mut changed = input(contacts::get_contact(&setup.db.conn(), &mira).unwrap());
            changed.company = "Elevant".into();
            contacts::save_contact(&setup.db.conn(), &changed).unwrap();
            let paul = setup.id_of("Paul");
            contacts::delete_contact(&setup.db.conn(), &paul).unwrap();
            sync(&setup, &graph).await;

            {
                let store = server.store();
                let (folder, clara) = store
                    .contacts
                    .values()
                    .find(|(_, contact)| contact["givenName"] == "Clara")
                    .unwrap();
                assert_eq!(folder, "folder-team");
                assert_eq!(clara["mobilePhone"], "+49 160 1");
                assert_eq!(clara["birthday"], "1990-03-14T00:00:00Z");
                assert_eq!(store.contacts["AAMk-mira"].1["companyName"], "Elevant");
                // Mira's own birthday without a year stays one.
                assert_eq!(
                    store.contacts["AAMk-mira"].1["birthday"],
                    "1604-07-31T00:00:00Z"
                );
                assert!(
                    !store
                        .contacts
                        .values()
                        .any(|(_, contact)| contact["givenName"] == "Paul")
                );
            }
            // As Graph keeps them, once each.
            let names = setup.names();
            assert_eq!(
                names.iter().filter(|name| *name == "Clara Öztürk").count(),
                1
            );
            assert!(!names.contains(&"Paul Becker".to_owned()));
            let waiting: i64 = setup
                .db
                .conn()
                .query_row(
                    "SELECT count(*) FROM contacts WHERE pending IS NOT NULL",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(waiting, 0);

            // Deleted there: gone here.
            let clara = server
                .store()
                .contacts
                .iter()
                .find(|(_, (_, contact))| contact["givenName"] == "Clara")
                .map(|(id, _)| id.clone())
                .unwrap();
            server.store().contacts.remove(&clara);
            sync(&setup, &graph).await;
            assert!(!setup.names().contains(&"Clara Öztürk".to_owned()));
        }
    }
}
