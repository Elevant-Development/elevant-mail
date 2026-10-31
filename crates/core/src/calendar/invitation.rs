//! Invitations that come by mail (iTIP, RFC 5546): what the reader shows of a message's calendar
//! part, and the answer that goes back to the organizer.
//!
//! The answer is a mail to the organizer with a `METHOD:REPLY` calendar part, sent over the
//! account's SMTP server like any mail, but not filed in Sent. Microsoft accounts answer over
//! Graph where the invitation is in their calendar: Exchange then tells the organizer and keeps
//! the calendar in step. What the user answered is kept per
//! event (`invitation_replies`), so it still shows when the invitation comes again.

use calcard::common::PartialDateTime;
use calcard::icalendar::{
    ICalendar, ICalendarComponentType, ICalendarEntry, ICalendarMethod, ICalendarParameter,
    ICalendarParameterName, ICalendarParticipationStatus, ICalendarProperty,
    ICalendarScheduleAgentValue, ICalendarValue, Uri,
};
use lettre::Message;
use lettre::message::header::ContentType;
use lettre::message::{Mailbox, MultiPart, SinglePart};
use lettre::transport::smtp::client::TlsParameters;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::graph::{self, Graph};
use super::{Attendee, PRODUCT_ID, Recurrence, events, ical};
use crate::accounts::{self, SignInMethod};
use crate::db::Database;
use crate::mail::send;
use crate::mail::smtp;
use crate::model::Address;
use crate::secrets::SecretStore;
use crate::{Error, Result};

/// What the reader shows above a mail that carries an invitation, a cancellation or an answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Invitation {
    pub method: InvitationMethod,
    pub title: Option<String>,
    pub location: Option<String>,
    /// Unix milliseconds. All-day events start at midnight UTC of their first day and end at
    /// midnight UTC of the day after their last, so they show the same dates everywhere.
    #[ts(type = "number")]
    pub start: i64,
    #[ts(type = "number")]
    pub end: i64,
    pub all_day: bool,
    /// For a series: how it repeats. `start` is its first date.
    pub recurrence: Option<Recurrence>,
    pub organizer: Option<Address>,
    /// Who is invited, as the mail lists them; for an answer (`reply`), the one who answered.
    pub attendees: Vec<Attendee>,
    /// What the user answered in this app, to this version of the invitation or an earlier one.
    pub answer: Option<Answer>,
    /// The answer was to an earlier version: the organizer changed the event since.
    pub changed_since_answer: bool,
    /// An invitation the account can answer: it asks, and the account did not send it itself.
    pub can_answer: bool,
    /// The event in the account's calendars (or the one on this computer), where it is.
    pub in_calendar: Option<CalendarEntry>,
    /// Other events at that time, a few at most; for an invitation only.
    pub conflicts: Vec<Conflict>,
}

/// Where an event is in the calendars.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CalendarEntry {
    pub event_id: String,
    pub calendar_id: String,
}

/// An event at the same time as an invitation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Conflict {
    pub title: Option<String>,
    #[ts(type = "number")]
    pub start: i64,
    #[ts(type = "number")]
    pub end: i64,
}

/// Conflicts named in the card.
const SHOWN_CONFLICTS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum InvitationMethod {
    /// An invitation, new or changed.
    Request,
    /// The organizer called the event off.
    Cancel,
    /// Someone answered an invitation.
    Reply,
    /// An event to put into the calendar, no answer expected.
    Publish,
    Other,
}

/// The user's answer to an invitation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Answer {
    Accepted,
    Tentative,
    Declined,
}

