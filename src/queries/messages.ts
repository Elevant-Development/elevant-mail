import { keepPreviousData, useQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo } from "react";

import { useDebouncedValue } from "@/hooks/use-debounced-value";
import { api, type Conversation, type Message } from "@/lib/tauri";
import { type ListFilter, useUiStore } from "@/stores/ui";

export function useMessages(mailboxId: string) {
  const threads = useUiStore((s) => s.threads);
  return useQuery({
    queryKey: ["messages", mailboxId, threads],
    queryFn: () => api.listMessages(mailboxId, threads),
  });
}

/** The messages the reader shows for the open row, newest first. */
export function useConversation(messageId: string | null) {
  const threads = useUiStore((s) => s.threads);
  return useQuery({
    queryKey: ["conversation", messageId, threads],
    queryFn: () => (messageId ? api.getConversation(messageId, threads) : []),
    // A reply that arrives moves the row to its newest message; the reader stays meanwhile.
    placeholderData: keepPreviousData,
    enabled: messageId !== null,
  });
}

export function useMessage(messageId: string | null) {
  const remoteImages = useUiStore((s) =>
    messageId ? (s.remoteImagesAllowed[messageId] ?? false) : false,
  );
  return useQuery({
    queryKey: ["message", messageId, remoteImages],
    queryFn: () => (messageId ? api.getMessage(messageId, remoteImages) : null),
    // Allowing images refetches; keep showing the mail meanwhile.
    placeholderData: (previous) => (previous?.id === messageId ? previous : undefined),
    enabled: messageId !== null,
  });
}

/**
 * Large mail is cached with headers only. Opening it fetches the full body from the server,
 * after which the message (and its preview line in the list) is read from the cache again.
 */
export function useFullBody(message: Message | null | undefined) {
  const queryClient = useQueryClient();
  const messageId = message?.id ?? null;
  return useQuery({
    queryKey: ["messageBody", messageId],
    queryFn: async () => {
      if (!messageId) return null;
      await api.loadMessageBody(messageId);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["message", messageId] }),
        queryClient.invalidateQueries({ queryKey: ["messages"] }),
        queryClient.invalidateQueries({ queryKey: ["conversation"] }),
      ]);
      return null;
    },
    enabled: message != null && !message.bodyComplete,
    retry: false,
  });
}

export function useSearch(query: string) {
  const threads = useUiStore((s) => s.threads);
  // What the servers found for this query (SearchOnServer) comes along.
  const also = useUiStore((s) => (s.serverHits?.query === query ? s.serverHits.ids : NONE));
  return useQuery({
    queryKey: ["search", query, threads, also],
    queryFn: () => api.searchMessages(query, threads, also),
    enabled: query.trim() !== "",
    // The previous results stay while the next word is typed.
    placeholderData: keepPreviousData,
  });
}

const NONE: string[] = [];

/** The search field's text as the list searches it: trimmed and a moment after typing. */
export function useSearchQuery() {
  return useDebouncedValue(
    useUiStore((s) => s.searchQuery.trim()),
    150,
  );
}

const matches: Record<ListFilter, (row: Conversation) => boolean> = {
  unread: (row) => !row.isRead,
  flagged: (row) => row.isFlagged,
  attachment: (row) => row.hasAttachments,
};

/**
 * What the list shows: search results while the search field has text, otherwise the selected
 * mailbox, narrowed by the filter bar. The list, the keyboard shortcuts and the actions all use
 * this, so they always agree. A selected row stays while it is selected, so reading an unread
 * mail does not pull it away under the "unread" filter.
 */
export function useVisibleMessages() {
  const mailboxId = useUiStore((s) => s.selectedMailboxId);
  const query = useSearchQuery();
  const filters = useUiStore((s) => s.listFilters);
  const selectedIds = useUiStore((s) => s.selectedIds);
  const mailbox = useMessages(mailboxId);
  const search = useSearch(query);
  const result = query === "" ? { ...mailbox, searching: false } : { ...search, searching: true };
  const data = useMemo(
    () =>
      filters.length === 0
        ? result.data
        : result.data?.filter(
            (row) =>
              filters.every((filter) => matches[filter](row)) || selectedIds.includes(row.id),
          ),
    [result.data, filters, selectedIds],
  );
  return { ...result, data, filtered: filters.length > 0 };
}
