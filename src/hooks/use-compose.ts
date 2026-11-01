import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";

import {
  type ComposeMode,
  forwardQuote,
  forwardSubject,
  replyQuote,
  replyRecipients,
  replySubject,
  withSignature,
} from "@/lib/compose";
import { type Address, api, UNIFIED_INBOX } from "@/lib/tauri";
import { useAccounts } from "@/queries/accounts";
import { useFolders } from "@/queries/folders";
import { useUiStore } from "@/stores/ui";

/**
 * Starts mail: a new one from the account of the open mailbox (or the one that opens at start),
 * or a reply or forward of a message, with recipients, subject and the quoted text filled in.
 * A reply or forward begun before opens again with what was written.
 */
export function useCompose() {
  const queryClient = useQueryClient();
  const { data: accounts = [] } = useAccounts();
  const { data: folders = [] } = useFolders();
  const openDraft = useUiStore((s) => s.openDraft);

  /** The account's signature, from its settings, and whether it was pasted as HTML. */
  const signatureOf = async (accountId: string) => {
    const settings = await queryClient.fetchQuery({
      queryKey: ["account-settings", accountId],
      queryFn: () => api.getAccountSettings(accountId),
    });
    return [settings.signature, settings.signatureHtml] as const;
  };

  /** A new mail, to `to` if given (e.g. from a contact). */
  const compose = async (to: Address[] = []) => {
    const { selectedMailboxId, startAccountId } = useUiStore.getState();
    const folderAccount =
      selectedMailboxId === UNIFIED_INBOX
        ? undefined
        : folders.find((f) => f.id === selectedMailboxId)?.accountId;
    const accountId =
      folderAccount ?? accounts.find((a) => a.id === startAccountId)?.id ?? accounts[0]?.id;
    if (!accountId) return;
    // Each new mail its own draft; the ones put aside wait in the drafts folder.
    openDraft({
      key: `new:${crypto.randomUUID()}`,
      mode: "new",
      sourceId: null,
      accountId,
      to,
      cc: [],
      bcc: [],
      subject: "",
      html: withSignature("", ...(await signatureOf(accountId))),
      text: "",
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
    const draft = {
      key,
      mode,
      sourceId: messageId,
      accountId: message.accountId,
      to,
      cc,
      bcc: [],
      subject: forward ? forwardSubject(message.subject) : replySubject(message.subject),
      html: withSignature(
        forward ? forwardQuote(message) : replyQuote(message),
        ...(await signatureOf(message.accountId)),
      ),
      // The composer fills it in from the editor.
      text: "",
    };
    openDraft(draft);
    // Forwarding takes the files along; the draft has to exist for them.
    if (forward && message.attachments.length > 0) {
      await api.saveDraft(draft);
      await api.forwardAttachments(key, messageId);
      void queryClient.invalidateQueries({ queryKey: ["draftAttachments", key] });
    }
  };

  return { compose, respond };
}

/**
 * At start: the drafts kept from before come back, and those changed since their last upload
 * go to the server (the backend skips the others).
 */
export function useKeptDrafts() {
  const loadDrafts = useUiStore((s) => s.loadDrafts);
  useEffect(() => {
    void api.listDrafts().then((drafts) => {
      loadDrafts(drafts);
      for (const draft of drafts) void api.uploadDraft(draft.key);
    });
  }, [loadDrafts]);
}

/**
 * When the composer closes or shows another draft, the one it showed is kept: uploaded to the
 * drafts folder, or thrown away if nothing was written. Sent and discarded drafts are gone by
 * then. Watches the store rather than the composer's lifetime, which React runs twice in
 * development.
 */
export function useDraftLeaving() {
  const queryClient = useQueryClient();
  useEffect(
    () =>
      useUiStore.subscribe((state, previous) => {
        const left = previous.composeKey;
        if (!left || left === state.composeKey) return;
        const draft = state.drafts[left];
        if (!draft) return;
        const files = queryClient.getQueryData<unknown[]>(["draftAttachments", left]) ?? [];
        const empty =
          draft.to.length + draft.cc.length + draft.bcc.length === 0 &&
          !draft.subject.trim() &&
          !draft.text.trim() &&
          files.length === 0;
        if (empty) {
          state.discardDraft(left);
          void api.discardDraft(left);
        } else {
          void api.saveDraft(draft).then(() => api.uploadDraft(left));
        }
      }),
    [queryClient],
  );
}
