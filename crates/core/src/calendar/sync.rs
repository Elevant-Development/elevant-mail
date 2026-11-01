//! Syncing an account's calendars: over CalDAV for accounts with a password, over Microsoft Graph
//! for Microsoft accounts. Either way the changes made here go first, then the server's state
//! comes in. The server wins: a change made here to an event someone changed on the server since
//! is dropped for the server's version.
//!
//! Where CalDAV calendars are is looked for on the mail server and the address's domain
//! (`/.well-known/caldav`, RFC 6764), after a few providers that keep them elsewhere; the user can
//! also give the address. Nothing is asked of any other server.

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use ts_rs::TS;
use url::Url;

use super::caldav::{self, Condition, Dav, Kind, RemoteCalendar, Written};
use super::events;
use super::graph::{self, Graph, GraphCalendar};
use crate::accounts::{self, SignInMethod};
use crate::db::Database;
use crate::ids::{new_id, now_millis};
use crate::secrets::{SecretKind, SecretStore};
use crate::{Error, Result};

/// Characters kept as they are in the file name of a new event: the UID, made safe for a path.
const FILE_NAME: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'@');

/// Looking again for calendars that were not found, at most this often.
const LOOK_AGAIN_MS: i64 = 24 * 60 * 60 * 1000;

/// What the account settings show about the account's calendars.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CaldavStatus {
    /// A Microsoft account: its calendars come over Microsoft Graph, not CalDAV.
    pub microsoft: bool,
    pub enabled: bool,
    /// Where the CalDAV calendars are, once found or set.
    pub url: Option<String>,
}

/// What a sync did, for the interface to update its views.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CalendarSyncReport {
    /// Calendars or events changed in the cache.
    pub changed: bool,
}

struct Account {
    sign_in: SignInMethod,
    username: String,
    imap_host: String,
    email: String,
    color: String,
    url: Option<String>,
    enabled: bool,
    checked_at: Option<i64>,
}

fn account(conn: &Connection, account_id: &str) -> Result<Account> {
    let settings = accounts::settings(conn, account_id)?;
    let (email, color, url, enabled, checked_at) = conn
        .query_row(
            "SELECT email_address, color, caldav_url, caldav_enabled, caldav_checked_at
             FROM accounts WHERE id = ?1",
            [account_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, bool>(3)?,
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
        color,
        url,
        enabled,
        checked_at,
    })
}

pub fn status(conn: &Connection, account_id: &str) -> Result<CaldavStatus> {
    let account = account(conn, account_id)?;
    Ok(CaldavStatus {
        microsoft: account.sign_in == SignInMethod::Microsoft,
        enabled: account.enabled,
        url: account.url,
    })
}

/// Where to look for an account's calendars or address books, in order.
pub(crate) fn candidates(imap_host: &str, email: &str, kind: Kind) -> Vec<Url> {
    let host = imap_host.trim().to_ascii_lowercase();
    let domain = email
        .rsplit_once('@')
        .map(|(_, domain)| domain.trim().to_ascii_lowercase())
        .unwrap_or_default();
    // Their calendars want an OAuth sign-in, which the app password does not give.
    let oauth_only = [
        "imap.gmail.com",
        "outlook.office365.com",
        "imap-mail.outlook.com",
    ];
    if oauth_only.contains(&host.as_str()) {
        return Vec::new();
    }
    let mut addresses = Vec::new();
    // Providers whose calendars and address books are not on the mail server.
    let known = match kind {
        Kind::Events => [
            ("mailbox.org", "https://dav.mailbox.org/"),
            ("posteo.de", "https://posteo.de:8443/"),
            ("imap.mail.me.com", "https://caldav.icloud.com/"),
        ],
        Kind::Contacts => [
            ("mailbox.org", "https://dav.mailbox.org/"),
            ("posteo.de", "https://posteo.de:8843/"),
            ("imap.mail.me.com", "https://contacts.icloud.com/"),
        ],
    };
    for (key, address) in known {
        if host == key || host.ends_with(&format!(".{key}")) || domain == key {
            addresses.push(address.to_owned());
        }
    }
    let well_known = match kind {
        Kind::Events => "caldav",
        Kind::Contacts => "carddav",
    };
    addresses.push(format!("https://{host}/.well-known/{well_known}"));
    if !domain.is_empty() && domain != host {
        addresses.push(format!("https://{domain}/.well-known/{well_known}"));
    }
    addresses
        .into_iter()
        .filter_map(|address| Url::parse(&address).ok())
        .collect()
}

/// Asks each address in turn; the first that names a home of the kind wins. Looking by itself
/// (`guessing`), a refused sign-in only means none for this password there; for an address the
/// user gave, it is the answer.
pub(crate) async fn look_for_home(
    dav: &Dav,
    addresses: &[Url],
    guessing: bool,
    kind: Kind,
) -> Result<Option<Url>> {
    for address in addresses {
        match dav.find_home(address, kind).await {
            Ok(Some(home)) => return Ok(Some(home)),
            Ok(None) => {}
            Err(error @ Error::AuthenticationFailed { .. }) if !guessing => return Err(error),
            // Another address may still answer.
            Err(error) => tracing::debug!(%address, %error, ?kind, "nothing there"),
        }
    }
    Ok(None)
}

async fn connect(secrets: &dyn SecretStore, account_id: &str, account: &Account) -> Result<Dav> {
    let password = secrets.get(account_id, SecretKind::ImapPassword)?;
    Dav::new(&account.username, password)
}

