import { describe, expect, it } from "vitest";

import { firstName, messagesOf, rowFor, senderNames } from "./conversations";

const rows = [
  { id: "hike-3", messageIds: ["hike-3", "hike-2", "hike-1"] },
  { id: "parcel", messageIds: ["parcel"] },
];

describe("rows of the list", () => {
  it("finds a row by its own id or by a message in its conversation", () => {
    expect(rowFor(rows, "parcel")?.id).toBe("parcel");
    expect(rowFor(rows, "hike-1")?.id).toBe("hike-3");
    expect(rowFor(rows, "gone")).toBeUndefined();
    expect(rowFor(rows, null)).toBeUndefined();
  });

  it("acts on every message behind the rows, once", () => {
    expect(messagesOf(rows, ["hike-3", "parcel", "hike-2"])).toEqual([
      "hike-3",
      "hike-2",
      "hike-1",
      "parcel",
    ]);
    // An id outside the list, e.g. from an older notice, stands for itself.
    expect(messagesOf(rows, ["elsewhere"])).toEqual(["elsewhere"]);
  });
});

describe("senders of a conversation", () => {
  it("uses first names", () => {
    expect(firstName({ name: "Clara Neumann", address: "c@example.com" })).toBe("Clara");
    expect(firstName({ name: "Müller, Hans", address: "h@example.com" })).toBe("Hans");
    expect(firstName({ name: "Dr. Jana Wolf", address: "j@example.com" })).toBe("Jana");
    expect(firstName({ name: null, address: "kalender@example.com" })).toBe("kalender");
  });

  it("names the user as me", () => {
    const senders = [
      { name: "Lena Fischer", address: "lena@example.org" },
      { name: "Alex Berger", address: "Alex@Example.org" },
      { name: "Jonas Weber", address: "jonas@example.org" },
    ];
    expect(senderNames(senders, new Set(["alex@example.org"]))).toBe("Lena, ich, Jonas");
  });
});
