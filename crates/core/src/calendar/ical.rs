//! Reading and writing iCalendar (RFC 5545) with `calcard`: the properties the app shows, the
//! dates of an event or series, and the text form of new events.

use calcard::common::PartialDateTime;
use calcard::icalendar::{
    ICalendar, ICalendarComponent, ICalendarEntry, ICalendarFrequency, ICalendarParameter,
    ICalendarParameterName, ICalendarParameterValue, ICalendarParticipationRole,
    ICalendarParticipationStatus, ICalendarProperty, ICalendarValue, ICalendarWeekday,
};
use chrono::NaiveDate;

use super::{Attendee, Frequency, Participation, Recurrence};
use crate::model::Address;

const DAY_MS: i64 = 86_400_000;
/// Dates of a series looked at, at most; a daily series reaches back about 27 years.
const EXPAND_LIMIT: usize = 10_000;

pub(crate) fn text(component: &ICalendarComponent, property: &ICalendarProperty) -> Option<String> {
    component
        .property(property)
        .and_then(|entry| entry.values.first())
        .and_then(ICalendarValue::as_text)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

pub(crate) fn param<'a>(
    entry: &'a ICalendarEntry,
    name: &ICalendarParameterName,
) -> Option<&'a ICalendarParameterValue> {
    entry
        .params
        .iter()
        .find(|param| &param.name == name)
        .map(|param| &param.value)
}

/// An organizer or attendee: the address from `mailto:`, the name from `CN`.
pub(crate) fn person(entry: &ICalendarEntry) -> Option<Address> {
    let value = entry.values.first()?.as_text()?.trim();
    let address = match value.get(..7) {
        Some(scheme) if scheme.eq_ignore_ascii_case("mailto:") => &value[7..],
        _ => value,
    }
    .trim();
    if !address.contains('@') {
        return None;
    }
    let name = param(entry, &ICalendarParameterName::Cn)
        .and_then(ICalendarParameterValue::as_text)
        .map(|name| name.trim().trim_matches('"').trim())
        .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case(address))
        .map(str::to_owned);
    Some(Address {
        name,
        address: address.to_owned(),
    })
}

pub(crate) fn attendees(event: &ICalendarComponent) -> Vec<Attendee> {
    event
        .properties(&ICalendarProperty::Attendee)
        .filter_map(|entry| {
            Some(Attendee {
                address: person(entry)?,
                status: match param(entry, &ICalendarParameterName::Partstat) {
                    Some(ICalendarParameterValue::Partstat(status)) => match status {
                        ICalendarParticipationStatus::Accepted => Participation::Accepted,
                        ICalendarParticipationStatus::Tentative => Participation::Tentative,
                        ICalendarParticipationStatus::Declined => Participation::Declined,
                        ICalendarParticipationStatus::Delegated => Participation::Delegated,
                        _ => Participation::NeedsAction,
                    },
                    _ => Participation::NeedsAction,
                },
                optional: matches!(
                    param(entry, &ICalendarParameterName::Role),
                    Some(ICalendarParameterValue::Role(
                        ICalendarParticipationRole::OptParticipant
                    ))
                ),
            })
        })
        .collect()
}

pub(crate) fn recurrence(event: &ICalendarComponent) -> Option<Recurrence> {
    let rule = match event.property(&ICalendarProperty::Rrule)?.values.first()? {
        ICalendarValue::RecurrenceRule(rule) => rule,
        _ => return None,
    };
    Some(Recurrence {
        frequency: match rule.freq {
            ICalendarFrequency::Daily => Frequency::Daily,
            ICalendarFrequency::Weekly => Frequency::Weekly,
            ICalendarFrequency::Monthly => Frequency::Monthly,
            ICalendarFrequency::Yearly => Frequency::Yearly,
            _ => Frequency::Other,
        },
        interval: rule.interval.unwrap_or(1).max(1),
        weekdays: rule
            .byday
            .iter()
            .map(|day| match day.weekday {
                ICalendarWeekday::Monday => 0,
                ICalendarWeekday::Tuesday => 1,
                ICalendarWeekday::Wednesday => 2,
                ICalendarWeekday::Thursday => 3,
                ICalendarWeekday::Friday => 4,
                ICalendarWeekday::Saturday => 5,
                ICalendarWeekday::Sunday => 6,
            })
            .collect(),
        count: rule.count,
        until: rule
            .until
            .as_ref()
            .and_then(PartialDateTime::to_timestamp)
            .map(|seconds| seconds * 1000),
    })
}

/// The date of an all-day event's `DTSTART`, `None` for an event with a time.
fn start_date(event: &ICalendarComponent) -> Option<NaiveDate> {
    let date = event
        .property(&ICalendarProperty::Dtstart)?
        .values
        .first()?
        .as_partial_date_time()?;
    if date.hour.is_some() {
        return None;
    }
    NaiveDate::from_ymd_opt(
        i32::from(date.year?),
        u32::from(date.month?),
        u32::from(date.day?),
    )
}

