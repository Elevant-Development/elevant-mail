/** What archiving, deleting and moving would do with a message, for buttons and shortcuts. */
import { folderLabel, sortFolders } from "./folders";
import { locale, t } from "./i18n";
import type { Folder, MessageSummary } from "./tauri";

function folderOf(message: MessageSummary, folders: Folder[]): Folder | undefined {
  return folders.find((f) => f.id === message.folderId);
}

/** Mail in the archive, or in Gmail's "All Mail", is archived already. */
export function canArchive(message: MessageSummary, folders: Folder[]): boolean {
  const role = folderOf(message, folders)?.role;
  return role !== "archive" && role !== "all";
}

/** In the trash, or in an account without one: deleting is for good. */
export function deletesForGood(message: MessageSummary, folders: Folder[]): boolean {
  if (folderOf(message, folders)?.role === "trash") return true;
  return !folders.some((f) => f.accountId === message.accountId && f.role === "trash");
}

/**
 * The message to open once `removed` leave the list: the next one below the last of them, or
 * the one above if they were at the end.
 */
export function nextAfter(order: string[], removed: string[]): string | null {
  const gone = new Set(removed);
  const last = Math.max(...removed.map((id) => order.indexOf(id)));
  if (last < 0) return null;
  const below = order.slice(last + 1).find((id) => !gone.has(id));
  if (below) return below;
  return (
    order
      .slice(0, last)
      .reverse()
      .find((id) => !gone.has(id)) ?? null
  );
}

/** "INBOX/Projekte" reads "Posteingang / Projekte", "[Gmail]/Wichtig" reads "Gmail / Wichtig". */
function pathLabel(folder: Folder): string {
  return folder.pathNames
    .map((name, index) =>
      index === 0 && name.toUpperCase() === "INBOX"
        ? t("folder.inbox")
        : name.replace(/^\[(.*)\]$/, "$1"),
    )
    .join(" / ");
}

/**
 * Where messages can move to: the other folders of their one account, special folders first,
 * the others by their full path. Null for messages of several accounts.
 */
export function moveTargets(
  messages: MessageSummary[],
  folders: Folder[],
): { folder: Folder; label: string }[] | null {
  const accounts = new Set(messages.map((m) => m.accountId));
  if (accounts.size !== 1) return null;
  const [accountId] = accounts;
  const from = new Set(messages.map((m) => m.folderId));
  const targets = sortFolders(
    folders.filter((f) => f.accountId === accountId && !(from.size === 1 && from.has(f.id))),
  ).map((folder) => ({ folder, label: folder.role ? folderLabel(folder) : pathLabel(folder) }));
  const special = targets.filter((target) => target.folder.role);
  const plain = targets
    .filter((target) => !target.folder.role)
    .sort((a, b) => a.label.localeCompare(b.label, locale));
  return [...special, ...plain];
}
