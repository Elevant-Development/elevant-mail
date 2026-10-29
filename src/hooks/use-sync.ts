import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useEffectEvent, useRef } from "react";

import { describeError } from "@/lib/errors";
import { api, type Folder, type SyncReport, usesMockData } from "@/lib/tauri";
import { useAccounts } from "@/queries/accounts";
import { useSyncStore } from "@/stores/sync";

/** New mail is checked this often while the app is open (IMAP IDLE comes later). */
const POLL_INTERVAL_MS = 5 * 60 * 1000;
/** Selecting a folder syncs it, unless that happened within this time. */
const FOLDER_FRESH_MS = 60 * 1000;

/** Sync calls that keep the progress store and the cached queries up to date. */
export function useSyncActions() {
  const queryClient = useQueryClient();

  const run = async (
    accountId: string,
    folderId: string | null,
    task: () => Promise<SyncReport>,
  ) => {
    const store = useSyncStore.getState();
    store.setProgress({ accountId, folderId, stage: "connecting", done: 0, total: 0 });
    try {
      await task();
      store.finish(accountId);
    } catch (error) {
      store.finish(accountId, describeError(error));
    } finally {
      await Promise.all(
        ["folders", "messages", "message"].map((key) =>
          queryClient.invalidateQueries({ queryKey: [key] }),
        ),
      );
    }
  };

  return {
    syncAccount: (accountId: string) => run(accountId, null, () => api.syncAccount(accountId)),
    syncFolder: (folder: Folder, { force = false } = {}) => {
      const lastSync = useSyncStore.getState().lastFolderSync[folder.id] ?? 0;
      if (!force && Date.now() - lastSync < FOLDER_FRESH_MS) return Promise.resolve();
      useSyncStore.getState().markFolderSynced(folder.id);
      return run(folder.accountId, folder.id, () => api.syncFolder(folder.id));
    },
    loadOlder: async (folder: Folder) => {
      const store = useSyncStore.getState();
      store.setLoadingOlder(folder.id, true);
      await run(folder.accountId, folder.id, () => api.loadOlder(folder.id));
      store.setLoadingOlder(folder.id, false);
    },
  };
}

/**
 * Keeps mail current in the background: listens for progress, syncs every account once at
 * start (and new accounts when they appear), then polls. Does nothing with sample data.
 */
export function useBackgroundSync() {
  const { data: accounts } = useAccounts();
  const { syncAccount } = useSyncActions();
  const synced = useRef(new Set<string>());
  const sync = useEffectEvent((accountId: string) => void syncAccount(accountId));

  useEffect(() => {
    if (usesMockData) return;
    const listening = api.onSyncProgress((progress) => {
      if (progress.stage !== "done") useSyncStore.getState().setProgress(progress);
    });
    return () => void listening.then((stop) => stop());
  }, []);

  useEffect(() => {
    if (usesMockData || !accounts) return;
    const current = new Set(accounts.map((account) => account.id));
    // Removed accounts drop out of polling.
    for (const id of synced.current) if (!current.has(id)) synced.current.delete(id);
    for (const id of current) {
      if (synced.current.has(id)) continue;
      synced.current.add(id);
      sync(id);
    }
  }, [accounts]);

  useEffect(() => {
    if (usesMockData) return;
    const timer = window.setInterval(() => {
      for (const id of synced.current) sync(id);
    }, POLL_INTERVAL_MS);
    return () => window.clearInterval(timer);
  }, []);
}
