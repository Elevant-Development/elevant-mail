import { Mails, Plus } from "lucide-react";

import { AccountList } from "@/components/accounts/AccountList";
import { Button } from "@/components/ui/button";
import { t } from "@/lib/i18n";
import { UNIFIED_INBOX } from "@/lib/tauri";
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

  const unifiedUnread = folders
    .filter((f) => f.role === "inbox")
    .reduce((sum, f) => sum + f.unreadCount, 0);

  return (
    <nav className={cn("flex flex-col bg-sidebar text-sidebar-foreground", className)}>
      <div className="flex flex-1 flex-col gap-4 overflow-y-auto p-2">
        {accounts.length > 1 && (
          <SidebarItem
            icon={<Mails />}
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
          onSelectMailbox={selectMailbox}
        />
      </div>
      <div className="border-t border-sidebar-border p-2">
        <Button variant="ghost" size="sm" className="w-full justify-start" disabled>
          <Plus />
          {t("sidebar.addAccount")}
        </Button>
      </div>
    </nav>
  );
}