/// Switches the account's calendars and address books on or off, and sets where the calendars
/// are: `url` is a server address (the account's own is looked for when it is `None`). Checks it
/// before saving. The address books are looked for anew, from there.
pub async fn set_caldav(
    db: &Database,
    secrets: &dyn SecretStore,
    account_id: &str,
    enabled: bool,
    url: Option<&str>,
) -> Result<CaldavStatus> {
    let account = account(&db.conn(), account_id)?;
    if !enabled {
        let conn = db.conn();
        conn.execute(
            "UPDATE accounts SET caldav_enabled = 0, carddav_url = NULL, carddav_checked_at = NULL
             WHERE id = ?1",
            [account_id],
        )?;
        // The cache keeps no calendars or contacts it does not sync.
        conn.execute("DELETE FROM calendars WHERE account_id = ?1", [account_id])?;
        conn.execute(
            "DELETE FROM address_books WHERE account_id = ?1",
            [account_id],
        )?;
        return status(&conn, account_id);
    }
    let url = url.filter(|_| account.sign_in == SignInMethod::Password);
    let home = match url.map(str::trim).filter(|url| !url.is_empty()) {
        Some(url) => {
            let start = Url::parse(url)
                .or_else(|_| Url::parse(&format!("https://{url}")))
                .map_err(|_| Error::InvalidLink)?;
            if !caldav::allowed(&start) {
                return Err(Error::InvalidLink);
            }
            let dav = connect(secrets, account_id, &account).await?;
            Some(
                look_for_home(&dav, &[start], false, Kind::Events)
                    .await?
                    .ok_or(Error::CalendarsNotFound)?,
            )
        }
        None => None,
    };
    db.conn().execute(
        "UPDATE accounts SET caldav_enabled = 1, caldav_url = ?2, caldav_checked_at = NULL,
           carddav_url = NULL, carddav_checked_at = NULL
         WHERE id = ?1",
        params![account_id, home.as_ref().map(Url::as_str)],
    )?;
    status(&db.conn(), account_id)
}

/// Syncs the account's calendars: finds them (once, or again a day after finding none), sends
/// what changed here, takes what changed there.
pub async fn sync_account(
    db: &Database,
    secrets: &dyn SecretStore,
    account_id: &str,
) -> Result<CalendarSyncReport> {
    let account = account(&db.conn(), account_id)?;
    if !account.enabled {
        return Ok(CalendarSyncReport::default());
    }
    if account.sign_in == SignInMethod::Microsoft {
        let token = crate::oauth::graph_token(secrets, account_id).await?;
        let graph = Graph::new(graph::GRAPH, token)?;
        return sync_graph(db, &graph, account_id, &account.color).await;
    }
    let dav = connect(secrets, account_id, &account).await?;
    let home = match account.url.as_deref().and_then(|url| Url::parse(url).ok()) {
        Some(home) => home,
        None => {
            if account
                .checked_at
                .is_some_and(|checked| now_millis() - checked < LOOK_AGAIN_MS)
            {
                return Ok(CalendarSyncReport::default());
            }
            let addresses = candidates(&account.imap_host, &account.email, Kind::Events);
            let found = look_for_home(&dav, &addresses, true, Kind::Events).await?;
            let conn = db.conn();
            match found {
                Some(home) => {
                    conn.execute(
                        "UPDATE accounts SET caldav_url = ?2, caldav_checked_at = NULL
                         WHERE id = ?1",
                        params![account_id, home.as_str()],
                    )?;
                    tracing::info!(account = %account_id, "calendars found");
                    home
                }
                None => {
                    conn.execute(
                        "UPDATE accounts SET caldav_checked_at = ?2 WHERE id = ?1",
                        params![account_id, now_millis()],
                    )?;
                    tracing::info!(account = %account_id, "no calendars found");
                    return Ok(CalendarSyncReport::default());
                }
            }
        }
    };

    let remote = dav.calendars(&home).await?;
    // Servers that schedule by themselves (RFC 6638) send the invitations. Asked once, with the
    // first calendars; switching the calendars off and on asks again.
    let known: Option<bool> = db.conn().query_row(
        "SELECT max(can_invite) FROM calendars WHERE account_id = ?1 AND read_only = 0",
        [account_id],
        |row| row.get(0),
    )?;
    let schedules = match known {
        Some(schedules) => schedules,
        None => dav.schedules(&home).await.unwrap_or(false),
    };
    let listed: Vec<ListedCalendar> = remote
        .iter()
        .map(|calendar| ListedCalendar {
            remote_id: calendar.url.to_string(),
            name: calendar.name.clone(),
            color: calendar.color.clone(),
            read_only: calendar.read_only,
            can_invite: schedules && !calendar.read_only,
        })
        .collect();
    let mut changed = store_calendars(&db.conn(), account_id, &account.color, &listed)?;
    for calendar in &remote {
        changed |= sync_calendar(db, &dav, calendar).await?;
    }
    Ok(CalendarSyncReport { changed })
}

/// A calendar as a server lists it.
struct ListedCalendar {
    remote_id: String,
    name: String,
    color: Option<String>,
    read_only: bool,
    /// The server sends invitations for events with attendees.
    can_invite: bool,
}

/// Takes the server's list of calendars: new ones come in shown, gone ones go with their events.
/// Returns whether anything changed.
fn store_calendars(
    conn: &Connection,
    account_id: &str,
    account_color: &str,
    remote: &[ListedCalendar],
) -> Result<bool> {
    let mut changed = false;
    let known: Vec<(String, String)> = conn
        .prepare("SELECT id, remote_id FROM calendars WHERE account_id = ?1")?
        .query_map([account_id], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (position, calendar) in (0_i64..).zip(remote) {
        let url = calendar.remote_id.as_str();
        let color = calendar.color.as_deref().unwrap_or(account_color);
        match known.iter().find(|(_, remote_id)| remote_id == url) {
            Some((id, _)) => {
                changed |= conn.execute(
                    "UPDATE calendars SET name = ?2, read_only = ?3, position = ?4, can_invite = ?5
                     WHERE id = ?1
                       AND (name, read_only, position, can_invite) IS NOT (?2, ?3, ?4, ?5)",
                    params![
                        id,
                        calendar.name,
                        calendar.read_only,
                        position,
                        calendar.can_invite
                    ],
                )? > 0;
            }
            None => {
                conn.execute(
                    "INSERT INTO calendars (id, account_id, remote_id, name, color, read_only,
                       position, can_invite)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        new_id(),
                        account_id,
                        url,
                        calendar.name,
                        color,
                        calendar.read_only,
                        position,
                        calendar.can_invite
                    ],
                )?;
                changed = true;
            }
        }
    }
    for (id, remote_id) in &known {
        if !remote
            .iter()
            .any(|calendar| &calendar.remote_id == remote_id)
        {
            conn.execute("DELETE FROM calendars WHERE id = ?1", [id])?;
            changed = true;
        }
    }
    Ok(changed)
}

