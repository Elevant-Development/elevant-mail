/** Events in words: when one takes place, how a series repeats, and who is invited. */
import { displayName } from "./formatting";
import { t, tCount } from "./i18n";
import type { Address, Attendee, Invitation, Recurrence } from "./tauri";

const DAY_MS = 24 * 60 * 60 * 1000;
/** Attendees named before "and 3 more". */
const SHOWN_ATTENDEES = 5;
/** A Monday, for naming weekdays (0 Monday … 6 Sunday) in the interface language. */
const A_MONDAY = Date.UTC(2024, 0, 1);

/**
 * "Dienstag, 3. November 2026, 10:00–11:00". All-day events are dates in UTC (see the backend's
 * `Invitation`) and end the day before `end`.
 */
export function formatEventTime(
  event: Pick<Invitation, "start" | "end" | "allDay">,
  locale: string,
): string {
  if (event.allDay) {
    const format = new Intl.DateTimeFormat(locale, { dateStyle: "full", timeZone: "UTC" });
    const last = Math.max(event.start, event.end - DAY_MS);
    return last === event.start
      ? format.format(event.start)
      : format.formatRange(event.start, last);
  }
  return new Intl.DateTimeFormat(locale, { dateStyle: "full", timeStyle: "short" }).formatRange(
    event.start,
    Math.max(event.start, event.end),
  );
}

function weekdayName(day: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, { weekday: "long", timeZone: "UTC" }).format(
    A_MONDAY + day * DAY_MS,
  );
}

/** "Alle 2 Wochen am Dienstag und Donnerstag, 4-mal". */
export function formatRecurrence(rule: Recurrence, locale: string): string {
  if (rule.frequency === "other") return t("invitation.repeat.other");
  const every = {
    daily: "invitation.repeat.daily",
    weekly: "invitation.repeat.weekly",
    monthly: "invitation.repeat.monthly",
    yearly: "invitation.repeat.yearly",
  } as const;
  let text = tCount(every[rule.frequency], rule.interval);
  if (rule.weekdays.length > 0) {
    const days = new Intl.ListFormat(locale, { type: "conjunction" }).format(
      rule.weekdays.map((day) => weekdayName(day, locale)),
    );
    text = t("invitation.repeat.onDays", { repeat: text, days });
  }
  if (rule.count !== null) {
    text = t("invitation.repeat.end", {
      repeat: text,
      end: tCount("invitation.repeat.times", rule.count),
    });
  } else if (rule.until !== null) {
    const date = new Intl.DateTimeFormat(locale, { dateStyle: "medium" }).format(rule.until);
    text = t("invitation.repeat.end", {
      repeat: text,
      end: t("invitation.repeat.until", { date }),
    });
  }
  return text;
}

/** "Lena Fischer (Organisator), Alex Berger, Sam Becker und 2 weitere". */
export function formatAttendees(
  event: { organizer: Address | null; attendees: Attendee[] },
  locale: string,
): string {
  const organizer = event.organizer;
  const others = event.attendees.filter(
    (attendee) => attendee.address.address.toLowerCase() !== organizer?.address.toLowerCase(),
  );
  const names = [
    ...(organizer ? [t("invitation.organizer", { name: displayName(organizer) })] : []),
    ...others.map((attendee) =>
      attendee.optional
        ? t("invitation.optional", { name: displayName(attendee.address) })
        : displayName(attendee.address),
    ),
  ];
  const shown = names.slice(0, SHOWN_ATTENDEES);
  if (names.length > shown.length) {
    shown.push(tCount("invitation.more", names.length - shown.length));
  }
  return new Intl.ListFormat(locale, { type: "conjunction" }).format(shown);
}
