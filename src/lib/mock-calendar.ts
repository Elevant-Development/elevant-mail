/**
 * Sample calendars and events for `pnpm dev` (see mock.ts): two calendars of the work account,
 * the private one with a read-only birthday calendar, and the one on this computer. Dates are
 * counted from the current week, so the views always have something to show.
 */
import { addDays, startOfDay, startOfWeek, utcDate } from "./calendar";
import { language } from "./i18n";
import type {
  Address,
  Attendee,
  Backend,
  CaldavStatus,
  Calendar,
  EventDetails,
  EventInput,
  Frequency,
  Occurrence,
} from "./tauri";

const L = (de: string, en: string) => (language === "de" ? de : en);

/** As the backend gives them: an account's calendars in the account's colour. */
const calendars: Calendar[] = [
  {
    id: "cal-work",
    accountId: "acc-work",
    name: L("Kalender", "Calendar"),
    color: "oklch(0.74 0.14 60)",
    readOnly: false,
    visible: true,
    canInvite: true,
  },
  {
    id: "cal-team",
    accountId: "acc-work",
    name: L("Team Elevant", "Team Elevant"),
    color: "oklch(0.74 0.14 60)",
    readOnly: false,
    visible: true,
    canInvite: true,
  },
  {
    id: "cal-private",
    accountId: "acc-private",
    name: L("Privat", "Personal"),
    color: "oklch(0.72 0.13 250)",
    readOnly: false,
    visible: true,
    canInvite: true,
  },
  {
    id: "cal-birthdays",
    accountId: "acc-private",
    name: L("Geburtstage", "Birthdays"),
    color: "oklch(0.72 0.13 250)",
    readOnly: true,
    visible: true,
    canInvite: false,
  },
  {
    id: "local",
    accountId: null,
    name: "",
    color: "oklch(0.68 0.14 295)",
    readOnly: false,
    visible: true,
    canInvite: false,
  },
];

interface MockEvent {
  id: string;
  calendarId: string;
  title: string;
  location: string;
  description: string;
  start: number;
  end: number;
  allDay: boolean;
  repeat: Frequency | null;
  organizer: Address | null;
  attendees: Attendee[];
}

const alex: Address = { name: "Alex Berger", address: "alex.berger@example.com" };
const mira: Address = { name: "Mira Koch", address: "mira.koch@example.com" };
const jonas: Address = { name: "Jonas Weber", address: "jonas.weber@example.com" };

