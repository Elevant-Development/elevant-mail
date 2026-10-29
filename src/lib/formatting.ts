import type { Address } from "./tauri";

const DAY_MS = 24 * 60 * 60 * 1000;

function startOfDay(timestamp: number): number {
  const date = new Date(timestamp);
  date.setHours(0, 0, 0, 0);
  return date.getTime();
}

function capitalize(text: string): string {
  return text.charAt(0).toLocaleUpperCase() + text.slice(1);
}

/**
 * Short date for the message list: the time for today, "Gestern", the weekday within the last
 * week, day and month within the year, the full date otherwise.
 */
export function formatListDate(timestamp: number, now: number, locale: string): string {
  const daysAgo = Math.round((startOfDay(now) - startOfDay(timestamp)) / DAY_MS);

  if (daysAgo <= 0) {
    return new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit" }).format(
      timestamp,
    );
  }
  if (daysAgo === 1) {
    return capitalize(new Intl.RelativeTimeFormat(locale, { numeric: "auto" }).format(-1, "day"));
  }
  if (daysAgo < 7) {
    return new Intl.DateTimeFormat(locale, { weekday: "short" }).format(timestamp);
  }
  if (new Date(timestamp).getFullYear() === new Date(now).getFullYear()) {
    return new Intl.DateTimeFormat(locale, { day: "numeric", month: "short" }).format(timestamp);
  }
  return new Intl.DateTimeFormat(locale, { dateStyle: "medium" }).format(timestamp);
}

/** Full date and time for the reader header. */
export function formatFullDate(timestamp: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, { dateStyle: "full", timeStyle: "short" }).format(
    timestamp,
  );
}

/** Name if known, otherwise the address. */
export function displayName(address: Address): string {
  return address.name?.trim() || address.address;
}

/** Up to two initials from the name, or the first letter of the address. */
export function initials(address: Address): string {
  const words = (address.name ?? "")
    .replace(/[^\p{L}\p{N}\s-]/gu, "")
    .split(/[\s-]+/)
    .filter(Boolean);
  const first = words[0];
  const last = words.length > 1 ? words[words.length - 1] : undefined;
  if (first) {
    return (first.charAt(0) + (last?.charAt(0) ?? "")).toLocaleUpperCase();
  }
  return address.address.charAt(0).toLocaleUpperCase() || "?";
}

/** Stable hue (0–359) derived from a string, used for avatar colours. */
export function hueFromString(value: string): number {
  let hash = 0;
  for (const char of value) {
    hash = (hash * 31 + char.codePointAt(0)!) | 0;
  }
  return Math.abs(hash) % 360;
}
