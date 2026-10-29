import { describe, expect, it } from "vitest";

import { buildFolderTree, type FolderNode, shownUnread, unreadBelow } from "./folders";
import type { Folder, FolderRole } from "./tauri";

function folder(path: string, role: FolderRole | null = null, unreadCount = 0): Folder {
  const pathNames = path.split("/");
  return {
    id: path,
    accountId: "acc",
    name: pathNames[pathNames.length - 1] ?? path,
    path,
    pathNames,
    role,
    unreadCount,
    totalCount: 0,
  };
}

const shape = (nodes: FolderNode[]): unknown[] =>
  nodes.map((node) => (node.children.length > 0 ? [node.label, shape(node.children)] : node.label));

describe("buildFolderTree", () => {
  it("lifts Gmail's special folders out of [Gmail] and groups the rest", () => {
    const tree = buildFolderTree([
      folder("INBOX", "inbox"),
      folder("[Gmail]/Gesendet", "sent"),
      folder("[Gmail]/Alle Nachrichten", "all"),
      folder("[Gmail]/Wichtig"),
      folder("[Gmail]/Markiert"),
      folder("Notes"),
    ]);
    expect(shape(tree)).toEqual([
      "Posteingang",
      "Gesendet",
      "Alle Nachrichten",
      ["Gmail", ["Markiert", "Wichtig"]],
      "Notes",
    ]);
    expect(tree[3]?.folder).toBeNull();
  });

  it("keeps subfolders under their parent, also under the inbox", () => {
    const tree = buildFolderTree([
      folder("INBOX", "inbox"),
      folder("INBOX/Projekte"),
      folder("INBOX/Projekte/2026", null, 3),
      folder("Kunden/Berger", null, 1),
      folder("Kunden"),
    ]);
    expect(shape(tree)).toEqual([
      ["Posteingang", [["Projekte", ["2026"]]]],
      ["Kunden", ["Berger"]],
    ]);
    expect(unreadBelow(tree[0]!)).toBe(3);
    // A listed parent is a real folder, not a group.
    expect(tree[1]?.folder?.path).toBe("Kunden");
  });

  it("names the server folder where a special folder and another share a label", () => {
    const tree = buildFolderTree([
      folder("INBOX", "inbox"),
      folder("Sent", "sent"),
      folder("Gesendet"),
      folder("Papierkorb", "trash"),
      folder("Trash"),
    ]);
    const byLabel = tree.map((node) => [node.label, node.detail]);
    expect(byLabel).toEqual([
      ["Posteingang", undefined],
      ["Gesendet", "Sent"],
      ["Papierkorb", undefined],
      ["Gesendet", undefined],
      ["Trash", undefined],
    ]);
  });

  it("keeps quiet about unread spam, trash and Gmail's All Mail", () => {
    expect(shownUnread(folder("Spam", "junk", 416))).toBe(0);
    expect(shownUnread(folder("[Gmail]/Alle Nachrichten", "all", 11144))).toBe(0);
    expect(shownUnread(folder("INBOX", "inbox", 12))).toBe(12);
    expect(shownUnread(folder("Kunden", null, 3))).toBe(3);
  });
});
