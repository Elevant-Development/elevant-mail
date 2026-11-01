/**
 * Dates for the calendar views: which days a view shows, its title, and where events go in a
 * day. Days are local midnights; all-day events come as UTC midnights (see the backend's
 * `Occurrence`).
 */
import type { Calendar, EventInput, Occurrence } from "./tauri";

export type CalendarView = "day" | "week" | "month";

export const MINUTE_MS = 60 * 1000;
export const HOUR_MS = 60 * MINUTE_MS;

/** Local midnight of the day `timestamp` falls on. */
export function startOfDay(timestamp: number): number {
  const date = new Date(timestamp);
  date.setHours(0, 0, 0, 0);
  return date.getTime();
}

/** `days` days later at the same local time, also across a clock change. */
export function addDays(timestamp: number, days: number): number {
  const date = new Date(timestamp);
  date.setDate(date.getDate() + days);
  return date.getTime();
}

/** The local day as the backend writes all-day dates: midnight UTC of that date. */
export function utcDate(day: number): number {
  const date = new Date(day);
  return Date.UTC(date.getFullYear(), date.getMonth(), date.getDate());
}

/** The local midnight of an all-day date (midnight UTC). */
export function localDay(utcMidnight: number): number {
  const date = new Date(utcMidnight);
  return new Date(date.getUTCFullYear(), date.getUTCMonth(), date.getUTCDate()).getTime();
}

/** The first day of the week where the user lives: 1 Monday … 7 Sunday, as `Intl.Locale`. */
export function firstDayOfWeek(locale: string): number {
  try {
    const info = new Intl.Locale(locale) as Intl.Locale & {
      getWeekInfo?: () => { firstDay: number };
      weekInfo?: { firstDay: number };
    };
    const week = info.getWeekInfo?.() ?? info.weekInfo;
    if (week) return week.firstDay;
  } catch {
    // An unknown locale: fall through.
  }
  return /^en-(us|ca)|^ja|^zh-tw|^he|^pt-br/i.test(locale) ? 7 : 1;
}

export function startOfWeek(timestamp: number, locale: string): number {
  const day = startOfDay(timestamp);
  const weekday = new Date(day).getDay() || 7; // 1 Monday … 7 Sunday
  const back = (weekday - firstDayOfWeek(locale) + 7) % 7;
  return addDays(day, -back);
}

/** The days a view shows around `anchor`, and the range to ask the backend for. */
export function viewDays(view: CalendarView, anchor: number, locale: string): number[] {
  let first: number;
  let count: number;
  if (view === "day") {
    first = startOfDay(anchor);
    count = 1;
  } else if (view === "week") {
    first = startOfWeek(anchor, locale);
    count = 7;
  } else {
    const date = new Date(anchor);
    first = startOfWeek(new Date(date.getFullYear(), date.getMonth(), 1).getTime(), locale);
    count = 42;
  }
  return Array.from({ length: count }, (_, index) => addDays(first, index));
}

/** One step back or on: a day, a week or a month. */
export function shiftAnchor(view: CalendarView, anchor: number, step: 1 | -1): number {
  if (view === "day") return addDays(anchor, step);
  if (view === "week") return addDays(anchor, 7 * step);
  const date = new Date(anchor);
  return new Date(date.getFullYear(), date.getMonth() + step, 1).getTime();
}

/** "Dienstag, 3. November 2026", "2.–8. Nov. 2026", "November 2026". */
export function periodTitle(view: CalendarView, anchor: number, locale: string): string {
  if (view === "day") {
    return new Intl.DateTimeFormat(locale, { dateStyle: "full" }).format(anchor);
  }
  if (view === "month") {
    return new Intl.DateTimeFormat(locale, { month: "long", year: "numeric" }).format(anchor);
  }
  const days = viewDays("week", anchor, locale);
  return new Intl.DateTimeFormat(locale, {
    day: "numeric",
    month: "short",
    year: "numeric",
  }).formatRange(days[0]!, days[6]!);
}

/** Whether the occurrence shows on the local day starting at `day`. */
export function isOnDay(occurrence: Occurrence, day: number): boolean {
  if (occurrence.allDay) {
    const date = utcDate(day);
    return occurrence.start <= date && date < occurrence.end;
  }
  const next = addDays(day, 1);
  return occurrence.start < next && Math.max(occurrence.end, occurrence.start + 1) > day;
}

/** A timed event placed in a day column: minutes from midnight, and its share of the width. */
export interface Placed {
  occurrence: Occurrence;
  /** Minutes after the day's midnight. */
  top: number;
  /** Minutes, at least `minimum`. */
  height: number;
  column: number;
  columns: number;
}

/**
 * Lays out the timed events of a day: events that overlap share the width side by side, each
 * group of overlapping events as wide as it needs. Events reaching into the day from the one
 * before, or on into the next, are cut at midnight.
 */
export function layoutDay(occurrences: Occurrence[], day: number, minimum = 20): Placed[] {
  const next = addDays(day, 1);
  const minutes = (timestamp: number) => (timestamp - day) / MINUTE_MS;
  const dayLength = minutes(next);
  const items = occurrences
    .filter((occurrence) => !occurrence.allDay && isOnDay(occurrence, day))
    .map((occurrence) => {
      const top = Math.max(0, minutes(occurrence.start));
      const bottom = Math.min(dayLength, Math.max(minutes(occurrence.end), top + minimum));
      return { occurrence, top, bottom };
    })
    .sort((a, b) => a.top - b.top || b.bottom - a.bottom);

  const placed: Placed[] = [];
  let group: Placed[] = [];
  let columnEnds: number[] = [];
  let groupEnd = -1;
  const closeGroup = () => {
    for (const item of group) item.columns = columnEnds.length;
    group = [];
    columnEnds = [];
  };
  for (const item of items) {
    if (item.top >= groupEnd) closeGroup();
    let column = columnEnds.findIndex((end) => end <= item.top);
    if (column === -1) {
      column = columnEnds.length;
      columnEnds.push(item.bottom);
    } else {
      columnEnds[column] = item.bottom;
    }
    const entry: Placed = {
      occurrence: item.occurrence,
      top: item.top,
      height: Math.max(item.bottom - item.top, Math.min(minimum, dayLength - item.top)),
      column,
      columns: 1,
    };
    group.push(entry);
    placed.push(entry);
    groupEnd = Math.max(groupEnd, item.bottom);
  }
  closeGroup();
  return placed;
}

/** The start of the hour after `now`, for a new event. */
export function nextFullHour(now: number): number {
  const date = new Date(now);
  date.setMinutes(0, 0, 0);
  return date.getTime() + HOUR_MS;
}

/** The calendar a new event goes to: the first visible one that can be written. */
export function defaultCalendar(calendars: Calendar[]): Calendar | undefined {
  const writable = calendars.filter((calendar) => !calendar.readOnly);
  return writable.find((calendar) => calendar.visible) ?? writable[0];
}

/**
 * A new event as the dialog starts it: an hour long with the default `reminder`, or one whole
 * day without one.
 */
export function newEvent(
  start: number,
  allDay: boolean,
  calendarId: string,
  reminder: number | null = null,
): EventInput {
  const first = allDay ? utcDate(startOfDay(start)) : start;
  return {
    id: null,
    calendarId,
    title: "",
    location: "",
    description: "",
    start: first,
    end: allDay ? utcDate(addDays(startOfDay(start), 1)) : start + HOUR_MS,
    allDay,
    repeat: null,
    attendees: [],
    reminder: allDay ? null : reminder,
    instance: null,
  };
}
