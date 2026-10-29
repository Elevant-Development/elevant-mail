import { useQueryClient } from "@tanstack/react-query";

import { describeError } from "@/lib/errors";
import { api } from "@/lib/tauri";
import { useSyncStore } from "@/stores/sync";

/**
 * Read and flagged state. The backend changes the cache at once, the lists are reread, and the
 * change goes to the server in the background. A failure shows on the account like a sync error;
 * the change stays queued, or is undone if the server refused it.
 */
export function useMessageActions() {
  const queryClient = useQueryClient();

  const refresh = () =>
    Promise.all(
      ["messages", "message", "folders", "search"].map((key) =>
        queryClient.invalidateQueries({ queryKey: [key] }),
      ),
    );

  const run = async (change: () => Promise<string[]>) => {
    const accountIds = await change();
    await refresh();
    for (const accountId of accountIds) {
      api.pushActions(accountId).catch(async (error: unknown) => {
        useSyncStore.getState().setError(accountId, describeError(error));
        // A refused change was undone in the cache.
        await refresh();
      });
    }
  };

  return {
    setRead: (messageIds: string[], read: boolean) => run(() => api.setRead(messageIds, read)),
    setFlagged: (messageIds: string[], flagged: boolean) =>
      run(() => api.setFlagged(messageIds, flagged)),
  };
}
