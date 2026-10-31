//! Calendars of Microsoft 365 and Outlook.com over Microsoft Graph: listing them and their
//! events, translating events to iCalendar (series with their rule, changed and cancelled dates,
//! in the event's own time zone) and back for writing.
//!
//! Graph lists a calendar's single events and series masters; a master read on its own brings
//! its changed dates (`exceptionOccurrences`) and cancelled ones (`cancelledOccurrences`). Times
//! are asked for in UTC and put back into the time zone the event was made in, so a series keeps
//! its local time across clock changes.

use std::sync::Arc;
use std::time::Duration;

use calcard::icalendar::{ICalendar, ICalendarProperty};
use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};
use reqwest::{Method, StatusCode};
use rustls::ClientConfig;
use rustls_platform_verifier::BuilderVerifierExt;
use serde::Deserialize;
use serde_json::{Value, json};
use url::Url;

use super::{Frequency, PRODUCT_ID, ical};
use crate::secrets::Secret;
use crate::{Error, Result};

pub(crate) const GRAPH: &str = "https://graph.microsoft.com/v1.0/";
const TIMEOUT: Duration = Duration::from_secs(30);
/// Times in UTC, put into the event's own zone here; descriptions as text.
const PREFER: &str = r#"outlook.timezone="UTC", outlook.body-content-type="text""#;
/// Requests in one `$batch`, the most Graph takes.
const BATCH: usize = 20;
/// What the app reads of an event.
const FIELDS: &str = "id,iCalUId,subject,body,start,end,isAllDay,location,organizer,attendees,\
recurrence,originalStartTimeZone,originalStart,changeKey,type,isCancelled";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GraphCalendar {
    pub id: String,
    pub name: String,
    pub hex_color: Option<String>,
    #[serde(default)]
    pub can_edit: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GraphEvent {
    pub id: String,
    #[serde(rename = "iCalUId")]
    pub ical_uid: Option<String>,
    pub subject: Option<String>,
    pub body: Option<Body>,
    pub start: Option<When>,
    pub end: Option<When>,
    #[serde(default)]
    pub is_all_day: bool,
    pub location: Option<Location>,
    pub organizer: Option<Recipient>,
    #[serde(default)]
    pub attendees: Vec<GraphAttendee>,
    pub recurrence: Option<PatternedRecurrence>,
    pub original_start_time_zone: Option<String>,
    pub original_start: Option<String>,
    pub change_key: Option<String>,
    #[serde(default)]
    pub is_cancelled: bool,
    #[serde(default)]
    pub cancelled_occurrences: Vec<String>,
    #[serde(default)]
    pub exception_occurrences: Vec<GraphEvent>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Body {
    pub content: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct When {
    pub date_time: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Location {
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Recipient {
    pub email_address: Option<EmailAddress>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EmailAddress {
    pub name: Option<String>,
    pub address: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GraphAttendee {
    pub email_address: Option<EmailAddress>,
    pub status: Option<ResponseStatus>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResponseStatus {
    pub response: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PatternedRecurrence {
    pub pattern: Pattern,
    pub range: Range,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Pattern {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default = "one")]
    pub interval: u32,
    pub month: Option<u32>,
    pub day_of_month: Option<u32>,
    #[serde(default)]
    pub days_of_week: Vec<String>,
    pub first_day_of_week: Option<String>,
    pub index: Option<String>,
}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Range {
    #[serde(rename = "type")]
    pub kind: String,
    pub end_date: Option<String>,
    pub number_of_occurrences: Option<u32>,
}

#[derive(Deserialize)]
struct Page<T> {
    value: Vec<T>,
    #[serde(rename = "@odata.nextLink")]
    next: Option<String>,
}

#[derive(Deserialize)]
struct Batch {
    responses: Vec<BatchAnswer>,
}

#[derive(Deserialize)]
struct BatchAnswer {
    status: u16,
    #[serde(default)]
    body: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Listed {
    pub id: String,
    pub change_key: Option<String>,
}

/// How a write went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Written {
    /// Done; the event's id on the server.
    Done(String),
    /// Changed on the server since: its version stays.
    Conflict,
}

pub(crate) struct Graph {
    client: reqwest::Client,
    base: Url,
    token: Secret,
}

fn network_error(error: &reqwest::Error) -> Error {
    if error.is_timeout() {
        Error::Timeout
    } else if error.is_connect() {
        Error::ConnectionFailed(std::io::Error::other(error.to_string()))
    } else {
        Error::Protocol(error.to_string())
    }
}

impl Graph {
    /// `base` is Graph's address ([`GRAPH`]); tests point it at a local stand-in.
    pub fn new(base: &str, token: Secret) -> Result<Self> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let tls = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .and_then(BuilderVerifierExt::with_platform_verifier)
            .map_err(Error::TlsSetup)?
            .with_no_client_auth();
        let client = reqwest::Client::builder()
            .tls_backend_preconfigured(tls)
            .timeout(TIMEOUT)
            .build()
            .map_err(|error| Error::Protocol(error.to_string()))?;
        Ok(Self {
            client,
            base: Url::parse(base).map_err(|_| Error::InvalidLink)?,
            token,
        })
    }

    async fn send(
        &self,
        method: Method,
        url: Url,
        if_match: Option<&str>,
        body: Option<&Value>,
    ) -> Result<reqwest::Response> {
        let mut request = self
            .client
            .request(method, url)
            .bearer_auth(self.token.expose())
            .header("Prefer", PREFER);
        if let Some(etag) = if_match {
            request = request.header("If-Match", format!("W/\"{etag}\""));
        }
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request
            .send()
            .await
            .map_err(|error| network_error(&error))?;
        match response.status() {
            StatusCode::UNAUTHORIZED => Err(Error::SignInExpired),
            _ => Ok(response),
        }
    }

    async fn get<T: for<'de> Deserialize<'de>>(&self, url: Url) -> Result<T> {
        let response = self.send(Method::GET, url, None, None).await?;
        match response.status().as_u16() {
            200 => response
                .json()
                .await
                .map_err(|error| Error::Protocol(error.to_string())),
            403 => Err(Error::CalendarAccessMissing),
            404 => Err(Error::EventNotFound),
            code => Err(Error::Protocol(format!("Graph answered {code}"))),
        }
    }

    async fn pages<T: for<'de> Deserialize<'de>>(&self, url: Url) -> Result<Vec<T>> {
        let mut items = Vec::new();
        let mut next = Some(url);
        while let Some(url) = next.take() {
            let page: Page<T> = self.get(url).await?;
            items.extend(page.value);
            next = page.next.and_then(|link| Url::parse(&link).ok());
        }
        Ok(items)
    }

    fn url(&self, path: &str) -> Result<Url> {
        self.base.join(path).map_err(|_| Error::InvalidLink)
    }

    pub async fn calendars(&self) -> Result<Vec<GraphCalendar>> {
        let mut url = self.url("me/calendars")?;
        url.query_pairs_mut()
            .append_pair("$select", "id,name,hexColor,canEdit")
            .append_pair("$top", "100");
        self.pages(url).await
    }

    /// The calendar's single events and series masters, with their versions.
    pub async fn list(&self, calendar_id: &str) -> Result<Vec<Listed>> {
        let mut url = self.url(&format!("me/calendars/{calendar_id}/events"))?;
        url.query_pairs_mut()
            .append_pair("$select", "id,changeKey")
            .append_pair("$top", "200");
        self.pages(url).await
    }

    /// The events with these ids; series masters with their changed and cancelled dates. Asked
    /// for in batches of 20 (`$batch`), the most Graph takes at once; gone ones are left out.
    pub async fn events(&self, ids: &[&str]) -> Result<Vec<GraphEvent>> {
        let mut events = Vec::new();
        for chunk in ids.chunks(BATCH) {
            let requests: Vec<Value> = chunk
                .iter()
                .enumerate()
                .map(|(index, id)| {
                    let mut url = Url::parse("https://graph.invalid/").expect("a valid address");
                    url.set_path(&format!("me/events/{id}"));
                    url.query_pairs_mut()
                        .append_pair("$select", &format!("{FIELDS},cancelledOccurrences"))
                        .append_pair(
                            "$expand",
                            &format!("exceptionOccurrences($select={FIELDS})"),
                        );
                    let query = url.query().unwrap_or_default();
                    json!({
                        "id": index.to_string(),
                        "method": "GET",
                        "url": format!("/me/events/{id}?{query}"),
                        "headers": { "Prefer": PREFER },
                    })
                })
                .collect();
            let response = self
                .send(
                    Method::POST,
                    self.url("$batch")?,
                    None,
                    Some(&json!({ "requests": requests })),
                )
                .await?;
            if response.status().as_u16() != 200 {
                return Err(Error::Protocol(format!(
                    "$batch answered {}",
                    response.status().as_u16()
                )));
            }
            let batch: Batch = response
                .json()
                .await
                .map_err(|error| Error::Protocol(error.to_string()))?;
            for answer in batch.responses {
                match answer.status {
                    200 => events.push(
                        serde_json::from_value(answer.body)
                            .map_err(|error| Error::Protocol(error.to_string()))?,
                    ),
                    // Deleted since it was listed: the next sync drops it.
                    404 => {}
                    403 => return Err(Error::CalendarAccessMissing),
                    401 => return Err(Error::SignInExpired),
                    code => return Err(Error::Protocol(format!("Graph answered {code}"))),
                }
            }
        }
        Ok(events)
    }

    pub async fn create(&self, calendar_id: &str, event: &Value) -> Result<Written> {
        let url = self.url(&format!("me/calendars/{calendar_id}/events"))?;
        let response = self.send(Method::POST, url, None, Some(event)).await?;
        match response.status().as_u16() {
            200 | 201 => {
                let created: Listed = response
                    .json()
                    .await
                    .map_err(|error| Error::Protocol(error.to_string()))?;
                Ok(Written::Done(created.id))
            }
            403 => Err(Error::CalendarReadOnly),
            code => Err(Error::Protocol(format!(
                "creating an event answered {code}"
            ))),
        }
    }

    /// Changes an event, only if it is still at `version` (its `changeKey`).
    pub async fn update(
        &self,
        event_id: &str,
        version: Option<&str>,
        event: &Value,
    ) -> Result<Written> {
        let url = self.url(&format!("me/events/{event_id}"))?;
        let response = self.send(Method::PATCH, url, version, Some(event)).await?;
        match response.status().as_u16() {
            200 | 204 => Ok(Written::Done(event_id.to_owned())),
            404 | 409 | 412 => Ok(Written::Conflict),
            403 => Err(Error::CalendarReadOnly),
            code => Err(Error::Protocol(format!(
                "changing an event answered {code}"
            ))),
        }
    }

    /// Deletes an event, a series with all its dates; gone already counts as done.
    pub async fn delete(&self, event_id: &str, version: Option<&str>) -> Result<Written> {
        let url = self.url(&format!("me/events/{event_id}"))?;
        let response = self.send(Method::DELETE, url, version, None).await?;
        match response.status().as_u16() {
            200 | 204 | 404 => Ok(Written::Done(event_id.to_owned())),
            409 | 412 => Ok(Written::Conflict),
            403 => Err(Error::CalendarReadOnly),
            code => Err(Error::Protocol(format!(
                "deleting an event answered {code}"
            ))),
        }
    }

    /// The event in the user's calendars with this iCalendar UID, as an invitation names it.
    pub async fn find_by_uid(&self, uid: &str) -> Result<Option<String>> {
        let mut url = self.url("me/events")?;
        url.query_pairs_mut()
            .append_pair(
                "$filter",
                &format!("iCalUId eq '{}'", uid.replace('\'', "''")),
            )
            .append_pair("$select", "id,changeKey");
        let page: Page<Listed> = self.get(url).await?;
        Ok(page.value.into_iter().next().map(|event| event.id))
    }

    /// Accepts, tentatively accepts or declines an invitation; Exchange tells the organizer.
    pub async fn respond(&self, event_id: &str, action: &str) -> Result<()> {
        let url = self.url(&format!("me/events/{event_id}/{action}"))?;
        let body = json!({ "sendResponse": true });
        let response = self.send(Method::POST, url, None, Some(&body)).await?;
        match response.status().as_u16() {
            200..=299 => Ok(()),
            404 => Err(Error::EventNotFound),
            code => Err(Error::Protocol(format!("answering answered {code}"))),
        }
    }
}

/// The zone an event was made in: IANA or Windows names; the system's when unknown.
fn zone_of(name: Option<&str>) -> chrono_tz::Tz {
    let resolved = name
        .and_then(|name| name.parse::<calcard::common::timezone::Tz>().ok())
        .and_then(|tz| match tz {
            calcard::common::timezone::Tz::Tz(zone) => Some(zone),
            _ => None,
        });
    resolved
        .or_else(|| {
            super::local_time_zone_name().and_then(|name| name.parse::<chrono_tz::Tz>().ok())
        })
        .unwrap_or(chrono_tz::UTC)
}

/// Graph's `2026-11-03T09:00:00.0000000` (asked for in UTC).
fn utc_of(when: &When) -> Option<DateTime<Utc>> {
    let text = when.date_time.trim_end_matches('Z');
    let naive = NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.f").ok()?;
    Some(Utc.from_utc_datetime(&naive))
}

fn utc_of_text(text: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|time| time.with_timezone(&Utc))
        .or_else(|| {
            utc_of(&When {
                date_time: text.to_owned(),
            })
        })
}

fn local(time: DateTime<Utc>, zone: chrono_tz::Tz) -> NaiveDateTime {
    zone.from_utc_datetime(&time.naive_utc()).naive_local()
}

/// `DTSTART` and the like: a date for all-day events, else the local time in the event's zone.
fn time_property(name: &str, time: DateTime<Utc>, all_day: bool, zone: chrono_tz::Tz) -> String {
    if all_day {
        format!("{name};VALUE=DATE:{}", local(time, zone).format("%Y%m%d"))
    } else {
        format!(
            "{name};TZID={}:{}",
            zone.name(),
            local(time, zone).format("%Y%m%dT%H%M%S")
        )
    }
}

const WEEKDAYS: [(&str, &str); 7] = [
    ("monday", "MO"),
    ("tuesday", "TU"),
    ("wednesday", "WE"),
    ("thursday", "TH"),
    ("friday", "FR"),
    ("saturday", "SA"),
    ("sunday", "SU"),
];

fn weekday_code(day: &str) -> Option<&'static str> {
    WEEKDAYS
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(day))
        .map(|(_, code)| *code)
}

/// The `RRULE` for Graph's pattern and range.
fn rrule(
    recurrence: &PatternedRecurrence,
    all_day: bool,
    start: NaiveDateTime,
    zone: chrono_tz::Tz,
) -> Option<String> {
    let pattern = &recurrence.pattern;
    let days = || {
        pattern
            .days_of_week
            .iter()
            .filter_map(|day| weekday_code(day))
            .collect::<Vec<_>>()
            .join(",")
    };
    let position = match pattern.index.as_deref().unwrap_or("first") {
        "second" => 2,
        "third" => 3,
        "fourth" => 4,
        "last" => -1,
        _ => 1,
    };
    let mut parts = vec![match pattern.kind.as_str() {
        "daily" => "FREQ=DAILY".to_owned(),
        "weekly" => format!(
            "FREQ=WEEKLY;BYDAY={};WKST={}",
            days(),
            pattern
                .first_day_of_week
                .as_deref()
                .and_then(weekday_code)
                .unwrap_or("SU")
        ),
        "absoluteMonthly" => format!(
            "FREQ=MONTHLY;BYMONTHDAY={}",
            pattern
                .day_of_month
                .unwrap_or(start.date().format("%d").to_string().parse().ok()?)
        ),
        "relativeMonthly" => format!("FREQ=MONTHLY;BYDAY={};BYSETPOS={position}", days()),
        "absoluteYearly" => format!(
            "FREQ=YEARLY;BYMONTH={};BYMONTHDAY={}",
            pattern.month?, pattern.day_of_month?
        ),
        "relativeYearly" => format!(
            "FREQ=YEARLY;BYMONTH={};BYDAY={};BYSETPOS={position}",
            pattern.month?,
            days()
        ),
        _ => return None,
    }];
    if pattern.interval > 1 {
        parts.push(format!("INTERVAL={}", pattern.interval));
    }
    let range = &recurrence.range;
    match range.kind.as_str() {
        "numbered" => parts.push(format!("COUNT={}", range.number_of_occurrences?)),
        "endDate" => {
            let last = NaiveDate::parse_from_str(range.end_date.as_deref()?, "%Y-%m-%d").ok()?;
            if all_day {
                parts.push(format!("UNTIL={}", last.format("%Y%m%d")));
            } else {
                // The end of that day where the event takes place.
                let end = zone
                    .from_local_datetime(&last.and_hms_opt(23, 59, 59)?)
                    .earliest()?
                    .with_timezone(&Utc);
                parts.push(format!("UNTIL={}", end.format("%Y%m%dT%H%M%SZ")));
            }
        }
        _ => {}
    }
    Some(format!("RRULE:{}", parts.join(";")))
}

fn person(address: &EmailAddress) -> Option<(String, Option<String>)> {
    let mail = address.address.as_deref()?.trim();
    if !mail.contains('@') {
        return None;
    }
    let name = address
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case(mail));
    Some((mail.to_owned(), name.map(str::to_owned)))
}

fn cn(name: Option<&str>) -> String {
    name.map(|name| format!(";CN=\"{}\"", name.replace('"', "'")))
        .unwrap_or_default()
}

/// The lines of one VEVENT: the master or single event, or a changed date of a series.
fn event_lines(
    event: &GraphEvent,
    uid: &str,
    zone: chrono_tz::Tz,
    master: Option<(bool, chrono_tz::Tz)>,
) -> Option<Vec<String>> {
    let all_day = event.is_all_day;
    let start = utc_of(event.start.as_ref()?)?;
    let end = event.end.as_ref().and_then(utc_of).unwrap_or(start);
    let mut lines = vec![
        "BEGIN:VEVENT".to_owned(),
        format!("UID:{}", ical::escape(uid)),
        format!("DTSTAMP:{}", Utc::now().format("%Y%m%dT%H%M%SZ")),
    ];
    if let Some((master_all_day, master_zone)) = master {
        // Which date of the series this one replaces.
        let original = event.original_start.as_deref().and_then(utc_of_text)?;
        lines.push(time_property(
            "RECURRENCE-ID",
            original,
            master_all_day,
            master_zone,
        ));
    }
    lines.push(time_property("DTSTART", start, all_day, zone));
    lines.push(time_property("DTEND", end, all_day, zone));
    for (name, value) in [
        ("SUMMARY", event.subject.as_deref()),
        (
            "LOCATION",
            event
                .location
                .as_ref()
                .and_then(|l| l.display_name.as_deref()),
        ),
        (
            "DESCRIPTION",
            event.body.as_ref().and_then(|b| b.content.as_deref()),
        ),
    ] {
        if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
            lines.push(format!("{name}:{}", ical::escape(value)));
        }
    }
    if let Some((mail, name)) = event
        .organizer
        .as_ref()
        .and_then(|o| o.email_address.as_ref())
        .and_then(person)
    {
        lines.push(format!("ORGANIZER{}:mailto:{mail}", cn(name.as_deref())));
    }
    for attendee in &event.attendees {
        let Some((mail, name)) = attendee.email_address.as_ref().and_then(person) else {
            continue;
        };
        let status = match attendee.status.as_ref().and_then(|s| s.response.as_deref()) {
            Some("accepted" | "organizer") => "ACCEPTED",
            Some("tentativelyAccepted") => "TENTATIVE",
            Some("declined") => "DECLINED",
            _ => "NEEDS-ACTION",
        };
        let role = if attendee.kind.as_deref() == Some("optional") {
            "OPT-PARTICIPANT"
        } else {
            "REQ-PARTICIPANT"
        };
        lines.push(format!(
            "ATTENDEE;ROLE={role};PARTSTAT={status}{}:mailto:{mail}",
            cn(name.as_deref())
        ));
    }
    if master.is_none()
        && let Some(recurrence) = &event.recurrence
        && let Some(rule) = rrule(recurrence, all_day, local(start, zone), zone)
    {
        lines.push(rule);
        // Cancelled dates: `OID.<master id>.<date>`, at the series' time of day.
        for cancelled in &event.cancelled_occurrences {
            let Some(date) = cancelled
                .rsplit('.')
                .next()
                .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
            else {
                continue;
            };
            if all_day {
                lines.push(format!("EXDATE;VALUE=DATE:{}", date.format("%Y%m%d")));
            } else {
                let at = date.and_time(local(start, zone).time());
                lines.push(format!(
                    "EXDATE;TZID={}:{}",
                    zone.name(),
                    at.format("%Y%m%dT%H%M%S")
                ));
            }
        }
    }
    lines.push("END:VEVENT".to_owned());
    Some(lines)
}

/// An event from Graph as iCalendar: the master or single event, then a series' changed dates.
pub(crate) fn to_ical(event: &GraphEvent) -> Option<String> {
    let uid = event.ical_uid.as_deref().unwrap_or(&event.id);
    let zone = zone_of(event.original_start_time_zone.as_deref());
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_owned(),
        "VERSION:2.0".to_owned(),
        format!("PRODID:{PRODUCT_ID}"),
    ];
    lines.extend(event_lines(event, uid, zone, None)?);
    for exception in &event.exception_occurrences {
        if exception.is_cancelled {
            continue;
        }
        let exception_zone = exception
            .original_start_time_zone
            .as_deref()
            .map_or(zone, |name| zone_of(Some(name)));
        if let Some(more) = event_lines(
            exception,
            uid,
            exception_zone,
            Some((event.is_all_day, zone)),
        ) {
            lines.extend(more);
        }
    }
    lines.extend(["END:VCALENDAR".to_owned(), String::new()]);
    let mut calendar = ICalendar::parse(lines.join("\r\n")).ok()?;
    calendar.add_missing_timezones();
    Some(calendar.to_string())
}

/// What Graph takes to create or change an event: from its iCalendar text, as the event dialog
/// left it. Times go in the system's zone. A series' rule goes along where Graph can say it;
/// one it cannot (certain weekdays of a month, say) is left as it is on the server.
pub(crate) fn to_graph(ics: &str, zone_name: Option<&str>, own: Option<&str>) -> Option<Value> {
    let calendar = ICalendar::parse(ics).ok()?;
    let index = calendar.components.iter().position(|component| {
        component.component_type == calcard::icalendar::ICalendarComponentType::VEvent
            && !component.has_property(&ICalendarProperty::RecurrenceId)
    })?;
    let event = &calendar.components[index];
    let dates = ical::first_dates(&calendar, index)?;
    let zone = zone_name
        .and_then(|name| name.parse::<chrono_tz::Tz>().ok())
        .unwrap_or(chrono_tz::UTC);
    let when = |millis: i64| -> Option<Value> {
        let utc = DateTime::<Utc>::from_timestamp_millis(millis)?;
        let text = if dates.all_day {
            // All-day dates are midnight UTC (see `Occurrence`): midnight of that day there.
            utc.naive_utc().format("%Y-%m-%dT00:00:00").to_string()
        } else {
            local(utc, zone).format("%Y-%m-%dT%H:%M:%S").to_string()
        };
        Some(json!({ "dateTime": text, "timeZone": zone.name() }))
    };
    let text = |property: &ICalendarProperty| ical::text(event, property).unwrap_or_default();
    let mut body = json!({
        "subject": text(&ICalendarProperty::Summary),
        "body": { "contentType": "text", "content": text(&ICalendarProperty::Description) },
        "location": { "displayName": text(&ICalendarProperty::Location) },
        "isAllDay": dates.all_day,
        "start": when(dates.start)?,
        "end": when(dates.end)?,
    });
    // Attendees only for the account's own events: Exchange invites them.
    let organizer = event
        .property(&ICalendarProperty::Organizer)
        .and_then(ical::person);
    if organizer
        .is_none_or(|organizer| own.is_some_and(|own| organizer.address.eq_ignore_ascii_case(own)))
    {
        let attendees: Vec<Value> = ical::attendees(event)
            .into_iter()
            .map(|attendee| {
                json!({
                    "emailAddress": {
                        "address": attendee.address.address,
                        "name": attendee.address.name.unwrap_or_default(),
                    },
                    "type": if attendee.optional { "optional" } else { "required" },
                })
            })
            .collect();
        body["attendees"] = json!(attendees);
    }
    match ical::recurrence(event) {
        None => body["recurrence"] = Value::Null,
        Some(rule) => {
            let start = DateTime::<Utc>::from_timestamp_millis(dates.start)?;
            let first = if dates.all_day {
                start.naive_utc().date()
            } else {
                local(start, zone).date()
            };
            let day_name = |day: u8| WEEKDAYS.get(usize::from(day)).map(|(name, _)| *name);
            let start_weekday = day_name(
                u8::try_from(first.format("%u").to_string().parse::<u32>().ok()? - 1).ok()?,
            )?;
            let pattern = match (rule.frequency, rule.weekdays.is_empty()) {
                (Frequency::Daily, true) => json!({ "type": "daily" }),
                (Frequency::Weekly, _) => {
                    let days: Vec<&str> = if rule.weekdays.is_empty() {
                        vec![start_weekday]
                    } else {
                        rule.weekdays
                            .iter()
                            .filter_map(|day| day_name(*day))
                            .collect()
                    };
                    json!({ "type": "weekly", "daysOfWeek": days, "firstDayOfWeek": "monday" })
                }
                (Frequency::Monthly, true) => {
                    json!({ "type": "absoluteMonthly", "dayOfMonth": first.format("%-d").to_string().parse::<u32>().ok()? })
                }
                (Frequency::Yearly, true) => json!({
                    "type": "absoluteYearly",
                    "dayOfMonth": first.format("%-d").to_string().parse::<u32>().ok()?,
                    "month": first.format("%-m").to_string().parse::<u32>().ok()?,
                }),
                // Kept as it is on the server.
                _ => return Some(body),
            };
            let mut pattern = pattern;
            pattern["interval"] = json!(rule.interval);
            let range = match (rule.count, rule.until) {
                (Some(count), _) => json!({
                    "type": "numbered",
                    "startDate": first.to_string(),
                    "numberOfOccurrences": count,
                }),
                (None, Some(until)) => {
                    let last = DateTime::<Utc>::from_timestamp_millis(until)?;
                    json!({
                        "type": "endDate",
                        "startDate": first.to_string(),
                        "endDate": local(last, zone).date().to_string(),
                    })
                }
                (None, None) => json!({ "type": "noEnd", "startDate": first.to_string() }),
            };
            body["recurrence"] = json!({ "pattern": pattern, "range": range });
        }
    }
    Some(body)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::calendar::ical::dates_between;

    const DAY: i64 = 86_400_000;
    const HOUR: i64 = 3_600_000;

    /// A weekly series made in Berlin, Tuesdays 10:00, from 20 October 2026 (summer time) on,
    /// with the 3 November moved to 11:00 and the 10 November cancelled.
    pub(crate) fn series() -> GraphEvent {
        serde_json::from_value(series_json()).unwrap()
    }

    pub(crate) fn series_json() -> Value {
        json!({
            "id": "AAMk-master",
            "iCalUId": "040000008200E00074C5B7101A82E0080000000",
            "subject": "Jour fixe",
            "body": { "contentType": "text", "content": "Agenda folgt" },
            "start": { "dateTime": "2026-10-20T08:00:00.0000000", "timeZone": "UTC" },
            "end": { "dateTime": "2026-10-20T08:30:00.0000000", "timeZone": "UTC" },
            "isAllDay": false,
            "location": { "displayName": "Raum 2.14" },
            "organizer": { "emailAddress": { "name": "Jonas Weber", "address": "jonas@example.com" } },
            "attendees": [
                { "emailAddress": { "name": "Alex", "address": "alex@example.com" },
                  "status": { "response": "accepted" }, "type": "required" }
            ],
            "originalStartTimeZone": "W. Europe Standard Time",
            "changeKey": "ck1",
            "type": "seriesMaster",
            "recurrence": {
                "pattern": { "type": "weekly", "interval": 1, "daysOfWeek": ["tuesday"], "firstDayOfWeek": "monday" },
                "range": { "type": "noEnd", "startDate": "2026-10-20" }
            },
            "cancelledOccurrences": ["OID.AAMk-master.2026-11-10"],
            "exceptionOccurrences": [{
                "id": "AAMk-exception",
                "subject": "Jour fixe (später)",
                "start": { "dateTime": "2026-11-03T10:00:00.0000000", "timeZone": "UTC" },
                "end": { "dateTime": "2026-11-03T10:30:00.0000000", "timeZone": "UTC" },
                "originalStart": "2026-11-03T09:00:00Z",
                "originalStartTimeZone": "W. Europe Standard Time",
                "type": "exception"
            }]
        })
    }

    #[test]
    fn a_series_keeps_its_local_time_its_changed_and_cancelled_dates() {
        let ics = to_ical(&series()).unwrap();
        assert!(ics.contains("RRULE:FREQ=WEEKLY;BYDAY=TU;WKST=MO"), "{ics}");
        assert!(
            ics.contains("DTSTART;TZID=Europe/Berlin:20261020T100000"),
            "{ics}"
        );
        let calendar = ICalendar::parse(&ics).unwrap();
        // 20 October 2026, 08:00 UTC, for four weeks.
        let first = 1_792_483_200_000;
        let dates = dates_between(&calendar, first, first + 28 * DAY);
        let starts: Vec<i64> = dates.iter().map(|dates| dates.start).collect();
        assert_eq!(
            starts,
            [
                first,
                // Winter time from 25 October: 10:00 is 09:00 UTC.
                first + 7 * DAY + HOUR,
                // Moved to 11:00 local.
                first + 14 * DAY + 2 * HOUR,
                // The 10 November is cancelled.
            ]
        );
        assert!(ics.contains("SUMMARY:Jour fixe (später)"), "{ics}");
        assert!(
            ics.contains("ORGANIZER;CN=Jonas Weber:mailto:jonas@example.com")
                || ics.contains("ORGANIZER;CN=\"Jonas Weber\":mailto:jonas@example.com"),
            "{ics}"
        );
    }

    #[test]
    fn all_day_events_stay_on_their_dates() {
        let event: GraphEvent = serde_json::from_value(json!({
            "id": "a",
            "subject": "Messe",
            "start": { "dateTime": "2026-11-11T23:00:00.0000000", "timeZone": "UTC" },
            "end": { "dateTime": "2026-11-13T23:00:00.0000000", "timeZone": "UTC" },
            "isAllDay": true,
            "originalStartTimeZone": "Europe/Berlin",
        }))
        .unwrap();
        let ics = to_ical(&event).unwrap();
        assert!(ics.contains("DTSTART;VALUE=DATE:20261112"), "{ics}");
        assert!(ics.contains("DTEND;VALUE=DATE:20261114"), "{ics}");
    }

    #[test]
    fn events_go_back_with_their_rule_where_graph_can_say_it() {
        let ics = to_ical(&series()).unwrap();
        let body = to_graph(&ics, Some("Europe/Berlin"), None).unwrap();
        assert_eq!(body["subject"], "Jour fixe");
        assert_eq!(body["start"]["dateTime"], "2026-10-20T10:00:00");
        assert_eq!(body["start"]["timeZone"], "Europe/Berlin");
        assert_eq!(body["recurrence"]["pattern"]["type"], "weekly");
        assert_eq!(
            body["recurrence"]["pattern"]["daysOfWeek"],
            json!(["tuesday"])
        );
        assert_eq!(body["recurrence"]["range"]["type"], "noEnd");

        let single = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:x\r\nDTSTART;VALUE=DATE:20261112\r\n\
DTEND;VALUE=DATE:20261113\r\nSUMMARY:Feiertag\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let body = to_graph(single, Some("Europe/Berlin"), None).unwrap();
        assert_eq!(body["isAllDay"], true);
        assert_eq!(body["start"]["dateTime"], "2026-11-12T00:00:00");
        assert_eq!(body["end"]["dateTime"], "2026-11-13T00:00:00");
        assert!(body["recurrence"].is_null());
    }
}
