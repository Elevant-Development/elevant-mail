import { useRef, useState } from "react";

import { AccountList } from "@/components/accounts/AccountList";
import { CalendarList } from "@/components/calendar/CalendarList";
import { AddressBookList } from "@/components/contacts/AddressBookList";
import { RemoveAccountDialog } from "@/components/accounts/RemoveAccountDialog";
import { Icon } from "@/components/common/Icon";
import { Button } from "@/components/ui/button";
import { useCompose } from "@/hooks/use-compose";
import { useSyncActions } from "@/hooks/use-sync";
import { useTreeFocus } from "@/hooks/use-tree-focus";
import { defaultCalendar, newEvent, nextFullHour } from "@/lib/calendar";
import { unifiedInboxes, unifiedInboxLabel } from "@/lib/folders";
import { t } from "@/lib/i18n";
import { type Account, UNIFIED_INBOX } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useAccounts } from "@/queries/accounts";
import { useCalendars } from "@/queries/calendar";
import { useAddressBooks } from "@/queries/contacts";
import { defaultAddressBook, newContact } from "@/lib/contacts";
import { useContactsStore } from "@/stores/contacts";
import { useFolders } from "@/queries/folders";
import { useCalendarStore } from "@/stores/calendar";
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
  const { compose } = useCompose();
  const treeRef = useRef<HTMLDivElement>(null);
  useTreeFocus(treeRef);
  const area = useCalendarStore((s) => s.area);
  const editEvent = useCalendarStore((s) => s.editEvent);
  const { data: calendars = [] } = useCalendars();
  const target = defaultCalendar(calendars);
  const { data: books = [] } = useAddressBooks();
  const shownBook = useContactsStore((s) => s.addressBookId);
  const editContact = useContactsStore((s) => s.editContact);
  const bookTarget = defaultAddressBook(books, shownBook);

  // Opening a folder other than the inbox fetches its mail (the inbox syncs in the background).
  const openMailbox = (mailboxId: string) => {
    selectMailbox(mailboxId);
    const folder = folders.find((f) => f.id === mailboxId);
    if (folder && folder.role !== "inbox") void syncFolder(folder);
  };

  const unifiedUnread = unifiedInboxes(folders, accounts).reduce(
    (sum, f) => sum + f.unreadCount,
    0,
  );

  return (
    <nav className={cn("flex flex-col bg-sidebar text-sidebar-foreground", className)}>
      <div className="px-2 pt-2">
        {area === "mail" ? (
          <Button
            className="w-full justify-start"
            disabled={accounts.length === 0}
            onClick={() => void compose()}
          >
            <Icon name="compose" />
            {t("compose.new")}
          </Button>
        ) : area === "contacts" ? (
          <Button
            className="w-full justify-start"
            disabled={!bookTarget}
            onClick={() => bookTarget && editContact(newContact(bookTarget.id))}
          >
            <Icon name="plus" />
            {t("contacts.new")}
          </Button>
        ) : (
          <Button
            className="w-full justify-start"
            disabled={!target}
            onClick={() =>
              target &&
              editEvent(
                newEvent(
                  nextFullHour(Date.now()),
                  false,
                  target.id,
                  useUiStore.getState().defaultReminder,
                ),
              )
            }
          >
            <Icon name="plus" />
            {t("calendar.newEvent")}
          </Button>
        )}
      </div>
      {area === "calendar" && (
        <div className="flex flex-1 flex-col overflow-y-auto p-2">
          <CalendarList />
        </div>
      )}
      {area === "contacts" && (
        <div className="flex flex-1 flex-col overflow-y-auto p-2">
          <AddressBookList />
        </div>
      )}
      <div
        ref={treeRef}
        // FOLDER_TREE_ATTRIBUTE and FOCUS_GROUP_ATTRIBUTE: one tab stop, F6 comes here.
        data-folder-tree=""
        data-focus-group=""
        // Stays mounted in the other areas, so the folders keep their keyboard handling.
        hidden={area !== "mail"}
        className="flex flex-1 flex-col gap-4 overflow-y-auto p-2"
      >
        {accounts.length > 1 && (
          <SidebarItem
            icon="unified"
            label={unifiedInboxLabel(accounts)}
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
