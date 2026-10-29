import { describe, expect, it } from "vitest";

import { newMailNotice } from "./notify";
import type { NewMail } from "./tauri";

const mail = (name: string, subject: string | null = "Hallo"): NewMail => ({
  id: name,
  accountId: "acc",
  from: { name, address: `${name.toLowerCase()}@example.org` },
  subject,
});

describe("newMailNotice", () => {
  it("names sender and subject of a single message", () => {
    expect(newMailNotice([mail("Lena")], "Privat", true)).toEqual({
      title: "Lena",
      body: "Hallo",
    });
    expect(newMailNotice([mail("Lena", null)], "Privat", true).body).toBe("(Kein Betreff)");
  });

  it("counts several and lists up to three senders", () => {
    expect(newMailNotice([mail("Lena"), mail("Jonas"), mail("Lena")], "Privat", true)).toEqual({
      title: "3 neue Mails",
      body: "Von Lena, Jonas",
    });
    const many = ["A", "B", "C", "D"].map((name) => mail(name));
    expect(newMailNotice(many, "Privat", true).body).toBe("Von A, B, C und weiteren");
  });

  it("shows only count and account without preview", () => {
    expect(newMailNotice([mail("Lena")], "Privat", false)).toEqual({
      title: "Neue Mail",
      body: "In Privat",
    });
  });
});
