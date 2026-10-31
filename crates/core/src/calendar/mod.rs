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
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::Address;

pub(crate) const PRODUCT_ID: &str = "-//Elevant//Elevant Mail//EN";

/// The system's time zone, for times given without one ("floating"); UTC if it is unknown.
pub(crate) fn local_time_zone() -> Tz {
    local_time_zone_name()
        .and_then(|name| name.parse::<chrono_tz::Tz>().ok())
        .map_or(Tz::UTC, Tz::from)
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
