/** Pure UI state: what is selected and how the window is laid out. Backend data lives in queries/. */
import { create } from "zustand";

import { UNIFIED_INBOX } from "@/lib/tauri";

export type Theme = "dark" | "light";

interface UiState {
  theme: Theme;
  /** Folder id or UNIFIED_INBOX. */
  selectedMailboxId: string;
  selectedMessageId: string | null;
  /** Only used in the narrow layout, where the sidebar is an overlay. */
  sidebarOpen: boolean;

  toggleTheme: () => void;
  selectMailbox: (mailboxId: string) => void;
  selectMessage: (messageId: string | null) => void;
  setSidebarOpen: (open: boolean) => void;
}

export const useUiStore = create<UiState>()((set) => ({
  theme: "dark",
  selectedMailboxId: UNIFIED_INBOX,
  selectedMessageId: null,
  sidebarOpen: false,

  toggleTheme: () => set((state) => ({ theme: state.theme === "dark" ? "light" : "dark" })),
  selectMailbox: (mailboxId) =>
    set({ selectedMailboxId: mailboxId, selectedMessageId: null, sidebarOpen: false }),
  selectMessage: (messageId) => set({ selectedMessageId: messageId }),
  setSidebarOpen: (open) => set({ sidebarOpen: open }),
}));
