import { Icon, type IconName } from "@/components/common/Icon";
import { SidebarItem } from "@/components/layout/SidebarItem";
import { folderLabel, sortFolders } from "@/lib/folders";
import { t } from "@/lib/i18n";
import type { Account, Folder, FolderRole } from "@/lib/tauri";

const roleIcons: Record<FolderRole, IconName> = {
  inbox: "inbox",
  drafts: "drafts",
  sent: "sent",
  archive: "archive",
  junk: "junk",
  trash: "trash",
  all: "archive",
};

interface AccountListProps {
  accounts: Account[];
  folders: Folder[];
  selectedMailboxId: string;
  onSelectMailbox: (mailboxId: string) => void;
  onRemoveAccount: (account: Account) => void;
}

/** One section per account with its folders. */
export function AccountList({
  accounts,
  folders,
  selectedMailboxId,
  onSelectMailbox,
  onRemoveAccount,
}: AccountListProps) {
  return (
    <div className="flex flex-col gap-4">
      {accounts.map((account) => (
        <section key={account.id} aria-labelledby={`account-${account.id}`}>
          <div className="group/account mb-1 flex h-6 items-center gap-2 px-2">
            <h3
              id={`account-${account.id}`}
              title={account.emailAddress}
              className="flex min-w-0 flex-1 items-center gap-2 text-xs font-medium text-muted-foreground select-none"
            >
              <span
                aria-hidden
                className="size-2 shrink-0 rounded-full"
                style={{ backgroundColor: account.color }}
              />
              <span className="truncate">{account.displayName}</span>
            </h3>
            <button
              type="button"
              aria-label={t("account.remove.action", { name: account.displayName })}
              title={t("account.remove.action", { name: account.displayName })}
              onClick={() => onRemoveAccount(account)}
              className="flex size-6 items-center justify-center rounded-md text-muted-foreground opacity-0 transition-opacity group-hover/account:opacity-100 hover:bg-sidebar-accent hover:text-foreground focus-visible:opacity-100 [&_svg]:size-3.5"
            >
              <Icon name="trash" />
            </button>
          </div>
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