impl Answer {
    fn as_str(self) -> &'static str {
        match self {
            Answer::Accepted => "accepted",
            Answer::Tentative => "tentative",
            Answer::Declined => "declined",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "accepted" => Answer::Accepted,
            "tentative" => Answer::Tentative,
            "declined" => Answer::Declined,
            _ => return None,
        })
    }

    /// Graph's action for it on the event.
    fn graph_action(self) -> &'static str {
        match self {
            Answer::Accepted => "accept",
            Answer::Tentative => "tentativelyAccept",
            Answer::Declined => "decline",
        }
    }

    fn participation(self) -> ICalendarParticipationStatus {
        match self {
            Answer::Accepted => ICalendarParticipationStatus::Accepted,
            Answer::Tentative => ICalendarParticipationStatus::Tentative,
            Answer::Declined => ICalendarParticipationStatus::Declined,
        }
    }
}

/// What the user chose, with the mail that carries it, written in the user's language.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct InvitationAnswer {
    /// The cached mail with the invitation.
    pub message_id: String,
    pub answer: Answer,
    pub subject: String,
    pub text: String,
}

/// An invitation as read, with what is needed to answer it.
struct Parsed {
    calendar: ICalendar,
    /// Index of the event in `calendar.components`.
    event: usize,
    uid: String,
    sequence: i64,
    /// Empty for the whole event or series.
    recurrence_id: String,
    invitation: Invitation,
}

/// Reads the calendar part of a mail. `None` for parts without an event.
fn parse(ics: &str, own_address: &str) -> Option<Parsed> {
    let calendar = ICalendar::parse(ics).ok()?;
    let root = calendar.components.first()?;
    if root.component_type != ICalendarComponentType::VCalendar {
        return None;
    }
    let method = match root
        .property(&ICalendarProperty::Method)
        .and_then(|entry| entry.values.first())
    {
        Some(ICalendarValue::Method(ICalendarMethod::Request)) => InvitationMethod::Request,
        Some(ICalendarValue::Method(ICalendarMethod::Cancel)) => InvitationMethod::Cancel,
        Some(ICalendarValue::Method(ICalendarMethod::Reply)) => InvitationMethod::Reply,
        Some(ICalendarValue::Method(ICalendarMethod::Publish)) | None => InvitationMethod::Publish,
        Some(_) => InvitationMethod::Other,
    };
    // The event itself; an invitation to one date of a series has only that date's override.
    let events = || {
        calendar
            .components
            .iter()
            .enumerate()
            .filter(|(_, component)| component.component_type == ICalendarComponentType::VEvent)
    };
    let (index, event) = events()
        .find(|(_, event)| !event.has_property(&ICalendarProperty::RecurrenceId))
        .or_else(|| events().next())?;
    let uid = event.uid()?.trim().to_owned();
    let (start, end, all_day) = ical::first_occurrence(&calendar, index)?;

    let organizer = event
        .property(&ICalendarProperty::Organizer)
        .and_then(ical::person);
    let attendees = ical::attendees(event);
    let can_answer = method == InvitationMethod::Request
        && organizer
            .as_ref()
            .is_some_and(|organizer| !organizer.address.eq_ignore_ascii_case(own_address));

    Some(Parsed {
        event: index,
        uid,
        sequence: event
            .property(&ICalendarProperty::Sequence)
            .and_then(|entry| entry.values.first())
            .and_then(ICalendarValue::as_integer)
            .unwrap_or(0),
        recurrence_id: event
            .property(&ICalendarProperty::RecurrenceId)
            .and_then(|entry| entry.values.first())
            .and_then(ICalendarValue::as_partial_date_time)
            .and_then(PartialDateTime::to_rfc3339)
            .unwrap_or_default(),
        invitation: Invitation {
            method,
            title: ical::text(event, &ICalendarProperty::Summary),
            location: ical::text(event, &ICalendarProperty::Location),
            start,
            end,
            all_day,
            recurrence: ical::recurrence(event),
            organizer,
            attendees,
            answer: None,
            changed_since_answer: false,
            can_answer,
            in_calendar: None,
            conflicts: Vec::new(),
        },
        calendar,
    })
}

