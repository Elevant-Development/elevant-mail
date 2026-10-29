import { useQuery } from "@tanstack/react-query";

import { api } from "@/lib/tauri";

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
