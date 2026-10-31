import { describe, expect, it } from "vitest";

import { formatEventTime, formatRecurrence } from "./invitations";
import type { Recurrence } from "./tauri";

const DAY = 24 * 60 * 60 * 1000;

describe("formatEventTime", () => {
  it("shows one day of an all-day event as a date, in UTC", () => {
    const start = Date.UTC(2026, 10, 12);
    const text = formatEventTime({ start, end: start + DAY, allDay: true }, "de-DE");
    expect(text).toBe("Donnerstag, 12. November 2026");
  });

  it("ends an all-day range on its last day", () => {
    const start = Date.UTC(2026, 10, 12);
    const text = formatEventTime({ start, end: start + 3 * DAY, allDay: true }, "de-DE");
    expect(text).toContain("12.");
    expect(text).toContain("14.");
    expect(text).not.toContain("15.");
  });

  it("names the day once for a meeting within it", () => {
    const start = new Date(2026, 10, 3, 10, 0).getTime();
    const text = formatEventTime({ start, end: start + 60 * 60 * 1000, allDay: false }, "de-DE");
    expect(text.match(/November/g)).toHaveLength(1);
    expect(text).toContain("10:00");
    expect(text).toContain("11:00");
  });
});

describe("formatRecurrence", () => {
  const weekly: Recurrence = {
    frequency: "weekly",
    interval: 1,
    weekdays: [],
    count: null,
    until: null,
  };

  it("says how often", () => {
    expect(formatRecurrence(weekly, "de-DE")).toBe("Wöchentlich");
    expect(formatRecurrence({ ...weekly, interval: 2 }, "de-DE")).toBe("Alle 2 Wochen");
  });

  it("names the weekdays and the end", () => {
    expect(formatRecurrence({ ...weekly, weekdays: [1, 3], count: 4 }, "de-DE")).toBe(
      "Wöchentlich am Dienstag und Donnerstag, 4-mal",
    );
    expect(formatRecurrence({ ...weekly, until: new Date(2026, 11, 22).getTime() }, "de-DE")).toBe(
      "Wöchentlich, bis 22.12.2026",
    );
  });
});
