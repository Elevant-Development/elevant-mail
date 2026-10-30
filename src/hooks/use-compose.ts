import { useQueryClient } from "@tanstack/react-query";

import {
  type ComposeMode,
  forwardQuote,
  forwardSubject,
  replyQuote,
  replyRecipients,
  replySubject,
} from "@/lib/compose";
import { api, UNIFIED_INBOX } from "@/lib/tauri";
import { useAccounts } from "@/queries/accounts";
import { useFolders } from "@/queries/folders";
import { useUiStore } from "@/stores/ui";

/**
 * Starts mail: a new one from the account of the open mailbox (or the one that opens at start),
 * or a reply or forward of a message, with recipients, subject and the quoted text filled in.
 * A draft that exists for the same purpose opens again with what was written.
 */
export function useCompose() {
  const queryClient = useQueryClient();
  const { data: accounts = [] } = useAccounts();
  const { data: folders = [] } = useFolders();
  const openDraft = useUiStore((s) => s.openDraft);

  const compose = () => {
    const { selectedMailboxId, startAccountId } = useUiStore.getState();
    const folderAccount =
      selectedMailboxId === UNIFIED_INBOX
        ? undefined
        : folders.find((f) => f.id === selectedMailboxId)?.accountId;
    const accountId =
      folderAccount ?? accounts.find((a) => a.id === startAccountId)?.id ?? accounts[0]?.id;
    if (!accountId) return;
    openDraft({
      key: "new",
      mode: "new",
      sourceId: null,
      accountId,
      to: [],
      cc: [],
      bcc: [],
      subject: "",
      html: "",
    });
  };

  const respond = async (messageId: string, mode: Exclude<ComposeMode, "new">) => {
    const key = `${mode}:${messageId}`;
    const existing = useUiStore.getState().drafts[key];
    if (existing) {
      openDraft(existing);
      return;
    }
    const message = await queryClient.fetchQuery({
      queryKey: ["message", messageId, false],
      queryFn: () => api.getMessage(messageId, false),
    });
    if (!message) return;
    const own = new Set(accounts.map((a) => a.emailAddress.toLowerCase()));
    const forward = mode === "forward";
    const { to, cc } = forward
      ? { to: [], cc: [] }
      : replyRecipients(message, mode === "replyAll", own);
    openDraft({
      key,
      mode,
      sourceId: messageId,
      accountId: message.accountId,
      to,
      cc,
      bcc: [],
      subject: forward ? forwardSubject(message.subject) : replySubject(message.subject),
      html: forward ? forwardQuote(message) : replyQuote(message),
    });
  };

  return { compose, respond };
}
