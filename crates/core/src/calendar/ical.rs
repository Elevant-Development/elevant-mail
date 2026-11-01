//! Reading and writing iCalendar (RFC 5545) with `calcard`: the properties the app shows, the
//! dates of an event or series, and the text form of new events.

use calcard::common::PartialDateTime;
use calcard::icalendar::{
    ICalendar, ICalendarComponent, ICalendarEntry, ICalendarFrequency, ICalendarParameter,
    ICalendarParameterName, ICalendarParameterValue, ICalendarParticipationRole,
    ICalendarParticipationStatus, ICalendarProperty, ICalendarValue, ICalendarWeekday,
};
use chrono::{Datelike, NaiveDate, Timelike};

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
    /// For a date of a series: its original start (`RECURRENCE-ID`), which stays the same when
    /// that date is moved; all-day dates at midnight UTC. `None` for a single event.
    pub instance: Option<i64>,
}

/// The dates of the events in `calendar` that overlap `from..to` (Unix milliseconds), for a
/// series each date, with the changed dates of a series as their own components. Time zones
/// come from the calendar (also Windows names like "W. Europe Standard Time"); times without one
/// are the system's local time.
pub(crate) fn dates_between(calendar: &ICalendar, from: i64, to: i64) -> Vec<Dates> {
    let local = super::local_time_zone();
    let calendar = &ready_to_expand(calendar, to);
    let series = calendar
        .components
        .iter()
        .any(|component| component.has_property(&ICalendarProperty::Rrule));
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
                        instance: None,
                    }
                }
                None => Dates {
                    component,
                    start: start * 1000,
                    end: end.max(start) * 1000,
                    all_day: false,
                    instance: None,
                },
            };
            let event = calendar.components.get(component)?;
            let instance = match event.property(&ICalendarProperty::RecurrenceId) {
                Some(entry) => instant_of(calendar, entry),
                None if series && event.has_property(&ICalendarProperty::Rrule) => {
                    Some(dates.start)
                }
                None => None,
            };
            let dates = Dates { instance, ..dates };
            (dates.start < to && dates.end.max(dates.start + 1) > from).then_some(dates)
        })
        .collect()
}

/// The moment a date-time property names, in Unix milliseconds: in its `TZID`, in UTC with `Z`,
/// else in the system's zone; a date alone at midnight UTC.
pub(crate) fn instant_of(calendar: &ICalendar, entry: &ICalendarEntry) -> Option<i64> {
    let date = entry.values.first()?.as_partial_date_time()?;
    if date.hour.is_none() {
        let day = NaiveDate::from_ymd_opt(
            i32::from(date.year?),
            u32::from(date.month?),
            u32::from(date.day?),
        )?;
        return Some(day.and_hms_opt(0, 0, 0)?.and_utc().timestamp() * 1000);
    }
    let zone = calendar
        .build_tz_resolver()
        .with_default(super::local_time_zone())
        .resolve_or_default(entry.tz_id());
    Some(date.to_date_time_with_tz(zone)?.timestamp() * 1000)
}

/// `RECURRENCE-ID` or `EXDATE` for the date of the series at `index` that starts at `instance`,
/// written the way the series writes its `DTSTART`, as servers compare them.
pub(crate) fn instance_property(
    calendar: &ICalendar,
    index: usize,
    name: &str,
    instance: i64,
) -> Option<String> {
    let start = calendar
        .components
        .get(index)?
        .property(&ICalendarProperty::Dtstart)?;
    let date = start.values.first()?.as_partial_date_time()?;
    let utc = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(instance)?;
    if date.hour.is_none() {
        return Some(format!("{name};VALUE=DATE:{}", utc.format("%Y%m%d")));
    }
    if let Some(tz_id) = start.tz_id() {
        let zone = calendar
            .build_tz_resolver()
            .with_default(super::local_time_zone())
            .resolve_or_default(Some(tz_id));
        let local = utc.with_timezone(&zone).naive_local();
        return Some(format!(
            "{name};TZID={tz_id}:{}",
            local.format("%Y%m%dT%H%M%S")
        ));
    }
    if (date.tz_hour, date.tz_minute) == (Some(0), Some(0)) {
        return Some(format!("{name}:{}", utc.format("%Y%m%dT%H%M%SZ")));
    }
    // Floating: the system's local time.
    let local = utc.with_timezone(&super::local_time_zone()).naive_local();
    Some(format!("{name}:{}", local.format("%Y%m%dT%H%M%S")))
}

