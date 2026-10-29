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

/** Heading for the message list. Names the account when there is more than one. */
export function mailboxTitle(mailboxId: string, folders: Folder[], accounts: Account[]): string {
  if (mailboxId === UNIFIED_INBOX) return t("sidebar.unifiedInbox");
  const folder = folders.find((f) => f.id === mailboxId);
  if (!folder) return "";
  const label = folderLabel(folder);
  const account = accounts.length > 1 && accounts.find((a) => a.id === folder.accountId);
  return account ? `${label} · ${account.displayName}` : label;
}
