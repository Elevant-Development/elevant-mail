import { type IconName } from "@/components/common/Icon";
import { SidebarItem } from "@/components/layout/SidebarItem";
import { folderLabel, sortFolders } from "@/lib/folders";
import type { Account, Folder, FolderRole } from "@/lib/tauri";

const roleIcons: Record<FolderRole, IconName> = {
  inbox: "inbox",
  drafts: "drafts",
  sent: "sent",
  archive: "archive",
  junk: "junk",
  trash: "trash",
};

interface AccountListProps {
  accounts: Account[];
  folders: Folder[];
  selectedMailboxId: string;
  onSelectMailbox: (mailboxId: string) => void;
}

/** One section per account with its folders. */
export function AccountList({
  accounts,
  folders,
  selectedMailboxId,
  onSelectMailbox,
}: AccountListProps) {
  return (
    <div className="flex flex-col gap-4">
      {accounts.map((account) => (
        <section key={account.id} aria-labelledby={`account-${account.id}`}>
          <h3
            id={`account-${account.id}`}
            title={account.emailAddress}
            className="mb-1 flex items-center gap-2 px-2 text-xs font-medium text-muted-foreground select-none"
          >
            <span
              aria-hidden
              className="size-2 rounded-full"
              style={{ backgroundColor: account.color }}
            />
            <span className="truncate">{account.displayName}</span>
          </h3>
          <ul className="flex flex-col gap-px">
            {sortFolders(folders.filter((f) => f.accountId === account.id)).map((folder) => (
              <li key={folder.id}>
                <SidebarItem
                  icon={folder.role ? roleIcons[folder.role] : "folder"}
                  label={folderLabel(folder)}
                  unreadCount={folder.unreadCount}
                  selected={folder.id === selectedMailboxId}
                  onSelect={() => onSelectMailbox(folder.id)}
                />
              </li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}