/// One date of an event: its component in the calendar, start and end in Unix milliseconds,
/// and whether it lasts whole days (then at midnight UTC of its days, end exclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Dates {
    pub component: usize,
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
}

/// The dates of the events in `calendar` that overlap `from..to` (Unix milliseconds), for a
/// series each date, with the changed dates of a series as their own components. Time zones
/// come from the calendar (also Windows names like "W. Europe Standard Time"); times without one
/// are the system's local time.
pub(crate) fn dates_between(calendar: &ICalendar, from: i64, to: i64) -> Vec<Dates> {
    let local = super::local_time_zone();
    let calendar = &utc_series_named(calendar);
    calendar
        .expand_dates(local, EXPAND_LIMIT)
        .events
        .iter()
        .filter_map(|occurrence| {
            let component = occurrence.comp_id as usize;
            let (start, end) = occurrence.timestamps();
            let dates = match start_date(calendar.components.get(component)?) {
                Some(_) => {
                    // The occurrence's own day: a series moves on from its first one.
                    let day = occurrence.start.naive_local().date();
                    // Rounded: a day with a clock change has 23 or 25 hours.
                    let days = ((end - start + 43_200) / 86_400).max(1);
                    let start = day.and_hms_opt(0, 0, 0)?.and_utc().timestamp() * 1000;
                    Dates {
                        component,
                        start,
                        end: start + days * DAY_MS,
                        all_day: true,
                    }
                }
                None => Dates {
                    component,
                    start: start * 1000,
                    end: end.max(start) * 1000,
                    all_day: false,
                },
            };
            (dates.start < to && dates.end.max(dates.start + 1) > from).then_some(dates)
        })
        .collect()
}

/// The calendar with the start of each series given in UTC (`DTSTART:…Z`, as Google writes it)
/// named `TZID=UTC` instead: `calcard` (0.3) expands such a series in the default time zone, which
/// would move every date by the local offset. Single events and named zones are not affected.
fn utc_series_named(calendar: &ICalendar) -> ICalendar {
    let mut calendar = calendar.clone();
    for component in &mut calendar.components {
        if !component.has_property(&ICalendarProperty::Rrule) {
            continue;
        }
        let Some(start) = component.property_mut(&ICalendarProperty::Dtstart) else {
            continue;
        };
        if param(start, &ICalendarParameterName::Tzid).is_some() {
            continue;
        }
        if let Some(ICalendarValue::PartialDateTime(date)) = start.values.first_mut()
            && date.hour.is_some()
            && (date.tz_hour, date.tz_minute) == (Some(0), Some(0))
        {
            date.tz_hour = None;
            date.tz_minute = None;
            start
                .params
                .push(ICalendarParameter::tzid("UTC".to_owned()));
        }
    }
    calendar
}

/// The first date of the event at `index`.
pub(crate) fn first_dates(calendar: &ICalendar, index: usize) -> Option<Dates> {
    dates_between(calendar, i64::MIN, i64::MAX)
        .into_iter()
        .filter(|dates| dates.component == index)
        .min_by_key(|dates| dates.start)
}

/// Start, end and whether it lasts whole days, of the first date of the event at `index`.
pub(crate) fn first_occurrence(calendar: &ICalendar, index: usize) -> Option<(i64, i64, bool)> {
    first_dates(calendar, index).map(|dates| (dates.start, dates.end, dates.all_day))
}

/// A TEXT value as iCalendar writes it: backslash, semicolon, comma and line breaks escaped.
pub(crate) fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            ';' => escaped.push_str("\\;"),
            ',' => escaped.push_str("\\,"),
            '\n' => escaped.push_str("\\n"),
            '\r' => {}
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_daily_series_gives_the_dates_in_range() {
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:a\r\n\
DTSTART:20261102T080000Z\r\nDTEND:20261102T083000Z\r\nRRULE:FREQ=DAILY\r\n\
END:VEVENT\r\nEND:VCALENDAR\r\n";
        let calendar = ICalendar::parse(ics).unwrap();
        // 4 November 2026, 00:00 UTC, for three days.
        let from = 1_793_750_400_000;
        let dates = dates_between(&calendar, from, from + 3 * DAY_MS);
        assert_eq!(dates.len(), 3);
        assert_eq!(dates[0].start, from + 8 * 3_600_000);
        assert_eq!(dates[0].end - dates[0].start, 1_800_000);
    }

    #[test]
    fn escapes_text() {
        assert_eq!(escape("a;b,c\\d\r\ne"), "a\\;b\\,c\\\\d\\ne");
    }
}
