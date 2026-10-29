import { useState } from "react";

import { AccountList } from "@/components/accounts/AccountList";
import { RemoveAccountDialog } from "@/components/accounts/RemoveAccountDialog";
import { Icon } from "@/components/common/Icon";
import { Button } from "@/components/ui/button";
import { useSyncActions } from "@/hooks/use-sync";
import { t } from "@/lib/i18n";
import { type Account, UNIFIED_INBOX } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useAccounts } from "@/queries/accounts";
import { useFolders } from "@/queries/folders";
import { useUiStore } from "@/stores/ui";

import { SidebarItem } from "./SidebarItem";

interface SidebarProps {
  className?: string;
}

export function Sidebar({ className }: SidebarProps) {
  const { data: accounts = [] } = useAccounts();
  const { data: folders = [] } = useFolders();
  const selectedMailboxId = useUiStore((s) => s.selectedMailboxId);
  const selectMailbox = useUiStore((s) => s.selectMailbox);
  const setAccountDialogOpen = useUiStore((s) => s.setAccountDialogOpen);
  const [accountToRemove, setAccountToRemove] = useState<Account | null>(null);
  const { syncFolder } = useSyncActions();

  // Opening a folder other than the inbox fetches its mail (the inbox syncs in the background).
  const openMailbox = (mailboxId: string) => {
    selectMailbox(mailboxId);
    const folder = folders.find((f) => f.id === mailboxId);
    if (folder && folder.role !== "inbox") void syncFolder(folder);
  };

  const unifiedUnread = folders
    .filter((f) => f.role === "inbox")
    .reduce((sum, f) => sum + f.unreadCount, 0);

  return (
    <nav className={cn("flex flex-col bg-sidebar text-sidebar-foreground", className)}>
      <div className="flex flex-1 flex-col gap-4 overflow-y-auto p-2">
        {accounts.length > 1 && (
          <SidebarItem
            icon="unified"
            label={t("sidebar.unifiedInbox")}
            unreadCount={unifiedUnread}
            selected={selectedMailboxId === UNIFIED_INBOX}
            onSelect={() => selectMailbox(UNIFIED_INBOX)}
          />
        )}
        <AccountList
          accounts={accounts}
          folders={folders}
          selectedMailboxId={selectedMailboxId}
          onSelectMailbox={openMailbox}
          onRemoveAccount={setAccountToRemove}
        />
      </div>
      <div className="border-t border-sidebar-border p-2">
        <Button
          variant="ghost"
          size="sm"
          className="w-full justify-start"
          onClick={() => setAccountDialogOpen(true)}
        >
          <Icon name="plus" />
          {t("sidebar.addAccount")}
        </Button>
      </div>
      <RemoveAccountDialog account={accountToRemove} onClose={() => setAccountToRemove(null)} />
    </nav>
  );
}
