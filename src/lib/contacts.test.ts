import { describe, expect, it } from "vitest";

import {
  birthdayOf,
  birthdayParts,
  defaultAddressBook,
  fold,
  formatBirthday,
  initials,
  letterOf,
} from "./contacts";

describe("contacts", () => {
  it("folds names as the backend does", () => {
    expect(fold("Müller")).toBe("muller");
    expect(fold("Straße")).toBe("strasse");
    expect(fold("Élodie")).toBe("elodie");
  });

  it("lists names under their letter", () => {
    expect(letterOf("Ümit Kaya")).toBe("U");
    expect(letterOf("anna")).toBe("A");
    expect(letterOf("+49 170")).toBe("#");
  });

  it("makes initials", () => {
    expect(initials("Anna Maria Müller")).toBe("AM");
    expect(initials("Stadtwerke")).toBe("S");
    expect(initials("jonas@example.com")).toBe("JC");
  });

  it("takes birthdays with and without a year", () => {
    expect(birthdayOf("12", "4", "1985")).toBe("1985-04-12");
    expect(birthdayOf("31", "7", "")).toBe("--07-31");
    expect(birthdayOf("32", "1", "")).toBeNull();
    expect(birthdayOf("1", "1", "85")).toBeNull();
    expect(birthdayParts("--07-31")).toEqual(["31", "7", ""]);
    expect(birthdayParts("1985-04-12")).toEqual(["12", "4", "1985"]);
    expect(formatBirthday("--07-31", "de-DE")).toBe("31. Juli");
    expect(formatBirthday("1985-04-12", "de-DE")).toBe("12. April 1985");
  });

  it("puts new contacts into the address book shown, if it can", () => {
    const books = [
      { id: "a", accountId: "x", name: "Firma", readOnly: true },
      { id: "b", accountId: "x", name: "Kontakte", readOnly: false },
      { id: "local", accountId: null, name: "", readOnly: false },
    ];
    expect(defaultAddressBook(books, null)?.id).toBe("b");
    expect(defaultAddressBook(books, "local")?.id).toBe("local");
    expect(defaultAddressBook(books, "a")?.id).toBe("b");
  });
});
