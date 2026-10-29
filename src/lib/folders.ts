import { locale, t } from "./i18n";
import type { Account, Folder, FolderRole } from "./tauri";
import { UNIFIED_INBOX } from "./tauri";

const roleOrder: FolderRole[] = ["inbox", "drafts", "sent", "archive", "all", "junk", "trash"];

/** Translated name for special folders, the server's name for all others. */
export function folderLabel(folder: Folder): string {
  return folder.role ? t(`folder.${folder.role}`) : folder.name;
}

/** Special folders in a fixed order, then all others alphabetically. */
export function sortFolders(folders: Folder[]): Folder[] {
  const rank = (folder: Folder) =>
    folder.role ? roleOrder.indexOf(folder.role) : roleOrder.length;
  return [...folders].sort(
    (a, b) => rank(a) - rank(b) || folderLabel(a).localeCompare(folderLabel(b), locale),
  );
}

/** A folder in the sidebar tree, or a group the server lists without mail of its own. */
export interface FolderNode {
  /** Stable across syncs: account and path, for remembering what is collapsed. */
  key: string;
  label: string;
  /** The server's name of a special folder, when another folder already has its label. */
  detail?: string;
  folder: Folder | null;
  children: FolderNode[];
}

/** "[Gmail]" reads as "Gmail". */
function groupLabel(segment: string): string {
  return segment.replace(/^\[(.*)\]$/, "$1");
}

/**
 * The folders of one account as a tree by their paths. Special folders always sit at the top
 * level, even where the server nests them (Gmail keeps "Gesendet" under "[Gmail]"); their own
 * subfolders stay beneath them ("INBOX/Projekte" under the inbox).
 */
export function buildFolderTree(folders: Folder[]): FolderNode[] {
  const top: FolderNode[] = [];
  const byPath = new Map<string, FolderNode>();
  const pathKey = (names: string[]) => names.join("\u0000");
  const keyFor = (folder: Folder, names: string[]) => `${folder.accountId}:${names.join("/")}`;

  // web.de, for example, marks "Sent" as the sent folder and also keeps an old folder named
  // "Gesendet": both would read "Gesendet".
  const plainNames = new Set(
    folders.filter((f) => !f.role).map((f) => f.name.toLocaleLowerCase(locale)),
  );
  const detailFor = (folder: Folder) => {
    if (!folder.role) return undefined;
    const label = folderLabel(folder);
    const clash = plainNames.has(label.toLocaleLowerCase(locale));
    return clash && folder.name !== label ? folder.name : undefined;
  };

  const byDepth = [...folders].sort(
    (a, b) => a.pathNames.length - b.pathNames.length || a.path.localeCompare(b.path),
  );
  for (const folder of byDepth) {
    const node: FolderNode = {
      key: keyFor(folder, folder.pathNames),
      label: folderLabel(folder),
      detail: detailFor(folder),
      folder,
      children: [],
    };
    byPath.set(pathKey(folder.pathNames), node);
    if (folder.role) {
      top.push(node);
      continue;
    }
    // Under the nearest listed parent; missing parents become groups.
    let parent: FolderNode | null = null;
    for (let depth = 1; depth < folder.pathNames.length; depth++) {
      const names = folder.pathNames.slice(0, depth);
      let group = byPath.get(pathKey(names));
      if (!group) {
        group = {
          key: keyFor(folder, names),
          label: groupLabel(names[names.length - 1] ?? ""),
          folder: null,
          children: [],
        };
        byPath.set(pathKey(names), group);
        (parent ? parent.children : top).push(group);
      }
      parent = group;
    }
    (parent ? parent.children : top).push(node);
  }

  const rank = (node: FolderNode) =>
    node.folder?.role ? roleOrder.indexOf(node.folder.role) : roleOrder.length;
  const sort = (nodes: FolderNode[]): FolderNode[] =>
    nodes
      .sort((a, b) => rank(a) - rank(b) || a.label.localeCompare(b.label, locale))
      .map((node) => ({ ...node, children: sort(node.children) }));
  return sort(top);
}

/** Folders whose unread count says nothing: spam, deleted, own mail, and Gmail's "All Mail". */
const quietRoles: FolderRole[] = ["junk", "trash", "sent", "drafts", "all"];

/** The unread count the sidebar shows for a folder: none for the quiet ones. */
export function shownUnread(folder: Folder | null): number {
  if (!folder || (folder.role && quietRoles.includes(folder.role))) return 0;
  return folder.unreadCount;
}

/** Unread mail in a node's subfolders, shown on a collapsed node. */
export function unreadBelow(node: FolderNode): number {
  return node.children.reduce(
    (sum, child) => sum + shownUnread(child.folder) + unreadBelow(child),
    0,
  );
}

/** Heading for the message list. Names the account when there is more than one. */
export function mailboxTitle(mailboxId: string, folders: Folder[], accounts: Account[]): string {
  if (mailboxId === UNIFIED_INBOX) return t("sidebar.unifiedInbox");
  const folder = folders.find((f) => f.id === mailboxId);
  if (!folder) return "";
  const label = folderLabel(folder);
  const account = accounts.length > 1 && accounts.find((a) => a.id === folder.accountId);
  return account ? `${label} · ${account.displayName}` : label;
}
