import { useQueryClient } from "@tanstack/react-query";

import { describeError } from "@/lib/errors";
import { folderLabel } from "@/lib/folders";
import { t, tCount } from "@/lib/i18n";
import { deletesForGood, nextAfter } from "@/lib/moves";
import { api, type Folder, type MoveOutcome, UNDO_WINDOW_MS } from "@/lib/tauri";
import { useFolders } from "@/queries/folders";
import { useVisibleMessages } from "@/queries/messages";
import { useSyncStore } from "@/stores/sync";
import { useUiStore } from "@/stores/ui";
import { useUndoStore } from "@/stores/undo";

/**
 * Read and flagged state, archiving, deleting and moving. The backend changes the cache at once,
 * the lists are reread, and the change goes to the server in the background: read and flagged
 * state right away, moves once they can no longer be undone. A failure shows on the account
 * like a sync error; the change stays queued, or is undone if the server refused it.
 */
export function useMessageActions() {
  const queryClient = useQueryClient();
  const { data: messages = [] } = useVisibleMessages();
  const { data: folders = [] } = useFolders();

  const refresh = () =>
    Promise.all(
      ["messages", "message", "folders", "search"].map((key) =>
        queryClient.invalidateQueries({ queryKey: [key] }),
      ),
    );

  const push = (accountIds: string[]) => {
    for (const accountId of accountIds) {
      api
        .pushActions(accountId)
        // Moved mail has arrived in its new folder, which the backend fetched right away.
        .then(refresh)
        .catch(async (error: unknown) => {
          useSyncStore.getState().setError(accountId, describeError(error));
          // A refused change was undone in the cache.
          await refresh();
        });
    }
  };

  const run = async (change: () => Promise<string[]>) => {
    const accountIds = await change();
    await refresh();
    push(accountIds);
  };

  /** Errors of archiving or moving show in the notice; nothing has changed then. */
  const report = (error: unknown) => {
    const { message, action } = describeError(error);
    useUndoStore.getState().show([message, action].filter(Boolean).join(" "), null);
  };

  const finish = async (outcome: MoveOutcome, text: (count: number) => string) => {
    const removed = [...outcome.moved, ...outcome.deleted];
    const { selectedIds, selectMessage } = useUiStore.getState();
    // The list moves on to the next message, like after reading one.
    if (removed.some((id) => selectedIds.includes(id))) {
      selectMessage(
        nextAfter(
          messages.map((m) => m.id),
          removed,
        ),
      );
    }
    await refresh();
    const notice = useUndoStore.getState();
    if (outcome.moved.length > 0) {
      notice.show(text(outcome.moved.length), outcome.moved);
      window.setTimeout(() => push(outcome.accounts), UNDO_WINDOW_MS + 500);
    } else if (outcome.deleted.length > 0) {
      notice.show(tCount("undo.deleted", outcome.deleted.length), null);
    }
    // Deleting for good does not wait.
    if (outcome.deleted.length > 0) push(outcome.accounts);
  };

  const deleteNow = async (messageIds: string[]) => {
    try {
      await finish(await api.deleteMessages(messageIds), (n) => tCount("undo.trashed", n));
    } catch (error) {
      report(error);
    }
  };

  return {
    setRead: (messageIds: string[], read: boolean) => run(() => api.setRead(messageIds, read)),
    setFlagged: (messageIds: string[], flagged: boolean) =>
      run(() => api.setFlagged(messageIds, flagged)),

    archive: async (messageIds: string[]) => {
      try {
        // The name of the folder made for an account without archive, in the interface language.
        const outcome = await api.archiveMessages(messageIds, t("folder.archive"));
        await finish(outcome, (n) => tCount("undo.archived", n));
      } catch (error) {
        report(error);
      }
    },

    /** Into the trash; deleting for good asks first (DeleteForeverDialog). */
    trash: async (messageIds: string[]) => {
      const forGood = messages.some((m) => messageIds.includes(m.id) && deletesForGood(m, folders));
      if (forGood) useUiStore.getState().askDeleteForever(messageIds);
      else await deleteNow(messageIds);
    },
    /** After the user agreed to deleting for good. */
    deleteNow,

    moveTo: async (messageIds: string[], folder: Folder) => {
      try {
        const outcome = await api.moveMessages(messageIds, folder.id);
        await finish(outcome, (n) => tCount("undo.moved", n, { folder: folderLabel(folder) }));
      } catch (error) {
        report(error);
      }
    },

    /** Takes back the last archiving, deleting or moving, while the notice shows. */
    undo: async () => {
      const notice = useUndoStore.getState();
      const messageIds = notice.notice?.messageIds;
      if (!messageIds) return;
      notice.clear();
      const restored = await api.undoMoves(messageIds);
      await refresh();
      if (restored.length > 0) useUiStore.getState().setSelection(restored);
      else notice.show(t("undo.tooLate"), null);
    },
  };
}
