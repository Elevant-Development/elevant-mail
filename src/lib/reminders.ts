/** Reminders: the choices offered, and the notification for one that is due. */
import { formatEventTime } from "./invitations";
import { t } from "./i18n";
import type { Notice } from "./notify";
import type { Reminder } from "./tauri";

/** Minutes before the start; null: none. */
export const reminderChoices: (number | null)[] = [null, 0, 5, 10, 15, 30, 60, 120, 1440, 2880];

/** "15 Minuten vorher", "1 Tag vorher", "Zum Beginn", "Keine". */
export function reminderLabel(minutes: number | null, locale: string): string {
  if (minutes === null) return t("reminder.none");
  if (minutes === 0) return t("reminder.atStart");
  const [value, unit]: [number, Intl.RelativeTimeFormatUnit] =
    minutes % 1440 === 0
      ? [minutes / 1440, "day"]
      : minutes % 60 === 0
        ? [minutes / 60, "hour"]
        : [minutes, "minute"];
  const amount = new Intl.NumberFormat(locale, {
    style: "unit",
    unit,
    unitDisplay: "long",
  }).format(value);
  return t("reminder.before", { amount });
}

/** The notification: the event's title, its time and place. */
export function reminderNotice(reminder: Reminder, locale: string): Notice {
  const when = formatEventTime(
    { start: reminder.start, end: reminder.start, allDay: reminder.allDay },
    locale,
  );
  return {
    title: reminder.title ?? t("invitation.untitled"),
    body: reminder.location ? `${when} · ${reminder.location}` : when,
  };
}
