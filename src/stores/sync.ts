/** What the mail sync is doing right now, per account. Not persisted. */
import { create } from "zustand";

import type { UserError } from "@/lib/errors";
import type { SyncProgress } from "@/lib/tauri";

interface SyncState {
  /** Running operation per account. */
  progress: Record<string, SyncProgress | undefined>;
  /** Last failure per account, cleared by the next success. */
  errors: Record<string, UserError | undefined>;
  /** Folders currently loading older mail. */
  loadingOlder: Record<string, boolean | undefined>;
  /** When a folder was last synced in this session (ms), to avoid re-syncing on every click. */
  lastFolderSync: Record<string, number | undefined>;

  setProgress: (progress: SyncProgress) => void;
  finish: (accountId: string, error?: UserError) => void;
  /** A failure outside a sync, e.g. sending read or flagged state. */
  setError: (accountId: string, error: UserError) => void;
  setLoadingOlder: (folderId: string, loading: boolean) => void;
  markFolderSynced: (folderId: string) => void;
}

export const useSyncStore = create<SyncState>()((set) => ({
  progress: {},
  errors: {},
  loadingOlder: {},
  lastFolderSync: {},

  setProgress: (progress) =>
    set((state) => ({ progress: { ...state.progress, [progress.accountId]: progress } })),
  finish: (accountId, error) =>
    set((state) => ({
      progress: { ...state.progress, [accountId]: undefined },
      errors: { ...state.errors, [accountId]: error },
    })),
  setError: (accountId, error) =>
    set((state) => ({ errors: { ...state.errors, [accountId]: error } })),
  setLoadingOlder: (folderId, loading) =>
    set((state) => ({ loadingOlder: { ...state.loadingOlder, [folderId]: loading } })),
  markFolderSynced: (folderId) =>
    set((state) => ({ lastFolderSync: { ...state.lastFolderSync, [folderId]: Date.now() } })),
}));
