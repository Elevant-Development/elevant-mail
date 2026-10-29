import { useQuery, useQueryClient } from "@tanstack/react-query";

import { api, type Message } from "@/lib/tauri";

export function useMessages(mailboxId: string) {
  return useQuery({
    queryKey: ["messages", mailboxId],
    queryFn: () => api.listMessages(mailboxId),
  });
}

export function useMessage(messageId: string | null) {
  return useQuery({
    queryKey: ["message", messageId],
    queryFn: () => (messageId ? api.getMessage(messageId) : null),
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
      ]);
      return null;
    },
    enabled: message != null && !message.bodyComplete,
    retry: false,
  });
}