/// The cached mail's calendar part, its account, the account's address, and its Message-ID.
fn load(conn: &Connection, message_id: &str) -> Result<Option<(String, String, String)>> {
    let row: Option<(Option<String>, String, String)> = conn
        .query_row(
            "SELECT m.calendar, m.account_id, a.email_address
             FROM messages m JOIN accounts a ON a.id = m.account_id WHERE m.id = ?1",
            [message_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    Ok(row.and_then(|(calendar, account, email)| Some((calendar?, account, email))))
}

/// The invitation a cached mail carries, with what the user answered to it.
pub fn for_message(conn: &Connection, message_id: &str) -> Result<Option<Invitation>> {
    let Some((ics, account_id, email)) = load(conn, message_id)? else {
        return Ok(None);
    };
    let Some(parsed) = parse(&ics, &email) else {
        return Ok(None);
    };
    let mut invitation = parsed.invitation;
    let replied: Option<(String, i64)> = conn
        .query_row(
            "SELECT answer, sequence FROM invitation_replies
             WHERE account_id = ?1 AND event_uid = ?2 AND recurrence_id = ?3",
            params![account_id, parsed.uid, parsed.recurrence_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if let Some((answer, sequence)) = replied {
        invitation.answer = Answer::parse(&answer);
        invitation.changed_since_answer = sequence < parsed.sequence;
    }
    invitation.in_calendar = in_calendar(conn, &account_id, &parsed.uid)?;
    if invitation.method == InvitationMethod::Request && !invitation.all_day {
        let own = invitation
            .in_calendar
            .as_ref()
            .map(|entry| entry.event_id.as_str());
        invitation.conflicts = events::occurrences(conn, invitation.start, invitation.end)?
            .into_iter()
            .filter(|occurrence| !occurrence.all_day && Some(occurrence.event_id.as_str()) != own)
            .take(SHOWN_CONFLICTS)
            .map(|occurrence| Conflict {
                title: occurrence.title,
                start: occurrence.start,
                end: occurrence.end,
            })
            .collect();
    }
    Ok(Some(invitation))
}

/// The event with this UID in the account's calendars or the one on this computer.
fn in_calendar(conn: &Connection, account_id: &str, uid: &str) -> Result<Option<CalendarEntry>> {
    Ok(conn
        .query_row(
            "SELECT e.id, e.calendar_id FROM events e JOIN calendars c ON c.id = e.calendar_id
             WHERE e.uid = ?2 AND e.pending IS NOT 'delete'
               AND (c.account_id = ?1 OR c.account_id IS NULL)
             ORDER BY c.account_id IS NULL LIMIT 1",
            params![account_id, uid],
            |row| {
                Ok(CalendarEntry {
                    event_id: row.get(0)?,
                    calendar_id: row.get(1)?,
                })
            },
        )
        .optional()?)
}

/// After an answer by mail: an accepted (or maybe) event goes into the account's calendar, as
/// the organizer sent it with the answer filled in; a declined one goes out of it. The copy
/// says `SCHEDULE-AGENT=CLIENT`, so a server that schedules by itself does not answer again.
fn file_in_calendar(
    conn: &Connection,
    account_id: &str,
    parsed: &Parsed,
    own_address: &str,
    answer: Answer,
) -> Result<()> {
    let existing = in_calendar(conn, account_id, &parsed.uid)?;
    if answer == Answer::Declined {
        if let Some(entry) = existing {
            events::delete_event(conn, &entry.event_id)?;
        }
        return Ok(());
    }
    let calendar_id = match &existing {
        Some(entry) => entry.calendar_id.clone(),
        None => conn
            .query_row(
                "SELECT id FROM calendars WHERE account_id = ?1 AND read_only = 0
                 ORDER BY position LIMIT 1",
                [account_id],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or_else(|| events::LOCAL_CALENDAR.to_owned()),
    };
    let mut calendar = parsed.calendar.clone();
    calendar.components[0]
        .entries
        .retain(|entry| entry.name != ICalendarProperty::Method);
    for component in &mut calendar.components {
        if component.component_type != ICalendarComponentType::VEvent {
            continue;
        }
        for entry in &mut component.entries {
            let is_own = entry.name == ICalendarProperty::Attendee
                && ical::person(entry)
                    .is_some_and(|person| person.address.eq_ignore_ascii_case(own_address));
            if is_own {
                entry.params.retain(|param| {
                    !matches!(
                        param.name,
                        ICalendarParameterName::Partstat | ICalendarParameterName::Rsvp
                    )
                });
                entry
                    .params
                    .push(ICalendarParameter::partstat(answer.participation()));
            } else if entry.name == ICalendarProperty::Organizer {
                entry
                    .params
                    .retain(|param| param.name != ICalendarParameterName::ScheduleAgent);
                entry.params.push(ICalendarParameter::schedule_agent(
                    ICalendarScheduleAgentValue::Client,
                ));
            }
        }
    }
    events::store_own(
        conn,
        &calendar_id,
        existing.as_ref().map(|entry| entry.event_id.as_str()),
        &calendar.to_string(),
    )?;
    Ok(())
}

/// Sends the answer to the organizer and keeps it. Returns the account.
pub async fn answer(
    db: &Database,
    secrets: &dyn SecretStore,
    answer: &InvitationAnswer,
) -> Result<String> {
    let account_id: String = db
        .conn()
        .query_row(
            "SELECT account_id FROM messages WHERE id = ?1",
            [&answer.message_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(Error::MessageNotFound)?;
    let settings = accounts::settings(&db.conn(), &account_id)?;
    let graph = match settings.sign_in {
        SignInMethod::Microsoft => match crate::oauth::graph_token(secrets, &account_id).await {
            Ok(token) => Some(Graph::new(graph::GRAPH, token)?),
            // Signed in before the app asked for calendars: the answer goes by mail.
            Err(Error::CalendarAccessMissing) => None,
            Err(error) => return Err(error),
        },
        SignInMethod::Password => None,
    };
    let tls = smtp::platform_tls(&settings.smtp.host)?;
    answer_with(db, secrets, answer, graph.as_ref(), tls).await
}

pub(crate) async fn answer_with(
    db: &Database,
    secrets: &dyn SecretStore,
    answer: &InvitationAnswer,
    graph: Option<&Graph>,
    tls: TlsParameters,
) -> Result<String> {
    let (ics, account_id, email) =
        load(&db.conn(), &answer.message_id)?.ok_or(Error::MessageNotFound)?;
    let parsed = parse(&ics, &email).ok_or(Error::MessageNotFound)?;
    // Only the organizer can take the answer.
    let organizer = match &parsed.invitation.organizer {
        Some(organizer) if parsed.invitation.can_answer => organizer.clone(),
        _ => return Err(Error::NoRecipients),
    };
    if let Some(graph) = graph
        && let Some(event_id) = graph.find_by_uid(&parsed.uid).await?
    {
        graph
            .respond(&event_id, answer.answer.graph_action())
            .await?;
        tracing::info!(account = %account_id, "invitation answered over Graph");
        record(&db.conn(), &account_id, &parsed, answer.answer)?;
        return Ok(account_id);
    }
    let settings = accounts::settings(&db.conn(), &account_id)?;
    // As invited, in case the invitation spells the address differently.
    let own = parsed
        .invitation
        .attendees
        .iter()
        .find(|attendee| attendee.address.address.eq_ignore_ascii_case(&email))
        .map_or_else(
            || Address {
                name: None,
                address: email.clone(),
            },
            |attendee| attendee.address.clone(),
        );
    let name = Some(settings.sender_name.trim())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .or_else(|| own.name.clone());
    let reply = reply_calendar(&parsed, &own.address, name.as_deref(), answer.answer);
    let message = reply_mail(
        &db.conn(),
        answer,
        &email,
        name.as_deref(),
        &organizer,
        reply,
    )?;
    let credential = send::credential(secrets, &account_id, settings.sign_in).await?;
    smtp::send_with(&settings.smtp, &credential, tls, message).await?;
    tracing::info!(account = %account_id, "invitation answered");
    record(&db.conn(), &account_id, &parsed, answer.answer)?;
    file_in_calendar(
        &db.conn(),
        &account_id,
        &parsed,
        &own.address,
        answer.answer,
    )?;
    Ok(account_id)
}

/// Keeps the answer for the event, so later copies of the invitation show it.
fn record(conn: &Connection, account_id: &str, parsed: &Parsed, answer: Answer) -> Result<()> {
    conn.execute(
        "INSERT INTO invitation_replies
           (account_id, event_uid, recurrence_id, answer, sequence, replied_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (account_id, event_uid, recurrence_id) DO UPDATE
           SET answer = excluded.answer, sequence = excluded.sequence,
             replied_at = excluded.replied_at",
        params![
            account_id,
            parsed.uid,
            parsed.recurrence_id,
            answer.as_str(),
            parsed.sequence,
            chrono::Utc::now().timestamp_millis(),
        ],
    )?;
    Ok(())
}

/// The `METHOD:REPLY` calendar: the event as the organizer knows it (UID, SEQUENCE,
/// RECURRENCE-ID, and the time zones these refer to), with only the account as attendee.
fn reply_calendar(parsed: &Parsed, address: &str, name: Option<&str>, answer: Answer) -> String {
    let mut calendar = parsed.calendar.clone();
    let kept: Vec<u32> = calendar.components[0]
        .component_ids
        .iter()
        .copied()
        .filter(|&id| {
            id as usize == parsed.event
                || calendar
                    .components
                    .get(id as usize)
                    .is_some_and(|component| {
                        component.component_type == ICalendarComponentType::VTimezone
                    })
        })
        .collect();
    let root = &mut calendar.components[0];
    root.component_ids = kept;
    root.entries.retain(|entry| {
        !matches!(
            entry.name,
            ICalendarProperty::Method | ICalendarProperty::Prodid
        )
    });
    root.entries.insert(
        0,
        ICalendarEntry::new(ICalendarProperty::Prodid)
            .with_value(ICalendarValue::Text(PRODUCT_ID.into())),
    );
    root.entries.push(
        ICalendarEntry::new(ICalendarProperty::Method)
            .with_value(ICalendarValue::Method(ICalendarMethod::Reply)),
    );

    let event = &mut calendar.components[parsed.event];
    // Alarms and the rest of the details stay with the organizer.
    event.component_ids.clear();
    event.entries.retain(|entry| {
        matches!(
            entry.name,
            ICalendarProperty::Uid
                | ICalendarProperty::Sequence
                | ICalendarProperty::RecurrenceId
                | ICalendarProperty::Organizer
                | ICalendarProperty::Summary
                | ICalendarProperty::Dtstart
                | ICalendarProperty::Dtend
                | ICalendarProperty::Duration
        )
    });
    event
        .entries
        .push(ICalendarEntry::new(ICalendarProperty::Dtstamp).with_value(PartialDateTime::now()));
    let mut attendee_params = vec![ICalendarParameter::partstat(answer.participation())];
    if let Some(name) = name {
        attendee_params.insert(0, ICalendarParameter::cn(name.to_owned()));
    }
    event.entries.push(
        ICalendarEntry::new(ICalendarProperty::Attendee)
            .with_params(attendee_params)
            .with_value(Uri::Location(format!("mailto:{address}"))),
    );
    calendar.to_string()
}

/// The answer as mail: a line of text and the calendar part, in the invitation's thread.
fn reply_mail(
    conn: &Connection,
    answer: &InvitationAnswer,
    email: &str,
    name: Option<&str>,
    organizer: &Address,
    calendar: String,
) -> Result<Message> {
    let from = Mailbox::new(
        name.map(str::to_owned),
        email.parse().map_err(|_| Error::InvalidEmail)?,
    );
    let to = Mailbox::new(
        organizer.name.clone(),
        organizer
            .address
            .parse()
            .map_err(|_| Error::InvalidRecipient(organizer.address.clone()))?,
    );
    let mut builder = Message::builder()
        .from(from)
        .to(to)
        .subject(answer.subject.trim())
        .date_now()
        .message_id(Some(send::new_message_id(email)));
    if let Some((parent, references)) = send::thread_headers(conn, &answer.message_id)? {
        builder = builder.in_reply_to(parent).references(references);
    }
    let calendar_type =
        ContentType::parse("text/calendar; method=REPLY; charset=utf-8").expect("a media type");
    builder
        .multipart(
            MultiPart::alternative()
                .singlepart(SinglePart::plain(answer.text.clone()))
                .singlepart(SinglePart::builder().header(calendar_type).body(calendar)),
        )
        .map_err(|error| Error::MessageRejected(error.to_string()))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::calendar::{Frequency, Participation};

    /// An invitation as Outlook sends it: Windows time zone, a weekly series, the organizer and
    /// two attendees.
    pub(crate) const OUTLOOK: &str = "BEGIN:VCALENDAR\r\n\
METHOD:REQUEST\r\n\
PRODID:Microsoft Exchange Server 2010\r\n\
VERSION:2.0\r\n\
BEGIN:VTIMEZONE\r\n\
TZID:W. Europe Standard Time\r\n\
BEGIN:STANDARD\r\n\
DTSTART:16010101T030000\r\n\
TZOFFSETFROM:+0200\r\n\
TZOFFSETTO:+0100\r\n\
RRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=-1SU;BYMONTH=10\r\n\
END:STANDARD\r\n\
BEGIN:DAYLIGHT\r\n\
DTSTART:16010101T020000\r\n\
TZOFFSETFROM:+0100\r\n\
TZOFFSETTO:+0200\r\n\
RRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=-1SU;BYMONTH=3\r\n\
END:DAYLIGHT\r\n\
END:VTIMEZONE\r\n\
BEGIN:VEVENT\r\n\
ORGANIZER;CN=Lena Fischer:mailto:lena@example.org\r\n\
ATTENDEE;ROLE=REQ-PARTICIPANT;PARTSTAT=NEEDS-ACTION;RSVP=TRUE;CN=Alex Berger:mailto:Alex@Example.org\r\n\
ATTENDEE;ROLE=OPT-PARTICIPANT;PARTSTAT=ACCEPTED;CN=Jürgen:mailto:juergen@example.org\r\n\
SUMMARY;LANGUAGE=de-DE:Planung Herbstfest\r\n\
DTSTART;TZID=W. Europe Standard Time:20261103T100000\r\n\
DTEND;TZID=W. Europe Standard Time:20261103T110000\r\n\
RRULE:FREQ=WEEKLY;BYDAY=TU;COUNT=4\r\n\
UID:040000008200E00074C5B7101A82E008\r\n\
SEQUENCE:2\r\n\
LOCATION:Raum 1\r\n\
BEGIN:VALARM\r\n\
ACTION:DISPLAY\r\n\
TRIGGER:-PT15M\r\n\
END:VALARM\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n";

    #[test]
    fn reads_an_outlook_invitation() {
        let parsed = parse(OUTLOOK, "alex@example.org").unwrap();
        let invitation = &parsed.invitation;
        assert_eq!(invitation.method, InvitationMethod::Request);
        assert_eq!(invitation.title.as_deref(), Some("Planung Herbstfest"));
        assert_eq!(invitation.location.as_deref(), Some("Raum 1"));
        // 10:00 in Berlin on 3 November (winter time) is 09:00 UTC.
        assert_eq!(invitation.start, 1_793_696_400_000);
        assert_eq!(invitation.end - invitation.start, 3_600_000);
        assert!(!invitation.all_day);
        let recurrence = invitation.recurrence.as_ref().unwrap();
        assert_eq!(recurrence.frequency, Frequency::Weekly);
        assert_eq!(recurrence.weekdays, vec![1]);
        assert_eq!(recurrence.count, Some(4));
        assert_eq!(
            invitation.organizer,
            Some(Address {
                name: Some("Lena Fischer".into()),
                address: "lena@example.org".into()
            })
        );
        assert_eq!(invitation.attendees.len(), 2);
        assert_eq!(invitation.attendees[0].status, Participation::NeedsAction);
        assert!(invitation.attendees[1].optional);
        assert_eq!(invitation.attendees[1].status, Participation::Accepted);
        assert!(invitation.can_answer);
        assert_eq!(parsed.sequence, 2);
        assert_eq!(parsed.uid, "040000008200E00074C5B7101A82E008");
    }

    #[test]
    fn all_day_events_keep_their_dates() {
        let ics = "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\nUID:a\r\n\
ORGANIZER:mailto:lena@example.org\r\nSUMMARY:Betriebsausflug\r\n\
DTSTART;VALUE=DATE:20261112\r\nDTEND;VALUE=DATE:20261114\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let invitation = parse(ics, "alex@example.org").unwrap().invitation;
        assert!(invitation.all_day);
        // 12 November 2026, 00:00 UTC, for two days.
        assert_eq!(invitation.start, 1_794_441_600_000);
        assert_eq!(invitation.end - invitation.start, 2 * 86_400_000);
    }

    #[test]
    fn the_organizers_own_copy_and_cancellations_ask_nothing() {
        let own = parse(OUTLOOK, "lena@example.org").unwrap().invitation;
        assert!(!own.can_answer);
        let cancel = OUTLOOK.replace("METHOD:REQUEST", "METHOD:CANCEL");
        let cancel = parse(&cancel, "alex@example.org").unwrap().invitation;
        assert_eq!(cancel.method, InvitationMethod::Cancel);
        assert!(!cancel.can_answer);
    }

    #[test]
    fn the_reply_names_only_the_account() {
        let parsed = parse(OUTLOOK, "alex@example.org").unwrap();
        let reply = reply_calendar(
            &parsed,
            "Alex@Example.org",
            Some("Alex Berger"),
            Answer::Tentative,
        );
        assert!(reply.contains("METHOD:REPLY"), "{reply}");
        assert!(
            reply.contains("PRODID:-//Elevant//Elevant Mail//EN"),
            "{reply}"
        );
        assert!(
            reply.contains("UID:040000008200E00074C5B7101A82E008"),
            "{reply}"
        );
        assert!(reply.contains("SEQUENCE:2"), "{reply}");
        assert!(reply.contains("TZID:W. Europe Standard Time"), "{reply}");
        assert!(reply.contains("DTSTAMP:"), "{reply}");
        let attendees: Vec<&str> = reply
            .lines()
            .filter(|line| line.starts_with("ATTENDEE"))
            .collect();
        assert_eq!(attendees.len(), 1, "{reply}");
        assert!(attendees[0].contains("PARTSTAT=TENTATIVE"), "{reply}");
        assert!(attendees[0].contains("mailto:Alex@Example.org"), "{reply}");
        assert!(!reply.contains("VALARM"), "{reply}");
        assert!(!reply.contains("RRULE:FREQ=WEEKLY"), "{reply}");
        // What a calendar reads back from it.
        let back = parse(&reply, "lena@example.org").unwrap().invitation;
        assert_eq!(back.method, InvitationMethod::Reply);
        assert_eq!(back.attendees[0].status, Participation::Tentative);
        assert_eq!(back.start, parsed.invitation.start);
    }

    #[test]
    fn broken_calendars_are_no_invitation() {
        assert!(parse("BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n", "a@example.org").is_none());
        assert!(parse("not a calendar", "a@example.org").is_none());
    }
}
