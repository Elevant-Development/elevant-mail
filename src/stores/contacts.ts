/** UI state of the contacts area: the address book shown, the contact open, search and dialog. */
import { create } from "zustand";

import type { UserError } from "@/lib/errors";
import type { ContactInput } from "@/lib/tauri";

interface ContactsState {
  /** The address book shown; `null` for all of them together. */
  addressBookId: string | null;
  /** The contact whose details show. */
  contactId: string | null;
  query: string;
  /** The contact being written, new or changed. */
  editing: ContactInput | null;
  /** Why the last contacts sync of an account failed; missing after one that worked. */
  syncErrors: Record<string, UserError | undefined>;

  showAddressBook: (addressBookId: string | null) => void;
  openContact: (contactId: string | null) => void;
  setQuery: (query: string) => void;
  editContact: (contact: ContactInput) => void;
  closeDialog: () => void;
  setSyncError: (accountId: string, error: UserError | null) => void;
}

export const useContactsStore = create<ContactsState>()((set) => ({
  addressBookId: null,
  contactId: null,
  query: "",
  editing: null,
  syncErrors: {},

  showAddressBook: (addressBookId) => set({ addressBookId, contactId: null }),
  openContact: (contactId) => set({ contactId }),
  setQuery: (query) => set({ query }),
  editContact: (contact) => set({ editing: contact }),
  closeDialog: () => set({ editing: null }),
  setSyncError: (accountId, error) =>
    set((state) => ({ syncErrors: { ...state.syncErrors, [accountId]: error ?? undefined } })),
}));
