import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { useContactsSync } from "@/hooks/use-contacts-sync";
import { api, type ContactInput } from "@/lib/tauri";

export function useAddressBooks() {
  return useQuery({ queryKey: ["address-books"], queryFn: () => api.listAddressBooks() });
}

/** The contacts of one address book or of all (`null`), narrowed by `query`. */
export function useContacts(addressBookId: string | null, query: string) {
  return useQuery({
    queryKey: ["contacts", addressBookId, query],
    queryFn: () => api.listContacts(addressBookId, query),
    placeholderData: keepPreviousData,
  });
}

export function useContact(contactId: string | null) {
  return useQuery({
    queryKey: ["contact", contactId],
    queryFn: () => (contactId ? api.getContact(contactId) : null),
    enabled: contactId !== null,
    retry: false,
  });
}

export function useSaveContact() {
  const queryClient = useQueryClient();
  const { sendChanges } = useContactsSync();
  return useMutation({
    mutationFn: (contact: ContactInput) => api.saveContact(contact),
    onSuccess: (_, contact) => {
      sendChanges(contact.addressBookId);
      return Promise.all(
        ["contacts", "contact", "addressSuggestions"].map((key) =>
          queryClient.invalidateQueries({ queryKey: [key] }),
        ),
      );
    },
  });
}

export function useDeleteContact() {
  const queryClient = useQueryClient();
  const { sendChanges } = useContactsSync();
  return useMutation({
    mutationFn: ({ contactId }: { contactId: string; addressBookId: string }) =>
      api.deleteContact(contactId),
    // Asking for the deleted contact again would only fail.
    onSuccess: (_, { addressBookId }) => {
      sendChanges(addressBookId);
      return Promise.all(
        ["contacts", "addressSuggestions"].map((key) =>
          queryClient.invalidateQueries({ queryKey: [key] }),
        ),
      );
    },
  });
}