/// The calendar as `calcard` (0.3) expands it right:
/// - The start of each series given in UTC (`DTSTART:…Z`, as Google writes it) is named
///   `TZID=UTC` instead: `calcard` expands such a series in the default time zone, which would
///   move every date by the local offset. Single events and named zones are not affected.
/// - Without `SEQUENCE`: `calcard` puts a changed date in place of the series' date only when
///   both have the same one, and servers count them apart.
/// - Each rule ends a day after `until` (Unix milliseconds) at the latest: `calcard` expands a
///   series from its start up to its limit whatever range is asked for, which took seconds for a
///   few series without end.
fn ready_to_expand(calendar: &ICalendar, until: i64) -> ICalendar {
    let mut calendar = calendar.clone();
    // Compared as wall-clock time, as `calcard` does; the day covers any time zone.
    let cap = until
        .checked_add(DAY_MS)
        .and_then(chrono::DateTime::<chrono::Utc>::from_timestamp_millis)
        .map(|cap| cap.naive_utc());
    for component in &mut calendar.components {
        component
            .entries
            .retain(|entry| entry.name != ICalendarProperty::Sequence);
        if !component.has_property(&ICalendarProperty::Rrule) {
            continue;
        }
        let start = component
            .property(&ICalendarProperty::Dtstart)
            .and_then(|entry| entry.values.first())
            .and_then(ICalendarValue::as_partial_date_time)
            .and_then(wall_clock);
        if let (Some(cap), Some(start)) = (cap, start)
            && let Some(ICalendarValue::RecurrenceRule(rule)) = component
                .property_mut(&ICalendarProperty::Rrule)
                .and_then(|entry| entry.values.first_mut())
        {
            // Never before the start: `calcard` drops such a rule.
            let cap = cap.max(start);
            if rule
                .until
                .as_ref()
                .and_then(wall_clock)
                .is_none_or(|end| end > cap)
            {
                rule.until = Some(PartialDateTime {
                    year: u16::try_from(cap.year()).ok(),
                    month: u8::try_from(cap.month()).ok(),
                    day: u8::try_from(cap.day()).ok(),
                    hour: u8::try_from(cap.hour()).ok(),
                    minute: u8::try_from(cap.minute()).ok(),
                    second: u8::try_from(cap.second()).ok(),
                    ..PartialDateTime::default()
                });
            }
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

/// The date and time of a value as written, without its zone.
fn wall_clock(date: &PartialDateTime) -> Option<chrono::NaiveDateTime> {
    NaiveDate::from_ymd_opt(
        i32::from(date.year?),
        u32::from(date.month?),
        u32::from(date.day?),
    )?
    .and_hms_opt(
        u32::from(date.hour.unwrap_or(0)),
        u32::from(date.minute.unwrap_or(0)),
        u32::from(date.second.unwrap_or(0)),
    )
}

/// How far after its start a series is expanded to find its first date: a few first dates may
/// be left out (`EXDATE`).
const FIRST_DATE_REACH_MS: i64 = 400 * DAY_MS;

/// The first date of the event at `index`.
pub(crate) fn first_dates(calendar: &ICalendar, index: usize) -> Option<Dates> {
    let start = calendar
        .components
        .get(index)?
        .property(&ICalendarProperty::Dtstart)
        .and_then(|entry| instant_of(calendar, entry));
    let until = start.map_or(i64::MAX, |start| start.saturating_add(FIRST_DATE_REACH_MS));
    dates_between(calendar, i64::MIN, until)
        .into_iter()
        .filter(|dates| dates.component == index)
        .min_by_key(|dates| dates.start)
}

/// Start, end and whether it lasts whole days, of the first date of the event at `index`.
pub(crate) fn first_occurrence(calendar: &ICalendar, index: usize) -> Option<(i64, i64, bool)> {
    first_dates(calendar, index).map(|dates| (dates.start, dates.end, dates.all_day))
}

/// When an alarm of an event goes off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Trigger {
    /// Seconds after the start (negative: before), or after the end.
    Relative { seconds: i64, to_end: bool },
    /// At this moment, Unix milliseconds.
    Absolute(i64),
}

/// The alarms of the event at `index` (its `VALARM` components).
pub(crate) fn triggers(calendar: &ICalendar, index: usize) -> Vec<Trigger> {
    let Ok(id) = u32::try_from(index) else {
        return Vec::new();
    };
    calendar
        .alarms_for_id(id)
        .filter_map(|alarm| {
            let entry = alarm.property(&ICalendarProperty::Trigger)?;
            match entry.values.first()? {
                ICalendarValue::Duration(duration) => Some(Trigger::Relative {
                    seconds: duration.as_seconds(),
                    to_end: matches!(
                        param(entry, &ICalendarParameterName::Related),
                        Some(ICalendarParameterValue::Related(
                            calcard::icalendar::ICalendarRelated::End
                        ))
                    ),
                }),
                ICalendarValue::PartialDateTime(date) => date
                    .to_timestamp()
                    .map(|seconds| Trigger::Absolute(seconds * 1000)),
                _ => None,
            }
        })
        .collect()
}

/// The reminder the event dialog shows: minutes before the start of the first alarm that says
/// so; `None` without one.
pub(crate) fn reminder_minutes(calendar: &ICalendar, index: usize) -> Option<i64> {
    triggers(calendar, index)
        .into_iter()
        .find_map(|trigger| match trigger {
            Trigger::Relative {
                seconds,
                to_end: false,
            } if seconds <= 0 => Some(-seconds / 60),
            _ => None,
        })
}

/// The lines of an alarm `minutes` before the start, for the event dialog's reminder.
pub(crate) fn alarm_lines(minutes: i64, title: &str) -> Vec<String> {
    vec![
        "BEGIN:VALARM".to_owned(),
        "ACTION:DISPLAY".to_owned(),
        format!("DESCRIPTION:{}", escape(title)),
        format!("TRIGGER:-PT{}M", minutes.max(0)),
        "END:VALARM".to_owned(),
    ]
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
