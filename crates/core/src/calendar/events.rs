//! Calendars and their events in the cache. The views read only from here; the sources (CalDAV,
//! Microsoft Graph) fill it. An event keeps its iCalendar text; a series is expanded when a range
//! is asked for.
//!
//! Changes take effect here at once. Events of a calendar on a server are marked `pending` until
//! the sync has sent them; the calendar on this computer (`local`) has no server.

use calcard::icalendar::{ICalendar, ICalendarComponentType, ICalendarProperty};
use chrono::{DateTime, TimeZone, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{Attendee, Frequency, PRODUCT_ID, Recurrence, ical};
use crate::ids::new_id;
use crate::model::Address;
use crate::{Error, Result};

/// The calendar on this computer, made by the migration.
pub const LOCAL_CALENDAR: &str = "local";
const DAY_MS: i64 = 86_400_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Calendar {
    pub id: String,
    /// `None` for the calendar on this computer.
    pub account_id: Option<String>,
    /// Empty for the calendar on this computer: the interface names it.
    pub name: String,
    /// The colour chosen for it, else the account's; the calendar's own for the one on this
    /// computer.
    pub color: String,
    /// A colour was chosen for it.
    pub own_color: bool,
    pub read_only: bool,
    /// Its events show in the views.
    pub visible: bool,
    /// Its server sends invitations to an event's attendees: events here can have some.
    pub can_invite: bool,
}

/// One date of an event in a view: a single event, or one date of a series.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Occurrence {
    pub event_id: String,
    pub calendar_id: String,
    pub title: Option<String>,
    pub location: Option<String>,
    /// Unix milliseconds. All-day events start at midnight UTC of their first day and end at
    /// midnight UTC of the day after their last.
    #[ts(type = "number")]
    pub start: i64,
    #[ts(type = "number")]
    pub end: i64,
    pub all_day: bool,
    /// A date of a series.
    pub recurring: bool,
    /// For a date of a series: its original start, which names it when only this date is
    /// opened, changed or deleted.
    #[ts(type = "number | null")]
    pub instance: Option<i64>,
}

/// An event as its dialog shows it; for a series its first date.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EventDetails {
    pub id: String,
    pub calendar_id: String,
    pub title: Option<String>,
    pub location: Option<String>,
    pub description: Option<String>,
    #[ts(type = "number")]
    pub start: i64,
    #[ts(type = "number")]
    pub end: i64,
    pub all_day: bool,
    pub recurrence: Option<Recurrence>,
    pub organizer: Option<Address>,
    pub attendees: Vec<Attendee>,
    /// In a calendar that can only be read.
    pub read_only: bool,
    /// The account organizes it (or nobody does): its attendees can be changed here.
    pub own_event: bool,
    /// Minutes before the start of its (first) reminder; `None` without one.
    #[ts(type = "number | null")]
    pub reminder: Option<i64>,
    /// Opened as one date of a series: that date's original start. The details are this date's.
    #[ts(type = "number | null")]
    pub instance: Option<i64>,
}

/// A reminder that is due: the app shows it as a notification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Reminder {
    pub event_id: String,
    pub title: Option<String>,
    pub location: Option<String>,
    /// The date it reminds of, as in `Occurrence`.
    #[ts(type = "number")]
    pub start: i64,
    pub all_day: bool,
    /// When it is due, Unix milliseconds.
    #[ts(type = "number")]
    pub at: i64,
}

/// Reminders this long before their event are looked for; earlier ones are rare.
const REMINDER_LEAD_MS: i64 = 8 * DAY_MS;

/// A new event or the changes to one, from the event dialog.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EventInput {
    /// `None` for a new event.
    pub id: Option<String>,
    pub calendar_id: String,
    pub title: String,
    pub location: String,
    pub description: String,
    /// Unix milliseconds; for all-day events midnight UTC of the first day and of the day after
    /// the last.
    #[ts(type = "number")]
    pub start: i64,
    #[ts(type = "number")]
    pub end: i64,
    pub all_day: bool,
    /// Every day, week, month or year from the start on; `None` for a single date. A series
    /// whose rule says more (certain weekdays, an end) keeps its rule while this stays the same.
    pub repeat: Option<Frequency>,
    /// Who is invited; the calendar's server sends the invitations. Only for calendars that
    /// can invite, and only for events the account organizes; ignored elsewhere.
    #[serde(default)]
    pub attendees: Vec<Address>,
    /// Minutes before the start of a reminder; `None` for none. An event whose reminder stays
    /// the same keeps its alarms as they are.
    #[serde(default)]
    #[ts(type = "number | null")]
    pub reminder: Option<i64>,
    /// Only this date of a series (its original start): it changes on its own, the series
    /// stays. Repetition and calendar are the series' then.
    #[serde(default)]
    #[ts(type = "number | null")]
    pub instance: Option<i64>,
}

/// Who organizes and who is invited, for an event in a calendar that can invite.
struct People<'a> {
    organizer: Address,
    attendees: &'a [Address],
}

