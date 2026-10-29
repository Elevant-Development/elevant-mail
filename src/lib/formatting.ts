import type { Address, ImapSettings, Security } from "./tauri";

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
 * Short date for the message list: the time for today, "yesterday", the weekday within the last
 * week, day and month within the year, the full date otherwise.
 */
export function formatListDate(timestamp: number, now: number, locale: string): string {
  const daysAgo = Math.round((startOfDay(now) - startOfDay(timestamp)) / DAY_MS);

  if (daysAgo <= 0) {
    // "09:05" where the day has 24 hours, "9:05 AM" where it has twice 12.
    const twelveHours = new Intl.DateTimeFormat(locale, { hour: "numeric" }).resolvedOptions()
      .hour12;
    return new Intl.DateTimeFormat(locale, {
      hour: twelveHours ? "numeric" : "2-digit",
      minute: "2-digit",
    }).format(timestamp);
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

const SIZE_UNITS = ["B", "KB", "MB", "GB"] as const;

/** File size the way file managers show it: "820 B", "12 KB", "1,4 MB". */
export function formatSize(bytes: number, locale: string): string {
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < SIZE_UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit >= 2 && value < 10 ? 1 : 0;
  const number = new Intl.NumberFormat(locale, { maximumFractionDigits: digits }).format(value);
  return `${number} ${SIZE_UNITS[unit]}`;
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

export function formatSecurity(security: Security): string {
  return security === "tls" ? "TLS" : "STARTTLS";
}

/** One line for server settings: "imap.example.org · 993 · TLS". */
export function formatServer(settings: ImapSettings): string {
  return `${settings.host} · ${settings.port} · ${formatSecurity(settings.security)}`;
}
