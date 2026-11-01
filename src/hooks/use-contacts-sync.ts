import { useQueryClient } from "@tanstack/react-query";

import { describeError } from "@/lib/errors";
import { oneAtATime } from "@/lib/one-at-a-time";
import { type AddressBook, api } from "@/lib/tauri";
import { useContactsStore } from "@/stores/contacts";

/** One sync per account at a time; asked for meanwhile, it runs once more after. */
const turn = oneAtATime();

/**
 * Syncs an account's address books over CardDAV: the changes made here go out, the server's come
 * in, and the views follow. A failure is kept per account for the address book list.
 */
export function useContactsSync() {
  const queryClient = useQueryClient();

  const syncContacts = (accountId: string) =>
    turn(accountId, async () => {
      const { setSyncError } = useContactsStore.getState();
      try {
        const report = await api.syncContacts(accountId);
        setSyncError(accountId, null);
        if (!report.changed) return;
      } catch (error) {
        setSyncError(accountId, describeError(error));
      }
      // Also after a failure: what came before it is in the cache.
      await Promise.all(
        ["address-books", "contacts", "contact", "addressSuggestions"].map((key) =>
          queryClient.invalidateQueries({ queryKey: [key] }),
        ),
      );
    });

  /** After a change in an address book: sends it to that address book's server right away. */
  const sendChanges = (addressBookId: string) => {
    const book = queryClient
      .getQueryData<AddressBook[]>(["address-books"])
      ?.find((b) => b.id === addressBookId);
    if (book?.accountId) void syncContacts(book.accountId);
  };

  return { syncContacts, sendChanges };
}