/// The calendar's account as organizer, where its server sends invitations.
fn organizer_for(conn: &Connection, calendar_id: &str) -> Result<Option<Address>> {
    let row: Option<(Option<bool>, Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT c.can_invite, a.email_address, a.sender_name
             FROM calendars c LEFT JOIN accounts a ON a.id = c.account_id WHERE c.id = ?1",
            [calendar_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    Ok(match row {
        Some((Some(true), Some(address), name)) => Some(Address {
            name: name
                .map(|name| name.trim().to_owned())
                .filter(|name| !name.is_empty()),
            address,
        }),
        _ => None,
    })
}

/// Whether the account (by address) organizes the event, or nobody does.
fn organized_by(event: &calcard::icalendar::ICalendarComponent, address: Option<&str>) -> bool {
    match event
        .property(&ICalendarProperty::Organizer)
        .and_then(ical::person)
    {
        None => true,
        Some(organizer) => address.is_some_and(|own| organizer.address.eq_ignore_ascii_case(own)),
    }
}

pub fn list_calendars(conn: &Connection) -> Result<Vec<Calendar>> {
    let mut statement = conn.prepare(
        // An account's calendars show in its colour unless one was chosen; the one the server
        // gave stays kept.
        "SELECT c.id, c.account_id, c.name, coalesce(c.own_color, a.color, c.color), c.read_only,
           c.visible, c.can_invite, c.own_color IS NOT NULL
         FROM calendars c LEFT JOIN accounts a ON a.id = c.account_id
         ORDER BY c.account_id IS NULL, a.sort_order, a.created_at, c.position, c.name",
    )?;
    let calendars = statement
        .query_map([], |row| {
            Ok(Calendar {
                id: row.get(0)?,
                account_id: row.get(1)?,
                name: row.get(2)?,
                color: row.get(3)?,
                read_only: row.get(4)?,
                visible: row.get(5)?,
                can_invite: row.get::<_, Option<bool>>(6)?.unwrap_or(false),
                own_color: row.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(calendars)
}

/// Gives a calendar a colour of its own; `None` shows it in its account's colour again.
pub fn set_calendar_color(conn: &Connection, calendar_id: &str, color: Option<&str>) -> Result<()> {
    conn.execute(
        "UPDATE calendars SET own_color = ?2 WHERE id = ?1",
        params![calendar_id, color],
    )?;
    Ok(())
}

/// Shows or hides a calendar's events in the views.
pub fn set_calendar_visible(conn: &Connection, calendar_id: &str, visible: bool) -> Result<()> {
    conn.execute(
        "UPDATE calendars SET visible = ?2 WHERE id = ?1",
        params![calendar_id, visible],
    )?;
    Ok(())
}

/// The dates of all events that overlap `from..to` (Unix milliseconds), earliest first, hidden
/// calendars included: the interface leaves those out, so showing them again needs no query.
pub fn occurrences(conn: &Connection, from: i64, to: i64) -> Result<Vec<Occurrence>> {
    let mut found = Vec::new();
    // A single date: its columns say everything. An event without length still counts at its
    // start.
    let mut single = conn.prepare(
        "SELECT id, calendar_id, title, location, starts_at, ends_at, all_day FROM events
         WHERE recurring = 0 AND pending IS NOT 'delete'
           AND starts_at < ?2 AND max(ends_at, starts_at + 1) > ?1",
    )?;
    let rows = single.query_map(params![from, to], |row| {
        Ok(Occurrence {
            event_id: row.get(0)?,
            calendar_id: row.get(1)?,
            title: row.get(2)?,
            location: row.get(3)?,
            start: row.get(4)?,
            end: row.get(5)?,
            all_day: row.get(6)?,
            recurring: false,
            instance: None,
        })
    })?;
    for row in rows {
        found.push(row?);
    }

    let mut series = conn.prepare(
        "SELECT id, calendar_id, ics FROM events
         WHERE recurring = 1 AND pending IS NOT 'delete' AND starts_at < ?1",
    )?;
    let rows = series.query_map([to], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (event_id, calendar_id, ics) = row?;
        let Ok(calendar) = ICalendar::parse(&ics) else {
            continue;
        };
        for dates in ical::dates_between(&calendar, from, to) {
            // A changed date of the series has a title and place of its own.
            let component = &calendar.components[dates.component];
            found.push(Occurrence {
                event_id: event_id.clone(),
                calendar_id: calendar_id.clone(),
                title: ical::text(component, &ICalendarProperty::Summary),
                location: ical::text(component, &ICalendarProperty::Location),
                start: dates.start,
                end: dates.end,
                all_day: dates.all_day,
                recurring: true,
                instance: dates.instance,
            });
        }
    }
    found.sort_by_key(|occurrence| (occurrence.start, occurrence.end));
    Ok(found)
}

/// The event's master: the component without `RECURRENCE-ID`, or the first one.
fn master(calendar: &ICalendar) -> Option<usize> {
    let events = || {
        calendar
            .components
            .iter()
            .enumerate()
            .filter(|(_, component)| component.component_type == ICalendarComponentType::VEvent)
    };
    events()
        .find(|(_, event)| !event.has_property(&ICalendarProperty::RecurrenceId))
        .or_else(|| events().next())
        .map(|(index, _)| index)
}

pub fn get_event(conn: &Connection, event_id: &str) -> Result<EventDetails> {
    get_event_at(conn, event_id, None)
}

/// A changed date can lie this far from where the series would have it.
const INSTANCE_REACH_MS: i64 = 400 * DAY_MS;

/// The event, or with `instance` one date of its series as that date is.
pub fn get_event_at(
    conn: &Connection,
    event_id: &str,
    instance: Option<i64>,
) -> Result<EventDetails> {
    let (calendar_id, ics, read_only, own_address): (String, String, bool, Option<String>) = conn
        .query_row(
            "SELECT e.calendar_id, e.ics, c.read_only, a.email_address
             FROM events e JOIN calendars c ON c.id = e.calendar_id
               LEFT JOIN accounts a ON a.id = c.account_id
             WHERE e.id = ?1 AND e.pending IS NOT 'delete'",
            [event_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?
        .ok_or(Error::EventNotFound)?;
    let calendar = ICalendar::parse(&ics).map_err(|_| Error::EventNotFound)?;
    let series = master(&calendar).ok_or(Error::EventNotFound)?;
    let dates = match instance {
        Some(instance) => ical::dates_between(
            &calendar,
            instance - INSTANCE_REACH_MS,
            instance + INSTANCE_REACH_MS,
        )
        .into_iter()
        .find(|dates| dates.instance == Some(instance))
        .ok_or(Error::EventNotFound)?,
        None => ical::first_dates(&calendar, series).ok_or(Error::EventNotFound)?,
    };
    // A changed date has details of its own; the rule is the series'.
    let index = dates.component;
    let event = &calendar.components[index];
    Ok(EventDetails {
        id: event_id.to_owned(),
        calendar_id,
        title: ical::text(event, &ICalendarProperty::Summary),
        location: ical::text(event, &ICalendarProperty::Location),
        description: ical::text(event, &ICalendarProperty::Description),
        start: dates.start,
        end: dates.end,
        all_day: dates.all_day,
        recurrence: ical::recurrence(&calendar.components[series]),
        organizer: event
            .property(&ICalendarProperty::Organizer)
            .and_then(ical::person),
        attendees: ical::attendees(event),
        read_only,
        own_event: organized_by(event, own_address.as_deref()),
        reminder: ical::reminder_minutes(&calendar, index),
        instance: dates.instance.filter(|_| instance.is_some()),
    })
}

/// The reminders due in `from..=to` (Unix milliseconds), for dates of events and series that
/// have alarms. All-day dates are reminded of from local midnight.
pub fn due_reminders(conn: &Connection, from: i64, to: i64) -> Result<Vec<Reminder>> {
    let mut statement = conn.prepare(
        "SELECT id, ics FROM events
         WHERE pending IS NOT 'delete' AND ics LIKE '%BEGIN:VALARM%'
           AND starts_at < ?2 AND (recurring = 1 OR ends_at > ?1)",
    )?;
    let rows = statement.query_map(
        params![from - REMINDER_LEAD_MS, to + REMINDER_LEAD_MS],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?;
    let zone = super::local_time_zone_name().and_then(|name| name.parse::<chrono_tz::Tz>().ok());
    let mut due = Vec::new();
    for row in rows {
        let (event_id, ics) = row?;
        let Ok(calendar) = ICalendar::parse(&ics) else {
            continue;
        };
        for dates in ical::dates_between(&calendar, from - REMINDER_LEAD_MS, to + REMINDER_LEAD_MS)
        {
            // An all-day date starts at midnight where the user is.
            let local_start = if dates.all_day {
                DateTime::<Utc>::from_timestamp_millis(dates.start)
                    .and_then(|utc| {
                        let midnight = utc.naive_utc();
                        match zone {
                            Some(zone) => zone
                                .from_local_datetime(&midnight)
                                .earliest()
                                .map(|local| local.timestamp_millis()),
                            None => Some(midnight.and_utc().timestamp_millis()),
                        }
                    })
                    .unwrap_or(dates.start)
            } else {
                dates.start
            };
            let local_end = local_start + (dates.end - dates.start);
            for trigger in ical::triggers(&calendar, dates.component) {
                let at = match trigger {
                    ical::Trigger::Relative { seconds, to_end } => {
                        (if to_end { local_end } else { local_start }) + seconds * 1000
                    }
                    ical::Trigger::Absolute(at) => at,
                };
                if at > from && at <= to {
                    let component = &calendar.components[dates.component];
                    due.push(Reminder {
                        event_id: event_id.clone(),
                        title: ical::text(component, &ICalendarProperty::Summary),
                        location: ical::text(component, &ICalendarProperty::Location),
                        start: dates.start,
                        all_day: dates.all_day,
                        at,
                    });
                }
            }
        }
    }
    due.sort_by_key(|reminder| reminder.at);
    Ok(due)
}

/// Saves a new event or the changes to one; returns its id. Times are written in the system's
/// time zone, so a series keeps its local time across clock changes.
pub fn save_event(conn: &Connection, input: &EventInput) -> Result<String> {
    save_event_in(conn, input, super::local_time_zone_name().as_deref())
}

/// The calendar a change goes to: whether it can be written, and whether it is on a server.
fn writable(conn: &Connection, calendar_id: &str) -> Result<bool> {
    let (read_only, remote): (bool, bool) = conn
        .query_row(
            "SELECT read_only, remote_id IS NOT NULL FROM calendars WHERE id = ?1",
            [calendar_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or(Error::EventNotFound)?;
    if read_only {
        return Err(Error::CalendarReadOnly);
    }
    Ok(remote)
}

pub(crate) fn save_event_in(
    conn: &Connection,
    input: &EventInput,
    time_zone: Option<&str>,
) -> Result<String> {
    let mut input = input.clone();
    if input.end < input.start {
        return Err(Error::InvalidEventTime);
    }
    if input.all_day {
        // Whole days, at least one.
        input.start -= input.start.rem_euclid(DAY_MS);
        input.end = (input.end - input.end.rem_euclid(DAY_MS)).max(input.start + DAY_MS);
    }
    let remote = writable(conn, &input.calendar_id)?;
    let now = Utc::now();

    let existing: Option<(String, String, String, Option<String>)> = match &input.id {
        Some(id) => Some(
            conn.query_row(
                "SELECT e.uid, e.ics, e.calendar_id, e.remote_id FROM events e
                 JOIN calendars c ON c.id = e.calendar_id
                 WHERE e.id = ?1 AND e.pending IS NOT 'delete' AND c.read_only = 0",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?
            .ok_or(Error::EventNotFound)?,
        ),
        None => None,
    };
    if let (Some(instance), Some((uid, ics, calendar_id, remote_id))) = (input.instance, &existing)
    {
        let ics = with_instance(ics, uid, &input, instance, time_zone, now)?;
        let id = input.id.clone().unwrap_or_default();
        mark_instance(
            conn,
            &id,
            calendar_id,
            remote_id.is_some(),
            &ics,
            instance,
            "update",
        )?;
        return Ok(id);
    }
    // The rule changed or the series moved: its changed dates are dropped, and Graph needs the
    // rule again.
    let rule_changed = match &existing {
        Some((_, ics, ..)) => {
            let calendar = ICalendar::parse(ics).map_err(|_| Error::EventNotFound)?;
            let series = master(&calendar).ok_or(Error::EventNotFound)?;
            ical::recurrence(&calendar.components[series]).map(|rule| rule.frequency)
                != input.repeat
                || ical::first_occurrence(&calendar, series)
                    != Some((input.start, input.end, input.all_day))
        }
        None => true,
    };
    if rule_changed && let Some(id) = &input.id {
        conn.execute("DELETE FROM pending_instances WHERE event_id = ?1", [id])?;
    }
    let organizer = organizer_for(conn, &input.calendar_id)?;
    let people = organizer.map(|organizer| People {
        organizer,
        attendees: &input.attendees,
    });
    let ics = match &existing {
        Some((uid, ics, ..)) => {
            changed_calendar(ics, uid, &input, people.as_ref(), time_zone, now)?
        }
        None => new_calendar(&new_id(), &input, people.as_ref(), time_zone, now, 0)?,
    };
    let columns = columns_of(&ics).ok_or(Error::InvalidEventTime)?;
    let now = now.timestamp_millis();
    match existing {
        Some((_, _, old_calendar, Some(_))) if old_calendar != input.calendar_id => {
            // Moved out of a calendar on a server: deleted there, and new in the other one.
            conn.execute(
                "UPDATE events SET pending = 'delete', changed_at = ?2 WHERE id = ?1",
                params![input.id, now],
            )?;
            insert_event(
                conn,
                &input.calendar_id,
                &ics,
                &columns,
                remote.then_some("create"),
                now,
            )
        }
        Some((_, _, _, remote_id)) => {
            let id = input.id.clone().unwrap_or_default();
            // Not on the server yet: still new there.
            let pending = remote.then_some(if remote_id.is_some() {
                "update"
            } else {
                "create"
            });
            conn.execute(
                "UPDATE events SET calendar_id = ?2, uid = ?3, ics = ?4, title = ?5, location = ?6,
                   all_day = ?7, starts_at = ?8, ends_at = ?9, recurring = ?10, changed_at = ?11,
                   pending = ?12, send_series = 1,
                   send_rule = CASE WHEN pending IS NULL THEN ?13 ELSE max(send_rule, ?13) END
                 WHERE id = ?1",
                params![
                    id,
                    input.calendar_id,
                    columns.uid,
                    ics,
                    columns.title,
                    columns.location,
                    columns.all_day,
                    columns.start,
                    columns.end,
                    columns.recurring,
                    now,
                    pending,
                    rule_changed,
                ],
            )?;
            Ok(id)
        }
        None => insert_event(
            conn,
            &input.calendar_id,
            &ics,
            &columns,
            remote.then_some("create"),
            now,
        ),
    }
}

/// What the views need of an event, read from its iCalendar text.
pub(crate) struct Columns {
    pub uid: String,
    title: Option<String>,
    location: Option<String>,
    all_day: bool,
    start: i64,
    end: i64,
    recurring: bool,
}

/// `None` for calendar text without an event (a task, say).
pub(crate) fn columns_of(ics: &str) -> Option<Columns> {
    let calendar = ICalendar::parse(ics).ok()?;
    let index = master(&calendar)?;
    let event = &calendar.components[index];
    let dates = ical::first_dates(&calendar, index)?;
    Some(Columns {
        uid: event.uid()?.to_owned(),
        title: ical::text(event, &ICalendarProperty::Summary),
        location: ical::text(event, &ICalendarProperty::Location),
        all_day: dates.all_day,
        start: dates.start,
        end: dates.end,
        recurring: event.has_property(&ICalendarProperty::Rrule),
    })
}

fn insert_event(
    conn: &Connection,
    calendar_id: &str,
    ics: &str,
    columns: &Columns,
    pending: Option<&str>,
    now: i64,
) -> Result<String> {
    let id = new_id();
    conn.execute(
        "INSERT INTO events (id, calendar_id, uid, ics, title, location, all_day, starts_at,
           ends_at, recurring, changed_at, pending)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            id,
            calendar_id,
            columns.uid,
            ics,
            columns.title,
            columns.location,
            columns.all_day,
            columns.start,
            columns.end,
            columns.recurring,
            now,
            pending,
        ],
    )?;
    Ok(id)
}

/// An event the app wrote itself (an answered invitation, say) into a calendar: new, or over the
/// cached one `existing`. One on a server waits for the sync. Returns its id.
pub(crate) fn store_own(
    conn: &Connection,
    calendar_id: &str,
    existing: Option<&str>,
    ics: &str,
) -> Result<String> {
    let columns = columns_of(ics).ok_or(Error::InvalidEventTime)?;
    let remote = writable(conn, calendar_id)?;
    let now = Utc::now().timestamp_millis();
    match existing {
        Some(id) => {
            let on_server: bool = conn.query_row(
                "SELECT remote_id IS NOT NULL FROM events WHERE id = ?1",
                [id],
                |row| row.get(0),
            )?;
            let pending = remote.then_some(if on_server { "update" } else { "create" });
            conn.execute(
                "UPDATE events SET uid = ?2, ics = ?3, title = ?4, location = ?5, all_day = ?6,
                   starts_at = ?7, ends_at = ?8, recurring = ?9, changed_at = ?10, pending = ?11
                 WHERE id = ?1",
                params![
                    id,
                    columns.uid,
                    ics,
                    columns.title,
                    columns.location,
                    columns.all_day,
                    columns.start,
                    columns.end,
                    columns.recurring,
                    now,
                    pending,
                ],
            )?;
            Ok(id.to_owned())
        }
        None => insert_event(
            conn,
            calendar_id,
            ics,
            &columns,
            remote.then_some("create"),
            now,
        ),
    }
}

/// An event as the server has it: takes the place of the cached one with the same address or
/// UID. Calendar text without an event is left out.
pub(crate) fn store_remote(
    conn: &Connection,
    calendar_id: &str,
    remote_id: &str,
    etag: Option<&str>,
    ics: &str,
) -> Result<()> {
    let Some(columns) = columns_of(ics) else {
        return Ok(());
    };
    let now = Utc::now().timestamp_millis();
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM events WHERE calendar_id = ?1 AND (remote_id = ?2 OR uid = ?3)",
            params![calendar_id, remote_id, columns.uid],
            |row| row.get(0),
        )
        .optional()?;
    match existing {
        Some(id) => {
            conn.execute(
                "UPDATE events SET uid = ?2, ics = ?3, title = ?4, location = ?5, all_day = ?6,
                   starts_at = ?7, ends_at = ?8, recurring = ?9, changed_at = ?10,
                   remote_id = ?11, etag = ?12, pending = NULL
                 WHERE id = ?1",
                params![
                    id,
                    columns.uid,
                    ics,
                    columns.title,
                    columns.location,
                    columns.all_day,
                    columns.start,
                    columns.end,
                    columns.recurring,
                    now,
                    remote_id,
                    etag,
                ],
            )?;
        }
        None => {
            let id = insert_event(conn, calendar_id, ics, &columns, None, now)?;
            conn.execute(
                "UPDATE events SET remote_id = ?2, etag = ?3 WHERE id = ?1",
                params![id, remote_id, etag],
            )?;
        }
    }
    Ok(())
}

/// Deletes an event, a series with all its dates. One on a server waits for the sync.
pub fn delete_event(conn: &Connection, event_id: &str) -> Result<()> {
    delete_event_at(conn, event_id, None)
}

/// Deletes an event, or with `instance` only that date of its series.
pub fn delete_event_at(conn: &Connection, event_id: &str, instance: Option<i64>) -> Result<()> {
    let (calendar_id, remote_id, ics): (String, Option<String>, String) = conn
        .query_row(
            "SELECT calendar_id, remote_id, ics FROM events WHERE id = ?1",
            [event_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or(Error::EventNotFound)?;
    writable(conn, &calendar_id)?;
    if let Some(instance) = instance {
        let ics = without_instance(&ics, instance)?;
        return mark_instance(
            conn,
            event_id,
            &calendar_id,
            remote_id.is_some(),
            &ics,
            instance,
            "delete",
        );
    }
    if remote_id.is_some() {
        conn.execute(
            "UPDATE events SET pending = 'delete', changed_at = ?2 WHERE id = ?1",
            params![event_id, Utc::now().timestamp_millis()],
        )?;
    } else {
        conn.execute("DELETE FROM events WHERE id = ?1", [event_id])?;
    }
    Ok(())
}

/// Keeps a change to one date of a series: the series' text with it, and for Graph which date
/// changed, while the series itself waits for nothing.
fn mark_instance(
    conn: &Connection,
    event_id: &str,
    calendar_id: &str,
    on_server: bool,
    ics: &str,
    instance: i64,
    kind: &str,
) -> Result<()> {
    let remote = writable(conn, calendar_id)?;
    let columns = columns_of(ics).ok_or(Error::InvalidEventTime)?;
    let pending = remote.then_some(if on_server { "update" } else { "create" });
    conn.execute(
        "UPDATE events SET ics = ?2, title = ?3, location = ?4, all_day = ?5, starts_at = ?6,
           ends_at = ?7, changed_at = ?8,
           send_series = CASE WHEN pending IS NULL THEN 0 ELSE send_series END,
           send_rule = CASE WHEN pending IS NULL THEN 0 ELSE send_rule END,
           pending = coalesce(pending, ?9)
         WHERE id = ?1",
        params![
            event_id,
            ics,
            columns.title,
            columns.location,
            columns.all_day,
            columns.start,
            columns.end,
            Utc::now().timestamp_millis(),
            pending,
        ],
    )?;
    if remote {
        conn.execute(
            "INSERT INTO pending_instances (event_id, instance, kind) VALUES (?1, ?2, ?3)
             ON CONFLICT (event_id, instance) DO UPDATE SET kind = excluded.kind",
            params![event_id, instance, kind],
        )?;
    }
    Ok(())
}

/// The components that change dates of the series, by the date they change.
fn overrides_of(calendar: &ICalendar, instance: i64) -> Vec<u32> {
    calendar
        .components
        .iter()
        .enumerate()
        .filter(|(_, component)| {
            component.component_type == ICalendarComponentType::VEvent
                && component
                    .property(&ICalendarProperty::RecurrenceId)
                    .and_then(|entry| ical::instant_of(calendar, entry))
                    == Some(instance)
        })
        .filter_map(|(position, _)| u32::try_from(position).ok())
        .collect()
}

/// The series' text with one date as the dialog left it: a component of its own, named by the
/// date's original start (`RECURRENCE-ID`), in place of an earlier change of that date. It keeps
/// the series' organizer and attendees.
fn with_instance(
    ics: &str,
    uid: &str,
    input: &EventInput,
    instance: i64,
    time_zone: Option<&str>,
    now: DateTime<Utc>,
) -> Result<String> {
    let mut calendar = ICalendar::parse(ics).map_err(|_| Error::EventNotFound)?;
    let series = master(&calendar).ok_or(Error::EventNotFound)?;
    let recurrence_id = ical::instance_property(&calendar, series, "RECURRENCE-ID", instance)
        .ok_or(Error::EventNotFound)?;
    let sequence = calendar.components[series]
        .property(&ICalendarProperty::Sequence)
        .and_then(|entry| entry.values.first())
        .and_then(|value| value.as_integer())
        .unwrap_or(0);
    let mut people = String::new();
    for entry in calendar.components[series].entries.iter().filter(|entry| {
        matches!(
            entry.name,
            ICalendarProperty::Organizer | ICalendarProperty::Attendee
        )
    }) {
        entry
            .write_to(&mut people)
            .map_err(|_| Error::InvalidEventTime)?;
    }
    let stamp = now.format("%Y%m%dT%H%M%SZ");
    let mut lines = vec![
        "BEGIN:VEVENT".to_owned(),
        format!("UID:{}", ical::escape(uid)),
        recurrence_id,
        format!("DTSTAMP:{stamp}"),
        format!("SEQUENCE:{}", sequence + 1),
        time_line("DTSTART", input.start, input.all_day, time_zone),
        time_line("DTEND", input.end, input.all_day, time_zone),
    ];
    for (name, value) in [
        ("SUMMARY", &input.title),
        ("LOCATION", &input.location),
        ("DESCRIPTION", &input.description),
    ] {
        if !value.trim().is_empty() {
            lines.push(format!("{name}:{}", ical::escape(value.trim())));
        }
    }
    lines.extend(people.lines().map(str::to_owned));
    if let Some(minutes) = input.reminder {
        lines.extend(ical::alarm_lines(minutes, input.title.trim()));
    }
    lines.push("END:VEVENT".to_owned());

    let replaced = overrides_of(&calendar, instance);
    calendar.components[0]
        .component_ids
        .retain(|id| !replaced.contains(id));
    let text = calendar.to_string();
    let end = text.rfind("END:VCALENDAR").ok_or(Error::InvalidEventTime)?;
    let text = format!("{}{}\r\n{}", &text[..end], lines.join("\r\n"), &text[end..]);
    let mut calendar = ICalendar::parse(&text).map_err(|_| Error::InvalidEventTime)?;
    calendar.add_missing_timezones();
    Ok(calendar.to_string())
}

/// The series' text without one date: left out (`EXDATE`), and an earlier change of it gone.
fn without_instance(ics: &str, instance: i64) -> Result<String> {
    let mut calendar = ICalendar::parse(ics).map_err(|_| Error::EventNotFound)?;
    let series = master(&calendar).ok_or(Error::EventNotFound)?;
    let exdate = ical::instance_property(&calendar, series, "EXDATE", instance)
        .ok_or(Error::EventNotFound)?;
    let replaced = overrides_of(&calendar, instance);
    calendar.components[0]
        .component_ids
        .retain(|id| !replaced.contains(id));
    // The EXDATE goes in as text, parsed with the rest.
    let text = calendar.to_string();
    let master_uid = calendar.components[series]
        .uid()
        .unwrap_or_default()
        .to_owned();
    let marker = format!("UID:{}", ical::escape(&master_uid));
    let at = text.find(&marker).ok_or(Error::EventNotFound)?;
    let line_end = text[at..].find("\r\n").map_or(text.len(), |end| at + end);
    let text = format!("{}\r\n{exdate}{}", &text[..line_end], &text[line_end..]);
    let mut calendar = ICalendar::parse(&text).map_err(|_| Error::InvalidEventTime)?;
    calendar.add_missing_timezones();
    Ok(calendar.to_string())
}

/// `DTSTART`/`DTEND` (or a date for all-day events): in the time zone by name, else in UTC.
fn time_line(name: &str, millis: i64, all_day: bool, time_zone: Option<&str>) -> String {
    let utc = DateTime::<Utc>::from_timestamp_millis(millis).unwrap_or_default();
    if all_day {
        return format!("{name};VALUE=DATE:{}", utc.format("%Y%m%d"));
    }
    match time_zone.and_then(|zone| zone.parse::<chrono_tz::Tz>().ok()) {
        Some(zone) => format!(
            "{name};TZID={}:{}",
            zone.name(),
            zone.from_utc_datetime(&utc.naive_utc())
                .format("%Y%m%dT%H%M%S")
        ),
        None => format!("{name}:{}", utc.format("%Y%m%dT%H%M%SZ")),
    }
}

/// A calendar with one event as the dialog describes it, its time zone included.
fn new_calendar(
    uid: &str,
    input: &EventInput,
    people: Option<&People<'_>>,
    time_zone: Option<&str>,
    now: DateTime<Utc>,
    sequence: i64,
) -> Result<String> {
    let stamp = now.format("%Y%m%dT%H%M%SZ");
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_owned(),
        "VERSION:2.0".to_owned(),
        format!("PRODID:{PRODUCT_ID}"),
        "BEGIN:VEVENT".to_owned(),
        format!("UID:{}", ical::escape(uid)),
        format!("DTSTAMP:{stamp}"),
        format!("LAST-MODIFIED:{stamp}"),
        format!("SEQUENCE:{sequence}"),
        time_line("DTSTART", input.start, input.all_day, time_zone),
        time_line("DTEND", input.end, input.all_day, time_zone),
    ];
    for (name, value) in [
        ("SUMMARY", &input.title),
        ("LOCATION", &input.location),
        ("DESCRIPTION", &input.description),
    ] {
        if !value.trim().is_empty() {
            lines.push(format!("{name}:{}", ical::escape(value.trim())));
        }
    }
    if let Some(rule) = input.repeat.and_then(Frequency::as_rrule) {
        lines.push(format!("RRULE:FREQ={rule}"));
    }
    if let Some(minutes) = input.reminder {
        lines.extend(ical::alarm_lines(minutes, input.title.trim()));
    }
    if let Some(people) = people.filter(|people| !people.attendees.is_empty()) {
        let cn = |name: Option<&str>| {
            name.map(|name| format!(";CN=\"{}\"", name.replace('"', "'")))
                .unwrap_or_default()
        };
        lines.push(format!(
            "ORGANIZER{}:mailto:{}",
            cn(people.organizer.name.as_deref()),
            people.organizer.address
        ));
        for attendee in people.attendees {
            lines.push(format!(
                "ATTENDEE;ROLE=REQ-PARTICIPANT;PARTSTAT=NEEDS-ACTION;RSVP=TRUE{}:mailto:{}",
                cn(attendee.name.as_deref()),
                attendee.address.trim()
            ));
        }
    }
    lines.extend([
        "END:VEVENT".to_owned(),
        "END:VCALENDAR".to_owned(),
        String::new(),
    ]);
    let mut calendar = ICalendar::parse(lines.join("\r\n")).map_err(|_| Error::InvalidEventTime)?;
    // Servers and other apps want the time zone spelled out.
    calendar.add_missing_timezones();
    Ok(calendar.to_string())
}

/// The event's calendar with the dialog's changes: what the dialog does not show (attendees,
/// alarms, other properties) stays. Moving the dates or changing how a series repeats drops the
/// series' changed and left-out dates, which would no longer fit.
fn changed_calendar(
    ics: &str,
    uid: &str,
    input: &EventInput,
    people: Option<&People<'_>>,
    time_zone: Option<&str>,
    now: DateTime<Utc>,
) -> Result<String> {
    let mut calendar = ICalendar::parse(ics).map_err(|_| Error::EventNotFound)?;
    let index = master(&calendar).ok_or(Error::EventNotFound)?;
    let event = &calendar.components[index];
    let sequence = event
        .property(&ICalendarProperty::Sequence)
        .and_then(|entry| entry.values.first())
        .and_then(|value| value.as_integer())
        .unwrap_or(0);
    let old_dates = ical::first_dates(&calendar, index);
    let old_reminder = ical::reminder_minutes(&calendar, index);
    let old_rule = ical::recurrence(event);
    let keep_rule = old_rule.as_ref().map(|rule| rule.frequency) == input.repeat;
    let moved = old_dates.is_none_or(|dates| {
        (dates.start, dates.end, dates.all_day) != (input.start, input.end, input.all_day)
    }) || !keep_rule;

    // Attendees change only on the account's own events; on an invitation the organizer's list
    // stays as it is.
    let people = people.filter(|people| organized_by(event, Some(&people.organizer.address)));
    let fresh = new_calendar(uid, input, people, time_zone, now, sequence + 1)?;
    let fresh = ICalendar::parse(&fresh).map_err(|_| Error::InvalidEventTime)?;
    let fresh_index = master(&fresh).ok_or(Error::InvalidEventTime)?;
    let managed = [
        ICalendarProperty::Summary,
        ICalendarProperty::Location,
        ICalendarProperty::Description,
        ICalendarProperty::Dtstart,
        ICalendarProperty::Dtend,
        ICalendarProperty::Duration,
        ICalendarProperty::Sequence,
        ICalendarProperty::Dtstamp,
        ICalendarProperty::LastModified,
    ];
    let replaced = |name: &ICalendarProperty| {
        managed.contains(name) || (*name == ICalendarProperty::Rrule && !keep_rule)
    };
    let dropped = |name: &ICalendarProperty| {
        moved && matches!(name, ICalendarProperty::Exdate | ICalendarProperty::Rdate)
    };
    let master_event = &mut calendar.components[index];
    master_event
        .entries
        .retain(|entry| !replaced(&entry.name) && !dropped(&entry.name));
    master_event.entries.extend(
        fresh.components[fresh_index]
            .entries
            .iter()
            .filter(|entry| replaced(&entry.name) && entry.name != ICalendarProperty::Uid)
            .cloned(),
    );
    if old_reminder != input.reminder {
        // The dialog's reminder replaces the event's alarms.
        let alarms: Vec<u32> = calendar.components[index]
            .component_ids
            .iter()
            .copied()
            .filter(|id| {
                calendar
                    .components
                    .get(*id as usize)
                    .is_some_and(|component| {
                        component.component_type == ICalendarComponentType::VAlarm
                    })
            })
            .collect();
        calendar.components[index]
            .component_ids
            .retain(|id| !alarms.contains(id));
        let fresh_alarms: Vec<_> = fresh.components[fresh_index]
            .component_ids
            .iter()
            .filter_map(|id| fresh.components.get(*id as usize))
            .filter(|component| component.component_type == ICalendarComponentType::VAlarm)
            .cloned()
            .collect();
        for alarm in fresh_alarms {
            let id =
                u32::try_from(calendar.components.len()).map_err(|_| Error::InvalidEventTime)?;
            calendar.components.push(alarm);
            calendar.components[index].component_ids.push(id);
        }
    }
    let master_event = &mut calendar.components[index];
    if people.is_some() {
        // Attendees invited before keep their entries, answers included; new ones come from
        // the fresh event, and the organizer stays while anyone is invited.
        let before: Vec<_> = master_event
            .entries
            .iter()
            .filter(|entry| {
                matches!(
                    entry.name,
                    ICalendarProperty::Attendee | ICalendarProperty::Organizer
                )
            })
            .cloned()
            .collect();
        master_event.entries.retain(|entry| {
            !matches!(
                entry.name,
                ICalendarProperty::Attendee | ICalendarProperty::Organizer
            )
        });
        let same = |a: &calcard::icalendar::ICalendarEntry,
                    b: &calcard::icalendar::ICalendarEntry| {
            a.name == b.name
                && ical::person(a)
                    .zip(ical::person(b))
                    .is_some_and(|(a, b)| a.address.eq_ignore_ascii_case(&b.address))
        };
        for entry in fresh.components[fresh_index]
            .entries
            .iter()
            .filter(|entry| {
                matches!(
                    entry.name,
                    ICalendarProperty::Attendee | ICalendarProperty::Organizer
                )
            })
        {
            let kept = before.iter().find(|old| same(old, entry)).unwrap_or(entry);
            master_event.entries.push(kept.clone());
        }
    }
    if moved {
        let overrides: Vec<u32> = calendar
            .components
            .iter()
            .enumerate()
            .filter(|(position, component)| {
                *position != index
                    && component.component_type == ICalendarComponentType::VEvent
                    && component.has_property(&ICalendarProperty::RecurrenceId)
            })
            .filter_map(|(position, _)| u32::try_from(position).ok())
            .collect();
        calendar.components[0]
            .component_ids
            .retain(|id| !overrides.contains(id));
    }
    calendar.add_missing_timezones();
    Ok(calendar.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    const HOUR: i64 = 3_600_000;
    /// Tuesday, 20 October 2026, 10:00 in Berlin (summer time, UTC+2).
    const TUESDAY_TEN: i64 = 1_792_483_200_000;

    fn input(title: &str, start: i64, end: i64) -> EventInput {
        EventInput {
            id: None,
            calendar_id: LOCAL_CALENDAR.into(),
            title: title.into(),
            location: String::new(),
            description: String::new(),
            start,
            end,
            all_day: false,
            repeat: None,
            attendees: Vec::new(),
            reminder: None,
            instance: None,
        }
    }

    #[test]
    fn the_calendar_on_this_computer_is_there() {
        let db = Database::open_in_memory().unwrap();
        let calendars = list_calendars(&db.conn()).unwrap();
        assert_eq!(calendars.len(), 1);
        assert_eq!(calendars[0].id, LOCAL_CALENDAR);
        assert_eq!(calendars[0].account_id, None);
        assert!(calendars[0].visible);
        set_calendar_visible(&db.conn(), LOCAL_CALENDAR, false).unwrap();
        assert!(!list_calendars(&db.conn()).unwrap()[0].visible);

        let standard = calendars[0].color.clone();
        assert!(!calendars[0].own_color);
        set_calendar_color(&db.conn(), LOCAL_CALENDAR, Some("oklch(0.72 0.14 150)")).unwrap();
        let chosen = &list_calendars(&db.conn()).unwrap()[0];
        assert_eq!(chosen.color, "oklch(0.72 0.14 150)");
        assert!(chosen.own_color);
        set_calendar_color(&db.conn(), LOCAL_CALENDAR, None).unwrap();
        assert_eq!(list_calendars(&db.conn()).unwrap()[0].color, standard);
    }

    #[test]
    fn events_are_saved_found_changed_and_deleted() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn();
        let mut new = input("Zahnarzt", TUESDAY_TEN, TUESDAY_TEN + HOUR);
        new.location = "Praxis Dr. Sommer, Hauptstr. 3".into();
        let id = save_event_in(&conn, &new, Some("Europe/Berlin")).unwrap();

        let ics: String = conn
            .query_row("SELECT ics FROM events WHERE id = ?1", [&id], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(
            ics.contains("DTSTART;TZID=Europe/Berlin:20261020T100000"),
            "{ics}"
        );
        assert!(ics.contains("BEGIN:VTIMEZONE"), "{ics}");
        assert!(
            ics.contains("LOCATION:Praxis Dr. Sommer\\, Hauptstr. 3"),
            "{ics}"
        );

        let found = occurrences(&conn, TUESDAY_TEN - HOUR, TUESDAY_TEN + 2 * HOUR).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title.as_deref(), Some("Zahnarzt"));
        assert_eq!(found[0].start, TUESDAY_TEN);
        assert!(
            occurrences(&conn, TUESDAY_TEN + HOUR, TUESDAY_TEN + 2 * HOUR)
                .unwrap()
                .is_empty()
        );

        let changed = EventInput {
            id: Some(id.clone()),
            title: "Zahnarzt (Kontrolle)".into(),
            start: TUESDAY_TEN + HOUR,
            end: TUESDAY_TEN + 2 * HOUR,
            ..new
        };
        assert_eq!(
            save_event_in(&conn, &changed, Some("Europe/Berlin")).unwrap(),
            id
        );
        let event = get_event(&conn, &id).unwrap();
        assert_eq!(event.title.as_deref(), Some("Zahnarzt (Kontrolle)"));
        assert_eq!(event.start, TUESDAY_TEN + HOUR);
        assert_eq!(
            event.location.as_deref(),
            Some("Praxis Dr. Sommer, Hauptstr. 3")
        );

        delete_event(&conn, &id).unwrap();
        assert!(matches!(get_event(&conn, &id), Err(Error::EventNotFound)));
    }

    #[test]
    fn a_weekly_series_keeps_its_local_time_across_the_clock_change() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn();
        let series = EventInput {
            repeat: Some(Frequency::Weekly),
            ..input("Jour fixe", TUESDAY_TEN, TUESDAY_TEN + HOUR)
        };
        let id = save_event_in(&conn, &series, Some("Europe/Berlin")).unwrap();
        let found = occurrences(&conn, TUESDAY_TEN, TUESDAY_TEN + 14 * DAY_MS).unwrap();
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|occurrence| occurrence.event_id == id));
        // 27 October is in winter time: 10:00 is one hour later in UTC.
        assert_eq!(found[1].start, TUESDAY_TEN + 7 * DAY_MS + HOUR);
        let event = get_event(&conn, &id).unwrap();
        assert_eq!(event.recurrence.unwrap().frequency, Frequency::Weekly);
    }

    #[test]
    fn old_series_without_end_find_their_dates_now() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn();
        // Daily since 2000, weekly since 2010 with an end in 2020: only the daily one goes on.
        let year = 365 * DAY_MS;
        let daily = EventInput {
            repeat: Some(Frequency::Daily),
            ..input(
                "Täglich",
                TUESDAY_TEN - 26 * year,
                TUESDAY_TEN - 26 * year + HOUR,
            )
        };
        save_event_in(&conn, &daily, Some("Europe/Berlin")).unwrap();
        let ended = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:ended\r\n\
DTSTART:20100105T090000Z\r\nDTEND:20100105T100000Z\r\nRRULE:FREQ=WEEKLY;UNTIL=20200101T000000Z\r\n\
SUMMARY:Vorbei\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        store_remote(&conn, LOCAL_CALENDAR, "ended.ics", None, ended).unwrap();
        let started = std::time::Instant::now();
        let week = occurrences(&conn, TUESDAY_TEN - HOUR, TUESDAY_TEN + 7 * DAY_MS - HOUR).unwrap();
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        assert_eq!(week.len(), 7);
        assert!(
            week.iter()
                .all(|date| date.title.as_deref() == Some("Täglich"))
        );
        // Its first date is still where it began.
        let id = &week[0].event_id;
        assert_eq!(get_event(&conn, id).unwrap().start, TUESDAY_TEN - 26 * year);
    }

    #[test]
    fn one_date_of_a_series_is_changed_or_left_out() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn();
        let series = EventInput {
            repeat: Some(Frequency::Weekly),
            ..input("Jour fixe", TUESDAY_TEN, TUESDAY_TEN + HOUR)
        };
        let id = save_event_in(&conn, &series, Some("Europe/Berlin")).unwrap();
        let dates = occurrences(&conn, TUESDAY_TEN, TUESDAY_TEN + 21 * DAY_MS).unwrap();
        assert_eq!(dates.len(), 3);
        let (second, third) = (dates[1].instance.unwrap(), dates[2].instance.unwrap());
        assert_eq!(second, dates[1].start);

        // The second date two hours later, with its own title.
        let moved = EventInput {
            id: Some(id.clone()),
            title: "Jour fixe (verschoben)".into(),
            start: second + 2 * HOUR,
            end: second + 3 * HOUR,
            instance: Some(second),
            ..series.clone()
        };
        assert_eq!(
            save_event_in(&conn, &moved, Some("Europe/Berlin")).unwrap(),
            id
        );
        let one = get_event_at(&conn, &id, Some(second)).unwrap();
        assert_eq!(one.title.as_deref(), Some("Jour fixe (verschoben)"));
        assert_eq!(one.start, second + 2 * HOUR);
        assert_eq!(one.instance, Some(second));
        assert_eq!(one.recurrence.unwrap().frequency, Frequency::Weekly);
        assert_eq!(
            get_event(&conn, &id).unwrap().title.as_deref(),
            Some("Jour fixe")
        );

        // Changed again: still one changed date, not two.
        let again = EventInput {
            start: second + 4 * HOUR,
            end: second + 5 * HOUR,
            ..moved.clone()
        };
        save_event_in(&conn, &again, Some("Europe/Berlin")).unwrap();
        let ics: String = conn
            .query_row("SELECT ics FROM events WHERE id = ?1", [&id], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(ics.matches("RECURRENCE-ID").count(), 1, "{ics}");
        assert!(
            ics.contains("RECURRENCE-ID;TZID=Europe/Berlin:20261027T100000"),
            "{ics}"
        );

        // The third date left out.
        delete_event_at(&conn, &id, Some(third)).unwrap();
        let dates = occurrences(&conn, TUESDAY_TEN, TUESDAY_TEN + 21 * DAY_MS).unwrap();
        let starts: Vec<i64> = dates.iter().map(|date| date.start).collect();
        assert_eq!(starts, vec![TUESDAY_TEN, second + 4 * HOUR]);
        assert_eq!(dates[1].title.as_deref(), Some("Jour fixe (verschoben)"));
        assert_eq!(dates[1].instance, Some(second));

        // A new title for the series keeps the changed date.
        let renamed = EventInput {
            id: Some(id.clone()),
            title: "Teamrunde".into(),
            ..series.clone()
        };
        save_event_in(&conn, &renamed, Some("Europe/Berlin")).unwrap();
        let dates = occurrences(&conn, TUESDAY_TEN, TUESDAY_TEN + 21 * DAY_MS).unwrap();
        assert_eq!(dates.len(), 2);
        assert_eq!(dates[0].title.as_deref(), Some("Teamrunde"));
        assert_eq!(dates[1].start, second + 4 * HOUR);

        // Nothing waits for a server: the calendar is on this computer.
        let waiting: i64 = conn
            .query_row("SELECT count(*) FROM pending_instances", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(waiting, 0);
    }

    #[test]
    fn reminders_come_due_before_each_date() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn();
        let series = EventInput {
            repeat: Some(Frequency::Weekly),
            reminder: Some(15),
            ..input("Jour fixe", TUESDAY_TEN, TUESDAY_TEN + HOUR)
        };
        let id = save_event_in(&conn, &series, Some("Europe/Berlin")).unwrap();
        assert_eq!(get_event(&conn, &id).unwrap().reminder, Some(15));
        // The second date, a week on (winter time: 10:00 is an hour later in UTC).
        let second = TUESDAY_TEN + 7 * DAY_MS + HOUR;
        let due = due_reminders(&conn, second - 20 * 60_000, second).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].at, second - 15 * 60_000);
        assert_eq!(due[0].start, second);
        assert_eq!(due[0].title.as_deref(), Some("Jour fixe"));
        // Not again in the next minute.
        assert!(
            due_reminders(&conn, second - 15 * 60_000, second)
                .unwrap()
                .is_empty()
        );

        // An hour before instead: the alarm is replaced, not added.
        let changed = EventInput {
            id: Some(id.clone()),
            reminder: Some(60),
            ..series.clone()
        };
        save_event_in(&conn, &changed, Some("Europe/Berlin")).unwrap();
        assert_eq!(get_event(&conn, &id).unwrap().reminder, Some(60));
        let ics: String = conn
            .query_row("SELECT ics FROM events WHERE id = ?1", [&id], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(ics.matches("BEGIN:VALARM").count(), 1, "{ics}");
        // And none.
        let none = EventInput {
            id: Some(id.clone()),
            reminder: None,
            ..series
        };
        save_event_in(&conn, &none, Some("Europe/Berlin")).unwrap();
        assert_eq!(get_event(&conn, &id).unwrap().reminder, None);
        assert!(due_reminders(&conn, 0, i64::MAX / 4).unwrap().is_empty());
    }

    #[test]
    fn all_day_events_cover_whole_days() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn();
        // 12 November 2026, midnight UTC, for two days.
        let day = 1_794_441_600_000;
        let id = save_event_in(
            &conn,
            &EventInput {
                all_day: true,
                ..input("Messe", day, day + 2 * DAY_MS)
            },
            Some("Europe/Berlin"),
        )
        .unwrap();
        let event = get_event(&conn, &id).unwrap();
        assert!(event.all_day);
        assert_eq!((event.start, event.end), (day, day + 2 * DAY_MS));
        let found = occurrences(&conn, day + DAY_MS, day + DAY_MS + HOUR).unwrap();
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn read_only_calendars_and_backwards_times_are_refused() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.conn();
        assert!(matches!(
            save_event_in(&conn, &input("x", TUESDAY_TEN, TUESDAY_TEN - 1), None),
            Err(Error::InvalidEventTime)
        ));
        conn.execute(
            "INSERT INTO calendars (id, name, color, read_only) VALUES ('holidays', 'Feiertage', 'red', 1)",
            [],
        )
        .unwrap();
        let holiday = EventInput {
            calendar_id: "holidays".into(),
            ..input("x", TUESDAY_TEN, TUESDAY_TEN + HOUR)
        };
        assert!(matches!(
            save_event_in(&conn, &holiday, None),
            Err(Error::CalendarReadOnly)
        ));
    }
}