function buildEvents(): MockEvent[] {
  const now = Date.now();
  const monday = startOfWeek(now, "de-DE");
  /** A local time on a day of this week (0 Monday … 6 Sunday), `week` weeks on. */
  const at = (weekday: number, hours: number, minutes = 0, week = 0) => {
    const date = new Date(addDays(monday, weekday + 7 * week));
    date.setHours(hours, minutes);
    return date.getTime();
  };
  /** An all-day date, `days` days from today. */
  const day = (days: number) => utcDate(addDays(startOfDay(now), days));
  let count = 0;
  const event = (
    fields: Partial<MockEvent> & Pick<MockEvent, "calendarId" | "title" | "start" | "end">,
  ) => ({
    id: `evt-${++count}`,
    location: "",
    description: "",
    allDay: false,
    repeat: null,
    organizer: null,
    attendees: [],
    ...fields,
  });
  const today = (new Date(now).getDay() + 6) % 7;
  return [
    event({
      calendarId: "cal-work",
      title: "Jour fixe",
      location: L("Raum 2.14", "Room 2.14"),
      start: at(0, 9, 30, -4),
      end: at(0, 10, 0, -4),
      repeat: "weekly",
    }),
    event({
      calendarId: "cal-team",
      title: L("Planung Herbstfest", "Autumn party planning"),
      location: L("Raum 2.14", "Room 2.14"),
      start: at(1, 10),
      end: at(1, 11),
      repeat: "weekly",
      organizer: jonas,
      attendees: [
        { address: alex, status: "accepted", optional: false },
        { address: mira, status: "accepted", optional: false },
      ],
    }),
    event({
      calendarId: "cal-work",
      title: L("Bewerbungsgespräch", "Job interview"),
      location: L("Raum 3.02", "Room 3.02"),
      start: at(1, 10, 30),
      end: at(1, 11, 30),
    }),
    event({
      calendarId: "cal-work",
      title: L("Sprint-Review", "Sprint review"),
      start: at(2, 14),
      end: at(2, 15),
      organizer: alex,
      attendees: [{ address: mira, status: "accepted", optional: false }],
      description: L(
        "Demo der neuen Kalenderansicht, danach Planung des nächsten Sprints.",
        "Demo of the new calendar view, then planning the next sprint.",
      ),
    }),
    event({
      calendarId: "cal-work",
      title: L("Kundentermin Lindner", "Client meeting Lindner"),
      location: L("Kanzlei Lindner, Marktplatz 4", "Lindner law office, Marktplatz 4"),
      start: at(3, 11),
      end: at(3, 12),
    }),
    event({
      calendarId: "cal-work",
      title: L("Mittagessen mit Tim", "Lunch with Tim"),
      start: at(today, 12, 30),
      end: at(today, 13, 30),
    }),
    event({
      calendarId: "cal-team",
      title: L("Team-Frühstück", "Team breakfast"),
      start: at(4, 9),
      end: at(4, 9, 45),
    }),
    event({
      calendarId: "cal-team",
      title: "Release 0.6",
      start: utcDate(addDays(monday, 4)),
      end: utcDate(addDays(monday, 5)),
      allDay: true,
    }),
    event({
      calendarId: "cal-private",
      title: "Yoga",
      location: L("Studio am Park", "Studio by the park"),
      start: at(1, 18, 30, -2),
      end: at(1, 19, 45, -2),
      repeat: "weekly",
    }),
    event({
      calendarId: "cal-private",
      title: L("Wanderung am See", "Hike around the lake"),
      location: L("Bahnhof, Gleis 3", "Station, platform 3"),
      start: at(5, 9),
      end: at(5, 16),
    }),
    event({
      calendarId: "cal-private",
      title: L("Zahnarzt", "Dentist"),
      location: L("Praxis Dr. Sommer", "Dr. Sommer's practice"),
      start: at(2, 8, 0, 1),
      end: at(2, 8, 45, 1),
    }),
    event({
      calendarId: "cal-private",
      title: L("Hüttenwochenende", "Cabin weekend"),
      location: L("Berghütte am Grünten, Allgäu", "Mountain hut on the Grünten, Allgäu"),
      start: day(12),
      end: day(15),
      allDay: true,
    }),
    event({
      calendarId: "cal-birthdays",
      title: L("Geburtstag Lena Fischer", "Lena Fischer's birthday"),
      start: utcDate(addDays(monday, 6)),
      end: utcDate(addDays(monday, 7)),
      allDay: true,
      repeat: "yearly",
    }),
    event({
      calendarId: "local",
      title: L("Steuererklärung abgeben", "File the tax return"),
      start: day(9),
      end: day(10),
      allDay: true,
    }),
  ];
}

let events = buildEvents();
let nextId = 1000;

/** The dates of a series from its start on, as far as `to`. */
function datesOf(event: MockEvent, to: number): { start: number; end: number }[] {
  const length = event.end - event.start;
  if (!event.repeat || event.repeat === "other") return [{ start: event.start, end: event.end }];
  const dates = [];
  for (let n = 0; n < 2000; n++) {
    const date = new Date(event.start);
    if (event.repeat === "daily") date.setUTCDate(date.getUTCDate() + n);
    if (event.repeat === "weekly") date.setUTCDate(date.getUTCDate() + 7 * n);
    if (event.repeat === "monthly") date.setUTCMonth(date.getUTCMonth() + n);
    if (event.repeat === "yearly") date.setUTCFullYear(date.getUTCFullYear() + n);
    // Timed series keep their local time across a clock change.
    let start = date.getTime();
    if (!event.allDay) {
      const local = new Date(event.start);
      const moved = new Date(start);
      moved.setHours(local.getHours(), local.getMinutes());
      start = moved.getTime();
    }
    if (start >= to) break;
    dates.push({ start, end: start + length });
  }
  return dates;
}

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

