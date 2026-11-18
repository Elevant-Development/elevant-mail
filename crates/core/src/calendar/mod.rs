//! Calendars: the invitations that come by mail ([`invitation`]) and the calendars with their
//! events in the cache ([`events`]). The sources that fill the cache (CalDAV, Microsoft Graph)
//! follow.

pub(crate) mod caldav;
#[cfg(test)]
pub(crate) mod caldav_test_server;
pub mod events;
pub(crate) mod graph;
#[cfg(test)]
pub(crate) mod graph_test_server;
pub(crate) mod ical;
pub mod invitation;
pub mod sync;
pub(crate) mod xml;

use calcard::common::timezone::Tz;
use jiff::tz::{Offset, TimeZone};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::Address;

pub(crate) const PRODUCT_ID: &str = "-//Elevant//Elevant Mail//EN";

/// The system's time zone as `calcard` takes it, for times given without one ("floating"); UTC
/// if it is unknown.
pub(crate) fn local_time_zone() -> Tz {
    local_time_zone_name()
        .and_then(|name| name.parse::<Tz>().ok())
        .filter(|zone| matches!(zone, Tz::Tz(_)))
        .unwrap_or(Tz::UTC)
}

/// A time zone by its IANA name ("Europe/Berlin"), for reckoning with dates.
pub(crate) fn zone_named(name: &str) -> Option<TimeZone> {
    TimeZone::get(name).ok()
}

/// The system's time zone for reckoning with dates, if known.
pub(crate) fn local_zone() -> Option<TimeZone> {
    local_time_zone_name().and_then(|name| zone_named(&name))
}

/// The IANA name a zone goes by in `TZID` and towards Graph; "UTC" for one without.
pub(crate) fn zone_name(zone: &TimeZone) -> &str {
    zone.iana_name().unwrap_or("UTC")
}

/// A zone `calcard` resolved (from a `TZID`, also a Windows name), for reckoning with dates. A
/// floating one is the system's.
pub(crate) fn zone_of_calcard(zone: Tz) -> TimeZone {
    match zone {
        Tz::Tz(zone) => zone_named(zone.name()),
        Tz::Fixed(offset) => Offset::from_seconds(offset.local_minus_utc())
            .ok()
            .map(TimeZone::fixed),
        Tz::Floating => local_zone(),
    }
    .unwrap_or(TimeZone::UTC)
}

/// The system's time zone by its IANA name ("Europe/Berlin"), if known.
pub(crate) fn local_time_zone_name() -> Option<String> {
    iana_time_zone::get_timezone().ok()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Attendee {
    pub address: Address,
    pub status: Participation,
    /// Invited as optional.
    pub optional: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Participation {
    NeedsAction,
    Accepted,
    Tentative,
    Declined,
    Delegated,
}

/// How a series repeats, as far as the interface says it in words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Recurrence {
    pub frequency: Frequency,
    /// Every n-th day, week, month or year.
    pub interval: u16,
    /// Days of the week, 0 Monday to 6 Sunday.
    pub weekdays: Vec<u8>,
    /// Ends after this many dates.
    pub count: Option<u32>,
    /// Ends on this day (Unix milliseconds).
    #[ts(type = "number | null")]
    pub until: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Frequency {
    Daily,
    Weekly,
    Monthly,
    Yearly,
    /// Hourly and finer: rare in calendars, shown without details.
    Other,
}

impl Frequency {
    pub(crate) fn as_rrule(self) -> Option<&'static str> {
        Some(match self {
            Frequency::Daily => "DAILY",
            Frequency::Weekly => "WEEKLY",
            Frequency::Monthly => "MONTHLY",
            Frequency::Yearly => "YEARLY",
            Frequency::Other => return None,
        })
    }
}
