import {
  Archive,
  FilePen,
  Folder as FolderIcon,
  Inbox,
  OctagonAlert,
  Send,
  Trash2,
} from "lucide-react";
import type { ReactNode } from "react";

import { SidebarItem } from "@/components/layout/SidebarItem";
import { folderLabel, sortFolders } from "@/lib/folders";
import type { Account, Folder, FolderRole } from "@/lib/tauri";

const roleIcons: Record<FolderRole, ReactNode> = {
  inbox: <Inbox />,
  drafts: <FilePen />,
  sent: <Send />,
  archive: <Archive />,
  junk: <OctagonAlert />,
  trash: <Trash2 />,
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
            className="mb-1 flex items-center gap-2 px-2 text-xs font-medium tracking-wide text-muted-foreground uppercase select-none"
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
                  icon={folder.role ? roleIcons[folder.role] : <FolderIcon />}
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
