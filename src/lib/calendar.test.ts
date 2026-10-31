import { describe, expect, it } from "vitest";

import {
  HOUR_MS,
  isOnDay,
  layoutDay,
  localDay,
  periodTitle,
  shiftAnchor,
  startOfWeek,
  utcDate,
  viewDays,
} from "./calendar";
import type { Occurrence } from "./tauri";

// Wednesday, 4 November 2026, 15:00 local time
const now = new Date(2026, 10, 4, 15, 0).getTime();
const day = new Date(2026, 10, 4).getTime();

function at(hours: number, minutes = 0): number {
  return new Date(2026, 10, 4, hours, minutes).getTime();
}

function event(start: number, end: number, allDay = false): Occurrence {
  return {
    eventId: `${start}-${end}`,
    calendarId: "local",
    title: null,
    location: null,
    start,
    end,
    allDay,
    recurring: false,
  };
}

describe("weeks and views", () => {
  it("starts the week on Monday in Germany and on Sunday in the US", () => {
    expect(new Date(startOfWeek(now, "de-DE")).getDate()).toBe(2);
    expect(new Date(startOfWeek(now, "en-US")).getDate()).toBe(1);
  });

  it("shows six weeks for a month, from the week of its first day", () => {
    const days = viewDays("month", now, "de-DE");
    expect(days).toHaveLength(42);
    // 1 November 2026 is a Sunday: the grid starts on Monday, 26 October.
    expect(new Date(days[0]!).getDate()).toBe(26);
    expect(new Date(days[0]!).getHours()).toBe(0);
  });

  it("steps by a day, a week or a month", () => {
    expect(new Date(shiftAnchor("day", now, 1)).getDate()).toBe(5);
    expect(new Date(shiftAnchor("week", now, -1)).getDate()).toBe(28);
    expect(new Date(shiftAnchor("month", now, 1)).getMonth()).toBe(11);
  });

  it("names the period", () => {
    expect(periodTitle("month", now, "de-DE")).toBe("November 2026");
    expect(periodTitle("day", now, "de-DE")).toBe("Mittwoch, 4. November 2026");
    expect(periodTitle("week", now, "de-DE")).toContain("8.");
  });
});

describe("events on a day", () => {
  it("puts all-day dates on their local day", () => {
    const allDay = event(utcDate(day), utcDate(day) + 24 * HOUR_MS, true);
    expect(isOnDay(allDay, day)).toBe(true);
    expect(isOnDay(allDay, new Date(2026, 10, 5).getTime())).toBe(false);
    expect(localDay(utcDate(day))).toBe(day);
  });

  it("shares the width between overlapping events only", () => {
    const placed = layoutDay(
      [
        event(at(9), at(10)),
        event(at(9, 30), at(11)),
        event(at(10), at(10, 30)),
        event(at(14), at(15)),
      ],
      day,
    );
    expect(placed.map((p) => [p.top, p.column, p.columns])).toEqual([
      [540, 0, 2],
      [570, 1, 2],
      [600, 0, 2],
      [840, 0, 1],
    ]);
  });

  it("cuts events at midnight and gives short ones a minimum height", () => {
    const night = event(new Date(2026, 10, 3, 22).getTime(), at(2));
    const [placed] = layoutDay([night, event(at(12), at(12))], day);
    expect(placed!.top).toBe(0);
    expect(placed!.height).toBe(120);
    expect(layoutDay([event(at(12), at(12))], day)[0]!.height).toBe(20);
  });
});
