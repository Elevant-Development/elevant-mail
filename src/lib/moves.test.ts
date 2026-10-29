import { describe, expect, it } from "vitest";

import { canArchive, deletesForGood, moveTargets, nextAfter } from "./moves";
import type { Folder, FolderRole, MessageSummary } from "./tauri";

function folder(id: string, role: FolderRole | null, accountId = "acc"): Folder {
  return {
    id,
    accountId,
    name: id,
    path: id,
    pathNames: id.split("/"),
    role,
    unreadCount: 0,
    totalCount: 0,
  };
}

function message(id: string, folderId: string, accountId = "acc"): MessageSummary {
  return {
    id,
    accountId,
    folderId,
    subject: null,
    from: { name: null, address: "lena@example.org" },
    dateSent: 0,
    snippet: "",
    isRead: false,
    isFlagged: false,
    hasAttachments: false,
  };
}

const folders = [
  folder("INBOX", "inbox"),
  folder("Archive", "archive"),
  folder("Trash", "trash"),
  folder("Clients/Berger", null),
  folder("INBOX/Projects", null),
  folder("Clients", null),
  folder("other-inbox", "inbox", "other"),
];

describe("canArchive", () => {
  it("is off for mail that is archived already", () => {
    expect(canArchive(message("1", "INBOX"), folders)).toBe(true);
    expect(canArchive(message("1", "Archive"), folders)).toBe(false);
    expect(canArchive(message("1", "all"), [folder("all", "all")])).toBe(false);
  });
});

describe("deletesForGood", () => {
  it("in the trash, or without one", () => {
    expect(deletesForGood(message("1", "INBOX"), folders)).toBe(false);
    expect(deletesForGood(message("1", "Trash"), folders)).toBe(true);
    expect(deletesForGood(message("1", "other-inbox", "other"), folders)).toBe(true);
  });
});

describe("nextAfter", () => {
  const order = ["a", "b", "c", "d", "e"];

  it("opens the next one below", () => {
    expect(nextAfter(order, ["b"])).toBe("c");
    expect(nextAfter(order, ["b", "c"])).toBe("d");
    expect(nextAfter(order, ["d", "b"])).toBe("e");
  });

  it("goes up at the end and stops when nothing is left", () => {
    expect(nextAfter(order, ["e"])).toBe("d");
    expect(nextAfter(order, ["d", "e"])).toBe("c");
    expect(nextAfter(order, order)).toBeNull();
    expect(nextAfter(order, ["x"])).toBeNull();
  });
});

describe("moveTargets", () => {
  it("lists the other folders of the account, with their path", () => {
    const targets = moveTargets([message("1", "INBOX")], folders);
    expect(targets?.map((target) => target.label)).toEqual([
      "Archiv",
      "Papierkorb",
      "Clients",
      "Clients / Berger",
      "Posteingang / Projects",
    ]);
  });

  it("refuses mail from several accounts", () => {
    expect(
      moveTargets([message("1", "INBOX"), message("2", "other-inbox", "other")], folders),
    ).toBeNull();
  });
});
