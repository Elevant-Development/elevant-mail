import { useEffect, useRef } from "react";

import { Icon } from "@/components/common/Icon";
import { letterOf } from "@/lib/contacts";
import { t } from "@/lib/i18n";
import type { ContactSummary } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useContacts } from "@/queries/contacts";
import { useContactsStore } from "@/stores/contacts";

import { ContactAvatar } from "./ContactAvatar";

/**
 * The contacts of the address book shown, by name under their letters, with a search field.
 * `↑`/`↓` move between them, `/` goes to the search.
 */
export function ContactList({ className }: { className?: string }) {
  const addressBookId = useContactsStore((s) => s.addressBookId);
  const query = useContactsStore((s) => s.query);
  const setQuery = useContactsStore((s) => s.setQuery);
  const contactId = useContactsStore((s) => s.contactId);
  const openContact = useContactsStore((s) => s.openContact);
  const { data: contacts = [], isPending } = useContacts(addressBookId, query);
  const search = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLDivElement>(null);

  // The selected one comes into view, e.g. after moving with the keys.
  useEffect(() => {
    list.current
      ?.querySelector<HTMLElement>('[aria-current="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [contactId]);

  const move = (step: 1 | -1) => {
    const index = contacts.findIndex((contact) => contact.id === contactId);
    const next = contacts[Math.min(Math.max(index + step, 0), contacts.length - 1)];
    if (next) openContact(next.id);
  };

  const groups: { letter: string; contacts: ContactSummary[] }[] = [];
  for (const contact of contacts) {
    const letter = letterOf(contact.name);
    const last = groups[groups.length - 1];
    if (last?.letter === letter) last.contacts.push(contact);
    else groups.push({ letter, contacts: [contact] });
  }

  return (
    <section
      aria-label={t("area.contacts")}
      className={cn("flex min-h-0 flex-col", className)}
      onKeyDown={(event) => {
        if (event.target instanceof HTMLInputElement) {
          if (event.key === "ArrowDown") {
            event.preventDefault();
            list.current?.focus();
            if (!contactId) move(1);
          }
          return;
        }
        if (event.key === "ArrowDown" || event.key === "j") move(1);
        else if (event.key === "ArrowUp" || event.key === "k") move(-1);
        else if (event.key === "/") search.current?.focus();
        else return;
        event.preventDefault();
      }}
    >
      <div className="flex h-12 shrink-0 items-center gap-2 border-b px-3">
        <Icon name="search" className="size-4 shrink-0 text-muted-foreground" />
        <input
          ref={search}
          type="search"
          value={query}
          onChange={(input) => setQuery(input.target.value)}
          placeholder={t("contacts.search")}
          aria-label={t("contacts.search")}
          className="h-8 min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground"
        />
      </div>
      <div
        ref={list}
        tabIndex={0}
        role="listbox"
        aria-label={t("area.contacts")}
        className="min-h-0 flex-1 overflow-y-auto py-1 outline-none"
      >
        {!isPending && contacts.length === 0 && (
          <p className="px-4 py-8 text-center text-sm text-muted-foreground">
            {query.trim() ? t("contacts.noMatch", { query: query.trim() }) : t("contacts.empty")}
          </p>
        )}
        {groups.map((group) => (
          <div key={group.letter} role="group" aria-label={group.letter}>
            <div className="sticky top-0 z-10 bg-card/95 px-4 pt-2 pb-1 text-xs font-semibold text-muted-foreground backdrop-blur">
              {group.letter}
            </div>
            {group.contacts.map((contact) => {
              const selected = contact.id === contactId;
              return (
                <button
                  key={contact.id}
                  type="button"
                  role="option"
                  aria-selected={selected}
                  aria-current={selected}
                  tabIndex={-1}
                  onClick={() => openContact(contact.id)}
                  className={cn(
                    "mx-1 flex w-[calc(100%-0.5rem)] items-center gap-3 rounded-lg px-3 py-1.5 text-left transition-colors hover:bg-accent/60",
                    selected && "bg-accent hover:bg-accent",
                  )}
                >
                  <ContactAvatar name={contact.name} className="size-8 text-xs" />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-sm font-medium">{contact.name}</span>
                    {(contact.company || contact.email) && (
                      <span className="block truncate text-xs text-muted-foreground">
                        {contact.company && contact.company !== contact.name
                          ? contact.company
                          : contact.email}
                      </span>
                    )}
                  </span>
                </button>
              );
            })}
          </div>
        ))}
      </div>
    </section>
  );
}