/// A change made here that waits for the server.
struct Pending {
    id: String,
    uid: String,
    ics: String,
    remote_id: Option<String>,
    etag: Option<String>,
    kind: String,
    /// The series itself changed, not only some of its dates (see `events.send_series`).
    send_series: bool,
    /// Its rule changed (see `events.send_rule`).
    send_rule: bool,
}

/// The changes made here in a calendar that wait for its server, oldest first.
fn pending_changes(conn: &Connection, calendar_id: &str) -> Result<Vec<Pending>> {
    Ok(conn
        .prepare(
            "SELECT id, uid, ics, remote_id, etag, pending, send_series, send_rule FROM events
             WHERE calendar_id = ?1 AND pending IS NOT NULL ORDER BY changed_at",
        )?
        .query_map([calendar_id], |row| {
            Ok(Pending {
                id: row.get(0)?,
                uid: row.get(1)?,
                ics: row.get(2)?,
                remote_id: row.get(3)?,
                etag: row.get(4)?,
                kind: row.get(5)?,
                send_series: row.get(6)?,
                send_rule: row.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?)
}

/// The dates of a series changed here on their own, for Graph: original start and kind.
fn pending_dates(conn: &Connection, event_id: &str) -> Result<Vec<(i64, String)>> {
    Ok(conn
        .prepare("SELECT instance, kind FROM pending_instances WHERE event_id = ?1")?
        .query_map([event_id], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?)
}

/// The cached events that are on the server, with the version each was fetched at.
fn cached_versions(conn: &Connection, calendar_id: &str) -> Result<Vec<(String, Option<String>)>> {
    Ok(conn
        .prepare(
            "SELECT remote_id, etag FROM events WHERE calendar_id = ?1 AND remote_id IS NOT NULL",
        )?
        .query_map([calendar_id], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?)
}

/// Whether the server's event `remote_id` at `version` is cached as it is.
fn is_current(cached: &[(String, Option<String>)], remote_id: &str, version: Option<&str>) -> bool {
    version.is_some()
        && cached
            .iter()
            .any(|(id, etag)| id == remote_id && etag.as_deref() == version)
}

/// Takes what was fetched (address, version, iCalendar) and drops what the server no longer
/// lists, unless it waits to go there. Returns how many were removed.
fn take_remote(
    db: &Database,
    calendar_id: &str,
    fetched: &[(String, Option<String>, String)],
    listed: &[&str],
    cached: &[(String, Option<String>)],
    version: Option<&str>,
) -> Result<usize> {
    let mut conn = db.conn();
    let tx = conn.transaction()?;
    for (remote_id, etag, ics) in fetched {
        events::store_remote(&tx, calendar_id, remote_id, etag.as_deref(), ics)?;
    }
    let mut removed = 0;
    for (remote_id, _) in cached {
        if !listed.contains(&remote_id.as_str()) {
            removed += tx.execute(
                "DELETE FROM events WHERE calendar_id = ?1 AND remote_id = ?2 AND pending IS NULL",
                params![calendar_id, remote_id],
            )?;
        }
    }
    tx.execute(
        "UPDATE calendars SET sync_token = ?2 WHERE id = ?1",
        params![calendar_id, version],
    )?;
    tx.commit()?;
    Ok(removed)
}

/// Sends the changes made in one calendar, then takes what changed on the server.
async fn sync_calendar(db: &Database, dav: &Dav, calendar: &RemoteCalendar) -> Result<bool> {
    let (calendar_id, version): (String, Option<String>) = db.conn().query_row(
        "SELECT id, sync_token FROM calendars WHERE remote_id = ?1",
        [calendar.url.as_str()],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let pending = pending_changes(&db.conn(), &calendar_id)?;
    for change in &pending {
        send_change(db, dav, calendar, change).await?;
    }
    // Nothing changed there, and nothing went from here: the cache is current.
    if pending.is_empty() && version.is_some() && version == calendar.version {
        return Ok(false);
    }

    let listed = dav.etags(&calendar.url).await?;
    let cached = cached_versions(&db.conn(), &calendar_id)?;
    let wanted: Vec<_> = listed
        .iter()
        .filter(|(url, etag)| !is_current(&cached, url.as_str(), etag.as_deref()))
        .map(|(url, _)| url.clone())
        .collect();
    let fetched: Vec<_> = dav
        .fetch(&calendar.url, &wanted, Kind::Events)
        .await?
        .into_iter()
        .map(|event| (event.url.to_string(), event.etag, event.data))
        .collect();
    let listed: Vec<&str> = listed.iter().map(|(url, _)| url.as_str()).collect();
    let removed = take_remote(
        db,
        &calendar_id,
        &fetched,
        &listed,
        &cached,
        calendar.version.as_deref(),
    )?;
    tracing::info!(
        fetched = fetched.len(),
        removed,
        sent = pending.len(),
        "calendar synced"
    );
    Ok(!fetched.is_empty() || removed > 0 || !pending.is_empty())
}

async fn send_change(
    db: &Database,
    dav: &Dav,
    calendar: &RemoteCalendar,
    change: &Pending,
) -> Result<()> {
    let target = match &change.remote_id {
        Some(url) => Url::parse(url).map_err(|_| Error::InvalidLink)?,
        None => calendar
            .url
            .join(&format!(
                "{}.ics",
                percent_encoding::utf8_percent_encode(&change.uid, FILE_NAME)
            ))
            .map_err(|_| Error::InvalidLink)?,
    };
    let conn = || db.conn();
    match change.kind.as_str() {
        "delete" => match dav
            .delete(&target, change.etag.as_deref(), Kind::Events)
            .await?
        {
            Written::Done(_) => {
                conn().execute("DELETE FROM events WHERE id = ?1", [&change.id])?;
            }
            // Changed there since: the server's version comes back with the sync.
            Written::Conflict => {
                conn().execute(
                    "UPDATE events SET pending = NULL, etag = NULL WHERE id = ?1",
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
                .put(&target, &change.ics, condition, Kind::Events)
                .await?;
            let etag = match &written {
                Written::Done(etag) => etag.clone(),
                // Someone else's version wins; it is fetched below.
                Written::Conflict => None,
            };
            conn().execute(
                "UPDATE events SET pending = NULL, remote_id = ?2, etag = ?3 WHERE id = ?1",
                params![change.id, target.as_str(), etag],
            )?;
        }
    }
    // Its changed dates went with the series' text.
    conn().execute(
        "DELETE FROM pending_instances WHERE event_id = ?1",
        [&change.id],
    )?;
    Ok(())
}

/// The calendars of a Microsoft account over Graph.
pub(crate) async fn sync_graph(
    db: &Database,
    graph: &Graph,
    account_id: &str,
    account_color: &str,
) -> Result<CalendarSyncReport> {
    let remote = graph.calendars().await?;
    let listed: Vec<ListedCalendar> = remote
        .iter()
        .map(|calendar| ListedCalendar {
            remote_id: calendar.id.clone(),
            name: calendar.name.clone(),
            color: calendar
                .hex_color
                .clone()
                .filter(|color| color.starts_with('#')),
            read_only: !calendar.can_edit,
            // Exchange sends the invitations.
            can_invite: calendar.can_edit,
        })
        .collect();
    let mut changed = store_calendars(&db.conn(), account_id, account_color, &listed)?;
    for calendar in &remote {
        changed |= sync_graph_calendar(db, graph, account_id, calendar).await?;
    }
    Ok(CalendarSyncReport { changed })
}

/// Graph keeps no version of a whole calendar: its events are listed with their `changeKey`
/// each time, and the changed ones fetched.
async fn sync_graph_calendar(
    db: &Database,
    graph: &Graph,
    account_id: &str,
    calendar: &GraphCalendar,
) -> Result<bool> {
    let calendar_id: String = db.conn().query_row(
        "SELECT id FROM calendars WHERE account_id = ?1 AND remote_id = ?2",
        params![account_id, calendar.id],
        |row| row.get(0),
    )?;
    let pending = pending_changes(&db.conn(), &calendar_id)?;
    let own: Option<String> = db
        .conn()
        .query_row(
            "SELECT email_address FROM accounts WHERE id = ?1",
            [account_id],
            |row| row.get(0),
        )
        .optional()?;
    for change in &pending {
        send_graph_change(db, graph, &calendar.id, own.as_deref(), change).await?;
    }
    let listed = graph.list(&calendar.id).await?;
    let cached = cached_versions(&db.conn(), &calendar_id)?;
    let wanted: Vec<&str> = listed
        .iter()
        .filter(|event| !is_current(&cached, &event.id, event.change_key.as_deref()))
        .map(|event| event.id.as_str())
        .collect();
    let fetched: Vec<_> = graph
        .events(&wanted)
        .await?
        .into_iter()
        .filter_map(|event| {
            let ics = graph::to_ical(&event)?;
            Some((event.id, event.change_key, ics))
        })
        .collect();
    let ids: Vec<&str> = listed.iter().map(|event| event.id.as_str()).collect();
    let removed = take_remote(db, &calendar_id, &fetched, &ids, &cached, None)?;
    tracing::info!(
        fetched = fetched.len(),
        removed,
        sent = pending.len(),
        "calendar synced"
    );
    Ok(!fetched.is_empty() || removed > 0 || !pending.is_empty())
}

/// One change to Graph: the event or series, then the dates of the series changed on their own.
/// Afterwards the event is fetched again, so the cache holds the server's version (with its own
/// UID for a new event).
async fn send_graph_change(
    db: &Database,
    graph: &Graph,
    calendar_remote_id: &str,
    own: Option<&str>,
    change: &Pending,
) -> Result<()> {
    let conn = || db.conn();
    let zone = super::local_time_zone_name();
    let written = match (change.kind.as_str(), change.remote_id.as_deref()) {
        ("delete", Some(id)) => graph.delete(id, change.etag.as_deref()).await?,
        ("delete", None) => graph::Written::Done(String::new()),
        // Only dates of the series changed: the series stays as it is there.
        ("update", Some(id)) if !change.send_series => graph::Written::Done(id.to_owned()),
        (kind, remote_id) => {
            let part = graph::Part::Series {
                rule: kind == "create" || change.send_rule,
            };
            let Some(body) = graph::to_graph(&change.ics, zone.as_deref(), own, part) else {
                conn().execute(
                    "UPDATE events SET pending = NULL WHERE id = ?1",
                    [&change.id],
                )?;
                return Ok(());
            };
            match (kind, remote_id) {
                ("update", Some(id)) => graph.update(id, change.etag.as_deref(), &body).await?,
                _ => graph.create(calendar_remote_id, &body).await?,
            }
        }
    };
    match (change.kind.as_str(), written) {
        ("delete", graph::Written::Done(_)) => {
            conn().execute("DELETE FROM events WHERE id = ?1", [&change.id])?;
        }
        (_, graph::Written::Done(id)) => {
            let dates = pending_dates(&conn(), &change.id)?;
            for (instance, kind) in dates {
                send_graph_date(graph, &id, &change.ics, zone.as_deref(), instance, &kind).await?;
            }
            conn().execute(
                "UPDATE events SET pending = NULL, remote_id = ?2, etag = NULL WHERE id = ?1",
                params![change.id, id],
            )?;
        }
        // Changed there since: the server's version comes with the sync.
        (_, graph::Written::Conflict) => {
            conn().execute(
                "UPDATE events SET pending = NULL, etag = NULL WHERE id = ?1",
                [&change.id],
            )?;
        }
    }
    conn().execute(
        "DELETE FROM pending_instances WHERE event_id = ?1",
        [&change.id],
    )?;
    Ok(())
}

/// One date of a series to Graph: changed as the series' text has it, or cancelled. A date
/// the series there no longer has is left alone.
async fn send_graph_date(
    graph: &Graph,
    series_id: &str,
    ics: &str,
    zone: Option<&str>,
    instance: i64,
    kind: &str,
) -> Result<()> {
    let Some(id) = graph.instance_id(series_id, instance).await? else {
        return Ok(());
    };
    if kind == "delete" {
        graph.delete(&id, None).await?;
    } else if let Some(body) = graph::to_graph(ics, zone, None, graph::Part::Date(instance)) {
        graph.update(&id, None, &body).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::{Login, NewAccount, save};
    use crate::calendar::caldav_test_server::{self, CaldavServer, PASSWORD};
    use crate::calendar::events::{self, EventInput};
    use crate::mail::imap::ConnectionReport;
    use crate::mail::settings::{ImapSettings, Security};
    use crate::model::Address;
    use crate::secrets::{MemoryStore, Secret};
    use serde_json::json;

    const MEETING: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:meeting-1\r\n\
DTSTAMP:20261101T080000Z\r\nDTSTART:20261103T090000Z\r\nDTEND:20261103T100000Z\r\n\
SUMMARY:Planung\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    /// 3 November 2026, 09:00 UTC.
    const NOV_3_NINE: i64 = 1_793_696_400_000;
    const HOUR: i64 = 3_600_000;
    const DAY: i64 = 24 * HOUR;

    struct Setup {
        db: Database,
        secrets: MemoryStore,
        account_id: String,
        server: CaldavServer,
    }

    /// An account whose IMAP user and password the CalDAV server knows, with a calendar `work`
    /// that holds one event, and a read-only `holidays`.
    async fn setup() -> Setup {
        let server = caldav_test_server::start().await;
        {
            let mut store = server.store();
            store.add_calendar("work", "Arbeit", false);
            store.add_calendar("holidays", "Feiertage", true);
            store.put("work", "meeting-1.ics", MEETING);
        }
        let db = Database::open_in_memory().unwrap();
        let secrets = MemoryStore::default();
        let account = save(
            &db,
            &secrets,
            &NewAccount {
                display_name: "Arbeit".into(),
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
        Setup {
            db,
            secrets,
            account_id: account.id,
            server,
        }
    }

    impl Setup {
        async fn connect(&self) -> CaldavStatus {
            set_caldav(
                &self.db,
                &self.secrets,
                &self.account_id,
                true,
                Some(&self.server.url("/.well-known/caldav")),
            )
            .await
            .unwrap()
        }

        async fn sync(&self) -> CalendarSyncReport {
            sync_account(&self.db, &self.secrets, &self.account_id)
                .await
                .unwrap()
        }

        fn calendar(&self, name: &str) -> events::Calendar {
            events::list_calendars(&self.db.conn())
                .unwrap()
                .into_iter()
                .find(|calendar| calendar.name == name)
                .unwrap()
        }

        fn titles(&self) -> Vec<String> {
            events::occurrences(&self.db.conn(), 0, i64::MAX / 2)
                .unwrap()
                .into_iter()
                .filter_map(|occurrence| occurrence.title)
                .collect()
        }
    }

    #[test]
    fn looks_on_the_mail_server_and_the_domain() {
        let addresses: Vec<String> =
            candidates("mail.example.org", "alex@example.org", Kind::Events)
                .iter()
                .map(Url::to_string)
                .collect();
        assert_eq!(
            addresses,
            [
                "https://mail.example.org/.well-known/caldav",
                "https://example.org/.well-known/caldav"
            ]
        );
        assert!(candidates("imap.gmail.com", "alex@gmail.com", Kind::Events).is_empty());
        let posteo = candidates("posteo.de", "alex@posteo.de", Kind::Events);
        assert_eq!(posteo[0].as_str(), "https://posteo.de:8443/");
        assert_eq!(posteo.len(), 2);
    }

    #[tokio::test]
    async fn finds_the_calendars_and_fetches_their_events() {
        let setup = setup().await;
        let status = setup.connect().await;
        assert!(status.enabled);
        assert_eq!(
            status.url.as_deref(),
            Some(setup.server.url("/dav/calendars/alex/").as_str())
        );
        assert!(setup.sync().await.changed);

        let names: Vec<String> = events::list_calendars(&setup.db.conn())
            .unwrap()
            .into_iter()
            .filter(|calendar| calendar.account_id.as_deref() == Some(&setup.account_id))
            .map(|calendar| calendar.name)
            .collect();
        // The task list is left out.
        assert_eq!(names, ["Feiertage", "Arbeit"]);
        assert!(setup.calendar("Feiertage").read_only);
        // In the account's colour, not the server's.
        let account_color: String = setup
            .db
            .conn()
            .query_row(
                "SELECT color FROM accounts WHERE id = ?1",
                [&setup.account_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(setup.calendar("Arbeit").color, account_color);
        assert_eq!(setup.titles(), ["Planung"]);

        // Nothing changed there: the second sync only asks for the calendars.
        let before = setup.server.store().requests.len();
        assert!(!setup.sync().await.changed);
        let asked: Vec<String> = setup.server.store().requests[before..].to_vec();
        assert_eq!(asked, ["PROPFIND /dav/calendars/alex/"]);
    }

    #[tokio::test]
    async fn changes_made_here_reach_the_server() {
        let setup = setup().await;
        setup.connect().await;
        setup.sync().await;
        let work = setup.calendar("Arbeit");

        let id = events::save_event_in(
            &setup.db.conn(),
            &EventInput {
                id: None,
                calendar_id: work.id.clone(),
                title: "Neu".into(),
                location: String::new(),
                description: String::new(),
                start: NOV_3_NINE + 3 * HOUR,
                end: NOV_3_NINE + 4 * HOUR,
                all_day: false,
                repeat: None,
                attendees: Vec::new(),
                reminder: None,
                instance: None,
            },
            Some("Europe/Berlin"),
        )
        .unwrap();
        setup.sync().await;
        let uid: String = setup
            .db
            .conn()
            .query_row("SELECT uid FROM events WHERE id = ?1", [&id], |row| {
                row.get(0)
            })
            .unwrap();
        {
            let store = setup.server.store();
            let (_, ics) = &store.calendars["work"].events[&format!("{uid}.ics")];
            assert!(ics.contains("SUMMARY:Neu"), "{ics}");
        }
        let pending: Option<String> = setup
            .db
            .conn()
            .query_row("SELECT pending FROM events WHERE id = ?1", [&id], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(pending, None);

        // Changed and deleted here.
        let planning: String = setup
            .db
            .conn()
            .query_row("SELECT id FROM events WHERE uid = 'meeting-1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        let mut changed = events::get_event(&setup.db.conn(), &planning).unwrap();
        changed.title = Some("Planung (verschoben)".into());
        events::save_event_in(
            &setup.db.conn(),
            &EventInput {
                id: Some(planning.clone()),
                calendar_id: work.id.clone(),
                title: changed.title.clone().unwrap(),
                location: String::new(),
                description: String::new(),
                start: changed.start,
                end: changed.end,
                all_day: false,
                repeat: None,
                attendees: Vec::new(),
                reminder: None,
                instance: None,
            },
            Some("Europe/Berlin"),
        )
        .unwrap();
        events::delete_event(&setup.db.conn(), &id).unwrap();
        setup.sync().await;
        let store = setup.server.store();
        let events = &store.calendars["work"].events;
        assert_eq!(events.len(), 1);
        assert!(events["meeting-1.ics"].1.contains("Planung (verschoben)"));
    }

    /// A weekly series from Tuesday, 3 November, 10:00 in Berlin.
    fn weekly(calendar_id: &str, title: &str) -> EventInput {
        EventInput {
            id: None,
            calendar_id: calendar_id.into(),
            title: title.into(),
            location: String::new(),
            description: String::new(),
            start: NOV_3_NINE,
            end: NOV_3_NINE + HOUR,
            all_day: false,
            repeat: Some(crate::calendar::Frequency::Weekly),
            attendees: Vec::new(),
            reminder: None,
            instance: None,
        }
    }

    fn waiting_dates(setup: &Setup) -> i64 {
        setup
            .db
            .conn()
            .query_row("SELECT count(*) FROM pending_instances", [], |row| {
                row.get(0)
            })
            .unwrap()
    }

    #[tokio::test]
    async fn single_dates_of_a_series_go_with_its_text() {
        let setup = setup().await;
        setup.connect().await;
        setup.sync().await;
        let work = setup.calendar("Arbeit");
        let series = weekly(&work.id, "Sprint");
        let id = events::save_event_in(&setup.db.conn(), &series, Some("Europe/Berlin")).unwrap();
        setup.sync().await;

        let week = 7 * DAY;
        let moved = EventInput {
            id: Some(id.clone()),
            title: "Sprint (Review)".into(),
            start: NOV_3_NINE + week + HOUR,
            end: NOV_3_NINE + week + 2 * HOUR,
            instance: Some(NOV_3_NINE + week),
            ..series.clone()
        };
        events::save_event_in(&setup.db.conn(), &moved, Some("Europe/Berlin")).unwrap();
        events::delete_event_at(&setup.db.conn(), &id, Some(NOV_3_NINE + 2 * week)).unwrap();
        assert_eq!(waiting_dates(&setup), 2);
        setup.sync().await;

        let uid: String = setup
            .db
            .conn()
            .query_row("SELECT uid FROM events WHERE id = ?1", [&id], |row| {
                row.get(0)
            })
            .unwrap();
        {
            let store = setup.server.store();
            let (_, ics) = &store.calendars["work"].events[&format!("{uid}.ics")];
            assert!(
                ics.contains("RECURRENCE-ID;TZID=Europe/Berlin:20261110T100000"),
                "{ics}"
            );
            assert!(ics.contains("SUMMARY:Sprint (Review)"), "{ics}");
            assert!(
                ics.contains("EXDATE;TZID=Europe/Berlin:20261117T100000"),
                "{ics}"
            );
        }
        assert_eq!(waiting_dates(&setup), 0);
        let dates =
            events::occurrences(&setup.db.conn(), NOV_3_NINE, NOV_3_NINE + 3 * week).unwrap();
        let starts: Vec<i64> = dates
            .iter()
            .filter(|date| date.event_id == id)
            .map(|date| date.start)
            .collect();
        assert_eq!(starts, [NOV_3_NINE, NOV_3_NINE + week + HOUR]);
    }

    #[tokio::test]
    async fn changes_on_the_server_arrive_and_win_conflicts() {
        let setup = setup().await;
        setup.connect().await;
        setup.sync().await;
        let work = setup.calendar("Arbeit");
        let planning: String = setup
            .db
            .conn()
            .query_row("SELECT id FROM events WHERE uid = 'meeting-1'", [], |row| {
                row.get(0)
            })
            .unwrap();

        // Another device changes the event while it is changed here too, and adds one.
        setup.server.store().put(
            "work",
            "meeting-1.ics",
            &MEETING.replace("SUMMARY:Planung", "SUMMARY:Planung (Raum 2)"),
        );
        setup.server.store().put(
            "work",
            "lunch.ics",
            &MEETING
                .replace("UID:meeting-1", "UID:lunch")
                .replace("SUMMARY:Planung", "SUMMARY:Mittagessen"),
        );
        events::save_event_in(
            &setup.db.conn(),
            &EventInput {
                id: Some(planning),
                calendar_id: work.id.clone(),
                title: "Planung hier".into(),
                location: String::new(),
                description: String::new(),
                start: NOV_3_NINE,
                end: NOV_3_NINE + HOUR,
                all_day: false,
                repeat: None,
                attendees: Vec::new(),
                reminder: None,
                instance: None,
            },
            None,
        )
        .unwrap();
        assert!(setup.sync().await.changed);
        let mut titles = setup.titles();
        titles.sort();
        assert_eq!(titles, ["Mittagessen", "Planung (Raum 2)"]);

        // Removed on the server: removed here.
        setup.server.store().remove("work", "lunch.ics");
        setup.sync().await;
        assert_eq!(setup.titles(), ["Planung (Raum 2)"]);
    }

    #[tokio::test]
    async fn attendees_go_to_servers_that_invite_and_keep_their_answers() {
        let setup = setup().await;
        setup.connect().await;
        setup.sync().await;
        let work = setup.calendar("Arbeit");
        assert!(work.can_invite);
        assert!(!setup.calendar("Feiertage").can_invite);

        let lena = Address {
            name: Some("Lena Fischer".into()),
            address: "lena@example.org".into(),
        };
        let mut input = EventInput {
            id: None,
            calendar_id: work.id.clone(),
            title: "Planung".into(),
            location: String::new(),
            description: String::new(),
            start: NOV_3_NINE + 5 * HOUR,
            end: NOV_3_NINE + 6 * HOUR,
            all_day: false,
            repeat: None,
            attendees: vec![lena.clone()],
            reminder: None,
            instance: None,
        };
        let id = events::save_event_in(&setup.db.conn(), &input, Some("Europe/Berlin")).unwrap();
        setup.sync().await;
        let (file, ics) = {
            let store = setup.server.store();
            let (file, (_, ics)) = store.calendars["work"]
                .events
                .iter()
                .find(|(_, (_, ics))| {
                    ics.contains("SUMMARY:Planung\r\n") || ics.contains("SUMMARY:Planung\n")
                })
                .map(|(file, event)| (file.clone(), event.clone()))
                .unwrap();
            (file, ics)
        };
        assert!(ics.contains("ORGANIZER"), "{ics}");
        assert!(ics.contains("mailto:alex@example.org"), "{ics}");
        assert!(ics.contains("mailto:lena@example.org"), "{ics}");
        assert!(ics.contains("PARTSTAT=NEEDS-ACTION"), "{ics}");

        // Lena accepts; her server tells the organizer's, which changes the event.
        setup.server.store().put(
            "work",
            &file,
            &ics.replace("PARTSTAT=NEEDS-ACTION", "PARTSTAT=ACCEPTED"),
        );
        setup.sync().await;
        let details = events::get_event(&setup.db.conn(), &id).unwrap();
        assert!(details.own_event);
        assert_eq!(
            details.attendees[0].status,
            crate::calendar::Participation::Accepted
        );

        // Jonas joins: Lena keeps her answer.
        input.id = Some(id.clone());
        input.attendees.push(Address {
            name: None,
            address: "jonas@example.org".into(),
        });
        events::save_event_in(&setup.db.conn(), &input, Some("Europe/Berlin")).unwrap();
        setup.sync().await;
        let ics = setup.server.store().calendars["work"].events[&file]
            .1
            .clone();
        let attendees: Vec<&str> = ics
            .lines()
            .filter(|line| line.starts_with("ATTENDEE"))
            .collect();
        assert_eq!(attendees.len(), 2, "{ics}");
        assert!(ics.contains("PARTSTAT=ACCEPTED"), "{ics}");
    }

    mod microsoft {
        use super::*;
        use crate::calendar::graph::tests::series_json;
        use crate::calendar::graph_test_server::{self, GraphServer, TOKEN};

        async fn graph_setup() -> (Setup, GraphServer, Graph) {
            let setup = setup().await;
            let server = graph_test_server::start().await;
            {
                let mut store = server.store();
                store
                    .calendars
                    .push(("cal-1".into(), "Kalender".into(), true));
                store
                    .calendars
                    .push(("cal-2".into(), "Feiertage".into(), false));
                store.put("cal-1", series_json());
                store.put(
                    "cal-1",
                    json!({
                        "id": "single",
                        "subject": "Zahnarzt",
                        "start": { "dateTime": "2026-11-04T07:00:00.0000000", "timeZone": "UTC" },
                        "end": { "dateTime": "2026-11-04T07:45:00.0000000", "timeZone": "UTC" },
                        "originalStartTimeZone": "Europe/Berlin",
                        "type": "singleInstance"
                    }),
                );
            }
            let graph = Graph::new(&server.base(), Secret::new(TOKEN)).unwrap();
            (setup, server, graph)
        }

        async fn sync(setup: &Setup, graph: &Graph) -> CalendarSyncReport {
            sync_graph(&setup.db, graph, &setup.account_id, "oklch(0.7 0.1 200)")
                .await
                .unwrap()
        }

        fn event_id(setup: &Setup, title: &str) -> String {
            events::occurrences(&setup.db.conn(), 0, i64::MAX / 2)
                .unwrap()
                .into_iter()
                .find(|occurrence| occurrence.title.as_deref() == Some(title))
                .map(|occurrence| occurrence.event_id)
                .unwrap()
        }

        #[tokio::test]
        async fn takes_the_calendars_and_their_series() {
            let (setup, _server, graph) = graph_setup().await;
            assert!(sync(&setup, &graph).await.changed);
            assert!(setup.calendar("Feiertage").read_only);
            let mut titles = setup.titles();
            titles.sort();
            titles.dedup();
            assert_eq!(titles, ["Jour fixe", "Jour fixe (später)", "Zahnarzt"]);
            let series = event_id(&setup, "Jour fixe");
            let details = events::get_event(&setup.db.conn(), &series).unwrap();
            assert_eq!(
                details.recurrence.unwrap().frequency,
                crate::calendar::Frequency::Weekly
            );
            assert_eq!(details.location.as_deref(), Some("Raum 2.14"));
            // Nothing changed: nothing fetched.
            assert!(!sync(&setup, &graph).await.changed);
        }

        #[tokio::test]
        async fn changes_made_here_reach_graph() {
            let (setup, server, graph) = graph_setup().await;
            sync(&setup, &graph).await;
            let calendar = setup.calendar("Kalender");
            events::save_event_in(
                &setup.db.conn(),
                &EventInput {
                    id: None,
                    calendar_id: calendar.id.clone(),
                    title: "Mittagessen".into(),
                    location: "Kantine".into(),
                    description: String::new(),
                    start: NOV_3_NINE + 3 * HOUR,
                    end: NOV_3_NINE + 4 * HOUR,
                    all_day: false,
                    repeat: None,
                    attendees: Vec::new(),
                    reminder: None,
                    instance: None,
                },
                Some("Europe/Berlin"),
            )
            .unwrap();
            let dentist = event_id(&setup, "Zahnarzt");
            let mut changed = events::get_event(&setup.db.conn(), &dentist).unwrap();
            changed.title = Some("Zahnarzt (Kontrolle)".into());
            events::save_event_in(
                &setup.db.conn(),
                &EventInput {
                    id: Some(dentist),
                    calendar_id: calendar.id.clone(),
                    title: changed.title.clone().unwrap(),
                    location: String::new(),
                    description: String::new(),
                    start: changed.start,
                    end: changed.end,
                    all_day: false,
                    repeat: None,
                    attendees: Vec::new(),
                    reminder: None,
                    instance: None,
                },
                Some("Europe/Berlin"),
            )
            .unwrap();
            let series = event_id(&setup, "Jour fixe");
            events::delete_event(&setup.db.conn(), &series).unwrap();
            sync(&setup, &graph).await;

            {
                let store = server.store();
                let subjects: Vec<&str> = store
                    .events
                    .values()
                    .filter_map(|(_, event)| event["subject"].as_str())
                    .collect();
                assert!(subjects.contains(&"Mittagessen"), "{subjects:?}");
                assert!(subjects.contains(&"Zahnarzt (Kontrolle)"), "{subjects:?}");
                assert!(!subjects.contains(&"Jour fixe"), "{subjects:?}");
                let lunch = store
                    .events
                    .values()
                    .find(|(_, event)| event["subject"] == "Mittagessen")
                    .unwrap();
                // Sent in the system's zone, whichever it is (UTC on the CI machines).
                let zone = crate::calendar::local_time_zone_name()
                    .and_then(|name| name.parse::<chrono_tz::Tz>().ok())
                    .unwrap_or(chrono_tz::UTC);
                assert_eq!(lunch.1["originalStartTimeZone"], zone.name());
                // The test server keeps the start in UTC, as Graph hands it out.
                assert_eq!(lunch.1["start"]["dateTime"], "2026-11-03T12:00:00.0000000");
            }
            // The server's versions came back: the new event once, under its id there.
            let mut titles = setup.titles();
            titles.sort();
            assert_eq!(titles, ["Mittagessen", "Zahnarzt (Kontrolle)"]);
            let pending: i64 = setup
                .db
                .conn()
                .query_row(
                    "SELECT count(*) FROM events WHERE pending IS NOT NULL",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(pending, 0);

            // Deleted elsewhere: gone here.
            let id = server
                .store()
                .events
                .iter()
                .find(|(_, (_, event))| event["subject"] == "Mittagessen")
                .map(|(id, _)| id.clone())
                .unwrap();
            server.store().events.remove(&id);
            sync(&setup, &graph).await;
            assert_eq!(setup.titles(), ["Zahnarzt (Kontrolle)"]);
        }

        #[tokio::test]
        async fn single_dates_change_without_touching_the_series() {
            let (setup, server, graph) = graph_setup().await;
            sync(&setup, &graph).await;
            let calendar = setup.calendar("Kalender");
            let series = event_id(&setup, "Jour fixe");
            let week = 7 * DAY;
            // Tuesday, 27 October, 10:00 in Berlin; the 17 November.
            let october_27 = NOV_3_NINE - week;
            let base = EventInput {
                id: Some(series.clone()),
                calendar_id: calendar.id.clone(),
                title: "Jour fixe (Raum 3)".into(),
                location: String::new(),
                description: String::new(),
                start: october_27 + HOUR,
                end: october_27 + HOUR + HOUR / 2,
                all_day: false,
                repeat: Some(crate::calendar::Frequency::Weekly),
                attendees: Vec::new(),
                reminder: None,
                instance: Some(october_27),
            };
            events::save_event_in(&setup.db.conn(), &base, Some("Europe/Berlin")).unwrap();
            events::delete_event_at(&setup.db.conn(), &series, Some(NOV_3_NINE + 2 * week))
                .unwrap();
            assert_eq!(waiting_dates(&setup), 2);
            sync(&setup, &graph).await;

            {
                let store = server.store();
                assert!(
                    !store
                        .writes
                        .iter()
                        .any(|(_, path, _)| path == "me/events/AAMk-master"),
                    "{:?}",
                    store.writes
                );
                let master = &store.events["AAMk-master"].1;
                let exceptions = master["exceptionOccurrences"].as_array().unwrap();
                // The one changed before stays.
                assert_eq!(exceptions.len(), 2, "{exceptions:?}");
                let ours = exceptions
                    .iter()
                    .find(|exception| exception["subject"] == "Jour fixe (Raum 3)")
                    .unwrap();
                assert_eq!(ours["originalStart"], "2026-10-27T09:00:00Z");
                assert_eq!(ours["start"]["dateTime"], "2026-10-27T10:00:00.0000000");
                assert_eq!(
                    master["cancelledOccurrences"],
                    json!(["OID.AAMk-master.2026-11-10", "OID.AAMk-master.2026-11-17"])
                );
            }
            assert_eq!(waiting_dates(&setup), 0);
            let dates =
                events::occurrences(&setup.db.conn(), october_27, NOV_3_NINE + 3 * week).unwrap();
            let titles: Vec<_> = dates
                .iter()
                .filter(|date| date.event_id == series)
                .map(|date| (date.title.clone().unwrap_or_default(), date.start))
                .collect();
            assert_eq!(
                titles,
                [
                    ("Jour fixe (Raum 3)".to_owned(), october_27 + HOUR),
                    ("Jour fixe (später)".to_owned(), NOV_3_NINE + HOUR),
                ]
            );

            // A new title for the series: sent without its rule, so Exchange keeps the dates.
            let details = events::get_event(&setup.db.conn(), &series).unwrap();
            let renamed = EventInput {
                title: "Teamrunde".into(),
                start: details.start,
                end: details.end,
                instance: None,
                ..base
            };
            events::save_event_in(&setup.db.conn(), &renamed, Some("Europe/Berlin")).unwrap();
            sync(&setup, &graph).await;
            let store = server.store();
            let (_, _, body) = store
                .writes
                .iter()
                .find(|(method, path, _)| method == "PATCH" && path == "me/events/AAMk-master")
                .unwrap();
            assert_eq!(body["subject"], "Teamrunde");
            assert!(body.get("recurrence").is_none(), "{body}");
        }
    }

    #[tokio::test]
    async fn a_wrong_password_and_switching_off_are_told_apart() {
        let setup = setup().await;
        setup.connect().await;
        setup.sync().await;
        let off = set_caldav(&setup.db, &setup.secrets, &setup.account_id, false, None)
            .await
            .unwrap();
        assert!(!off.enabled);
        assert!(
            events::list_calendars(&setup.db.conn())
                .unwrap()
                .iter()
                .all(|calendar| calendar.account_id.is_none())
        );
        assert!(!setup.sync().await.changed);

        setup
            .secrets
            .set(
                &setup.account_id,
                SecretKind::ImapPassword,
                &Secret::new("falsch"),
            )
            .unwrap();
        let refused = set_caldav(
            &setup.db,
            &setup.secrets,
            &setup.account_id,
            true,
            Some(&setup.server.url("/.well-known/caldav")),
        )
        .await;
        assert!(matches!(refused, Err(Error::AuthenticationFailed { .. })));
    }
}