function details(event: MockEvent): EventDetails {
  return {
    id: event.id,
    calendarId: event.calendarId,
    title: event.title || null,
    location: event.location || null,
    description: event.description || null,
    start: event.start,
    end: event.end,
    allDay: event.allDay,
    recurrence: event.repeat
      ? { frequency: event.repeat, interval: 1, weekdays: [], count: null, until: null }
      : null,
    organizer: event.organizer,
    attendees: event.attendees,
    readOnly: calendars.find((c) => c.id === event.calendarId)?.readOnly ?? false,
    ownEvent: event.organizer === null || event.organizer.address.startsWith("alex"),
  };
}

function writable(calendarId: string) {
  const calendar = calendars.find((c) => c.id === calendarId);
  if (!calendar) throw { kind: "notFound", hint: null };
  if (calendar.readOnly) throw { kind: "invalidInput", hint: null };
}

/** Where the sample accounts' calendars are; the work account signs in with Microsoft. */
const caldav: Record<string, CaldavStatus> = {
  "acc-private": {
    microsoft: false,
    enabled: true,
    url: "https://mail.example.org/SOGo/dav/alex/Calendar/",
  },
  "acc-work": { microsoft: true, enabled: true, url: null },
};

/** The calendar backend; `accountColor` gives an account's colour now, as the backend uses it. */
export const mockCalendar = (
  accountColor: (accountId: string) => string | undefined,
): Pick<
  Backend,
  | "syncCalendars"
  | "caldavStatus"
  | "setCaldav"
  | "listCalendars"
  | "setCalendarVisible"
  | "listOccurrences"
  | "getEvent"
  | "saveEvent"
  | "deleteEvent"
> => ({
  syncCalendars: async () => ({ changed: false }),
  caldavStatus: async (accountId) =>
    caldav[accountId] ?? { microsoft: false, enabled: true, url: null },
  setCaldav: async (accountId, enabled, url) => {
    await wait(600);
    // "falsch" in the address stands for a server without calendars.
    if (enabled && url?.includes("falsch"))
      throw { kind: "calendarsNotFound", hint: "checkServerAddress" };
    const status = {
      microsoft: caldav[accountId]?.microsoft ?? false,
      enabled,
      url: enabled ? (url ?? caldav[accountId]?.url ?? null) : null,
    };
    caldav[accountId] = status;
    return status;
  },
  listCalendars: async () =>
    calendars.map((calendar) => ({
      ...calendar,
      color: (calendar.accountId && accountColor(calendar.accountId)) || calendar.color,
    })),
  setCalendarVisible: async (calendarId, visible) => {
    const calendar = calendars.find((c) => c.id === calendarId);
    if (calendar) calendar.visible = visible;
  },
  listOccurrences: async (from, to): Promise<Occurrence[]> =>
    events
      .flatMap((event) =>
        datesOf(event, to)
          .filter((date) => date.start < to && Math.max(date.end, date.start + 1) > from)
          .map((date) => ({
            eventId: event.id,
            calendarId: event.calendarId,
            title: event.title || null,
            location: event.location || null,
            start: date.start,
            end: date.end,
            allDay: event.allDay,
            recurring: event.repeat !== null,
          })),
      )
      .sort((a, b) => a.start - b.start || a.end - b.end),
  getEvent: async (eventId) => {
    const event = events.find((e) => e.id === eventId);
    if (!event) throw { kind: "notFound", hint: null };
    return details(event);
  },
  saveEvent: async (input: EventInput) => {
    await wait(300);
    if (input.end < input.start) throw { kind: "invalidInput", hint: null };
    writable(input.calendarId);
    const fields = {
      calendarId: input.calendarId,
      title: input.title.trim(),
      location: input.location.trim(),
      description: input.description.trim(),
      start: input.start,
      end: input.end,
      allDay: input.allDay,
      repeat: input.repeat,
      attendees: input.attendees.map((address) => ({
        address,
        status: "needsAction" as const,
        optional: false,
      })),
      organizer: input.attendees.length > 0 ? alex : null,
    };
    const existing = input.id ? events.find((e) => e.id === input.id) : undefined;
    if (existing) {
      Object.assign(existing, fields);
      return existing.id;
    }
    const id = `evt-${++nextId}`;
    events = [...events, { id, ...fields }];
    return id;
  },
  deleteEvent: async (eventId) => {
    await wait(200);
    const event = events.find((e) => e.id === eventId);
    if (!event) throw { kind: "notFound", hint: null };
    writable(event.calendarId);
    events = events.filter((e) => e.id !== eventId);
  },
});
