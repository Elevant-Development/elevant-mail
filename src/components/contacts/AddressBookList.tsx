import { Icon, type IconName } from "@/components/common/Icon";
import { addressBookName } from "@/lib/contacts";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { useAccounts } from "@/queries/accounts";
import { useAddressBooks } from "@/queries/contacts";
import { useContactsStore } from "@/stores/contacts";

import { UnlockContacts } from "./UnlockContacts";

function Row({
  icon,
  label,
  detail,
  selected,
  onSelect,
}: {
  icon: IconName;
  label: string;
  detail?: string;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      aria-current={selected ? "page" : undefined}
      onClick={onSelect}
      className={cn(
        "flex h-9 w-full items-center gap-2.5 rounded-md px-2 text-left text-sm text-sidebar-foreground transition-colors select-none hover:bg-sidebar-accent/70 [&_svg]:size-4 [&_svg]:shrink-0 [&_svg]:text-muted-foreground",
        selected &&
          "bg-sidebar-accent text-sidebar-accent-foreground hover:bg-sidebar-accent [&_svg]:text-sidebar-primary",
      )}
    >
      <Icon name={icon} />
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {detail && <span className="shrink-0 text-[11px] text-muted-foreground">{detail}</span>}
    </button>
  );
}

/** All contacts together, then the address books per account; the one on this computer last. */
export function AddressBookList() {
  const { data: books = [] } = useAddressBooks();
  const { data: accounts = [] } = useAccounts();
  const shown = useContactsStore((s) => s.addressBookId);
  const show = useContactsStore((s) => s.showAddressBook);
  const errors = useContactsStore((s) => s.syncErrors);
  const groups = [
    ...accounts.map((account) => ({
      key: account.id,
      label: account.displayName,
      error: errors[account.id],
      books: books.filter((book) => book.accountId === account.id),
    })),
    {
      key: "local",
      label: t("calendar.thisComputer"),
      error: undefined,
      books: books.filter((book) => book.accountId === null),
    },
  ].filter((group) => group.books.length > 0 || group.error);

  return (
    <div className="flex flex-col gap-4">
      <Row
        icon="unified"
        label={t("contacts.all")}
        selected={shown === null}
        onSelect={() => show(null)}
      />
      {groups.map((group) => (
        <section key={group.key} aria-label={group.label}>
          <h3 className="mb-1 px-2 text-xs font-medium text-muted-foreground">{group.label}</h3>
          {group.books.map((book) => (
            <Row
              key={book.id}
              icon="contacts"
              label={addressBookName(book)}
              detail={book.readOnly ? t("calendar.readOnly") : undefined}
              selected={shown === book.id}
              onSelect={() => show(book.id)}
            />
          ))}
          {group.error?.kind === "contactsAccessMissing" && (
            <div className="grid gap-2 px-2 py-1">
              <p className="text-xs text-muted-foreground">{t("contacts.unlockHint")}</p>
              <UnlockContacts accountId={group.key} />
            </div>
          )}
          {group.error && group.error.kind !== "contactsAccessMissing" && (
            <p className="flex items-start gap-2 px-2 py-1 text-xs text-destructive">
              <Icon name="alert" className="mt-px size-3.5 shrink-0" />
              <span>{group.error.message}</span>
            </p>
          )}
        </section>
      ))}
    </div>
  );
}
