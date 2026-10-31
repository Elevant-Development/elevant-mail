import type { CSSProperties } from "react";

import { t } from "@/lib/i18n";
import type { Calendar, Occurrence } from "@/lib/tauri";

/** The calendar on this computer has no name of its own. */
export function calendarName(calendar: Calendar): string {
  return calendar.name || t("calendar.local");
}

/**
 * An event block filled with its calendar's colour, as calendars usually show them. The
 * calendar colours are light, so the text is dark in both themes.
 */
export function eventStyle(color: string): CSSProperties {
  return { backgroundColor: color, color: "oklch(0.21 0.02 260)" };
}

/** "10:00–11:30" in the user's clock. */
export function formatTimeRange(occurrence: Occurrence, locale: string): string {
  return new Intl.DateTimeFormat(locale, { hour: "numeric", minute: "2-digit" }).formatRange(
    occurrence.start,
    Math.max(occurrence.start, occurrence.end),
  );
}

export function formatTime(timestamp: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, { hour: "numeric", minute: "2-digit" }).format(timestamp);
}
