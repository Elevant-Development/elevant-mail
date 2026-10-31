import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useEffectEvent, useRef } from "react";

import { notificationsAllowed, showNotification } from "@/lib/desktop-notify";
import { describeError } from "@/lib/errors";
import { newMailNotice } from "@/lib/notify";
import {
  type Account,
  api,
  type Folder,
  type NewMail,
  type SyncReport,
  usesMockData,
} from "@/lib/tauri";
import { useCalendarSync } from "@/hooks/use-calendar-sync";
import { useAccounts } from "@/queries/accounts";
import { useSyncStore } from "@/stores/sync";
import { useUiStore } from "@/stores/ui";

/**
 * All accounts are checked this often while the app is open. Where the server supports IMAP
 * IDLE, new mail in the inbox arrives at once anyway (`inbox-changed`); the poll covers the
 * other servers and the other folders' counts.
 */
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
  ): Promise<SyncReport | null> => {
    const store = useSyncStore.getState();
    store.setProgress({ accountId, folderId, stage: "connecting", done: 0, total: 0 });
    try {
      const report = await task();
      store.finish(accountId);
      return report;
    } catch (error) {
      store.finish(accountId, describeError(error));
      return null;
    } finally {
      await Promise.all(
        ["folders", "messages", "message", "conversation"].map((key) =>
          queryClient.invalidateQueries({ queryKey: [key] }),
        ),
      );
    }
  };

  /** New mail in an inbox, as a notification while the window is not in front. */
  const announce = async (accountId: string, mail: NewMail[]) => {
    const { notifications, notificationPreview } = useUiStore.getState();
    if (!notifications || mail.length === 0 || document.hasFocus()) return;
    if (!(await notificationsAllowed())) return;
    const account = queryClient
      .getQueryData<Account[]>(["accounts"])
      ?.find((a) => a.id === accountId);
    showNotification(newMailNotice(mail, account?.displayName ?? "", notificationPreview));
  };

  return {
    syncAccount: async (accountId: string) => {
      // Already running (startup, poll, a new account): the running sync covers it.
      if (useSyncStore.getState().progress[accountId]) return;
      const report = await run(accountId, null, () => api.syncAccount(accountId));
      if (report) await announce(accountId, report.newMail);
    },
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
 * start (and new accounts when they appear), syncs an account whenever its server reports a
 * change in the inbox, and polls all current accounts. Does nothing with
 * sample data. Adding an account also syncs it right away (AccountDialog).
 */
export function useBackgroundSync() {
  const { data: accounts } = useAccounts();
  const { syncAccount } = useSyncActions();
  const { syncCalendars } = useCalendarSync();
  const synced = useRef(new Set<string>());
  const sync = useEffectEvent((accountId: string) => void syncAccount(accountId));
  // Calendars go along at start and with every poll, not on every new mail.
  const syncAll = useEffectEvent((accountId: string) => {
    sync(accountId);
    void syncCalendars(accountId);
  });
  const pollAll = useEffectEvent(() => {
    for (const account of accounts ?? []) syncAll(account.id);
  });

  // Sent mail filed, drafts uploaded or removed in the background: the lists follow.
  const queryClient = useQueryClient();
  useEffect(() => {
    const stop = api.onCacheChanged(() => {
      for (const key of ["folders", "messages", "conversation", "search"]) {
        void queryClient.invalidateQueries({ queryKey: [key] });
      }
    });
    return () => void stop.then((unlisten) => unlisten());
  }, [queryClient]);

  useEffect(() => {
    if (usesMockData) return;
    const listening = api.onSyncProgress((progress) => {
      if (progress.stage !== "done") useSyncStore.getState().setProgress(progress);
    });
    // The server told the backend's waiting connection about a change in an inbox.
    const watching = api.onInboxChanged(sync);
    return () => {
      void listening.then((stop) => stop());
      void watching.then((stop) => stop());
    };
  }, []);

  useEffect(() => {
    if (usesMockData || !accounts) return;
    const current = new Set(accounts.map((account) => account.id));
    // Removed accounts drop out of polling.
    for (const id of synced.current) if (!current.has(id)) synced.current.delete(id);
    for (const id of current) {
      if (synced.current.has(id)) continue;
      synced.current.add(id);
      syncAll(id);
    }
  }, [accounts]);

  useEffect(() => {
    if (usesMockData) return;
    const timer = window.setInterval(pollAll, POLL_INTERVAL_MS);
    return () => window.clearInterval(timer);
  }, []);
}
