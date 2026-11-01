import { describe, expect, it } from "vitest";

import { reminderLabel, reminderNotice } from "./reminders";

describe("reminders", () => {
  it("says how long before", () => {
    expect(reminderLabel(null, "de-DE")).toBe("Keine");
    expect(reminderLabel(0, "de-DE")).toBe("Zum Beginn");
    expect(reminderLabel(15, "de-DE")).toBe("15 Minuten vorher");
    expect(reminderLabel(60, "de-DE")).toBe("1 Stunde vorher");
    expect(reminderLabel(2880, "de-DE")).toBe("2 Tage vorher");
  });

  it("names the event, its time and place", () => {
    const start = new Date(2026, 10, 3, 10, 0).getTime();
    const notice = reminderNotice(
      { eventId: "e", title: "Jour fixe", location: "Raum 2.14", start, allDay: false, at: start },
      "de-DE",
    );
    expect(notice.title).toBe("Jour fixe");
    expect(notice.body).toContain("10:00");
    expect(notice.body).toContain("Raum 2.14");
  });
});
