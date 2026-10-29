import { describe, expect, it } from "vitest";

import { displayName, formatListDate, formatSize, hueFromString, initials } from "./formatting";

const locale = "de-DE";
// Wednesday, 28 October 2026, 15:00 local time
const now = new Date(2026, 9, 28, 15, 0).getTime();

describe("formatListDate", () => {
  it("uses the 12-hour clock where it is usual", () => {
    expect(formatListDate(new Date(2026, 9, 28, 9, 5).getTime(), now, "en-US")).toMatch(
      /^9:05\sAM$/,
    );
  });

  it("shows the time for today", () => {
    expect(formatListDate(new Date(2026, 9, 28, 9, 5).getTime(), now, locale)).toBe("09:05");
  });

  it("shows Gestern for yesterday", () => {
    expect(formatListDate(new Date(2026, 9, 27, 23, 59).getTime(), now, locale)).toBe("Gestern");
  });

  it("shows the weekday within the last week", () => {
    // The abbreviation ("Sa" or "Sa.") depends on the ICU version, so compare against Intl.
    const saturday = new Date(2026, 9, 24, 12, 0).getTime();
    const expected = new Intl.DateTimeFormat(locale, { weekday: "short" }).format(saturday);
    expect(formatListDate(saturday, now, locale)).toBe(expected);
  });

  it("shows day and month within the year", () => {
    expect(formatListDate(new Date(2026, 2, 3, 12, 0).getTime(), now, locale)).toBe("3. März");
  });

  it("shows the full date for earlier years", () => {
    expect(formatListDate(new Date(2025, 11, 24, 12, 0).getTime(), now, locale)).toBe("24.12.2025");
  });
});

describe("initials", () => {
  it("takes first and last word of the name", () => {
    expect(initials({ name: "Lena Maria Fischer", address: "lena@example.org" })).toBe("LF");
  });

  it("falls back to the address", () => {
    expect(initials({ name: null, address: "kalender@example.com" })).toBe("K");
  });

  it("ignores punctuation in names", () => {
    expect(initials({ name: '"Weber, Jonas"', address: "jonas@example.org" })).toBe("WJ");
  });
});

describe("displayName", () => {
  it("prefers the name over the address", () => {
    expect(displayName({ name: " Clara Neumann ", address: "clara@example.com" })).toBe(
      "Clara Neumann",
    );
    expect(displayName({ name: "", address: "clara@example.com" })).toBe("clara@example.com");
  });
});

describe("hueFromString", () => {
  it("is stable and within range", () => {
    const hue = hueFromString("lena@example.org");
    expect(hue).toBe(hueFromString("lena@example.org"));
    expect(hue).toBeGreaterThanOrEqual(0);
    expect(hue).toBeLessThan(360);
  });
});

describe("formatSize", () => {
  it("picks a readable unit", () => {
    expect(formatSize(820, locale)).toBe("820 B");
    expect(formatSize(12_400, locale)).toBe("12 KB");
    expect(formatSize(1.4 * 1024 * 1024, locale)).toBe("1,4 MB");
    expect(formatSize(25 * 1024 * 1024, locale)).toBe("25 MB");
  });
});
