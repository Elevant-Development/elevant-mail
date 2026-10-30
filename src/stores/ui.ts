/** Pure UI state: what is selected and how the window is laid out. Backend data lives in queries/. */
import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

import {
  type Contrast,
  defaultContrast,
  defaultStyle,
  defaultThemeId,
  type Style,
  type ThemeId,
} from "@/design/themes";
import { defaultFontId, type FontId } from "@/design/fonts";
import type { LanguageSetting } from "@/lib/i18n";
import { type Draft, type ListFilter, UNIFIED_INBOX } from "@/lib/tauri";

export type { ListFilter } from "@/lib/tauri";

export type { Draft } from "@/lib/tauri";
export type Mode = "dark" | "light" | "system";
/** What the search on the servers found for a query, and which accounts it failed on. */
export interface ServerHits {
  query: string;
  ids: string[];
  failed: { accountId: string; message: string }[];
}
export type Density = "comfortable" | "compact";
/** A page of the settings window: an account (null: the first one), or the appearance. */
export type SettingsPage =
  | { section: "accounts"; accountId: string | null }
  | { section: "appearance" }
  | { section: "notifications" }
  | { section: "reading" };

interface UiState {
  mode: Mode;
  themeId: ThemeId;
  contrast: Contrast;
  style: Style;
  fontId: FontId;
  density: Density;
  /** Read once at start by lib/i18n; changing it reloads the interface. */
  language: LanguageSetting;
  /** A desktop notification for new mail while the window is not in front. */
  notifications: boolean;
  /** Sender and subject in it; off, it only names the account. */
  notificationPreview: boolean;
  /** Once a day, ask GitHub whether a newer version was released. */
  updateCheck: boolean;
  /** The first start explained the check above. */
  updateCheckExplained: boolean;
  /** A version the user put off with "Later"; not offered again. */
  skippedUpdate: string | null;
  /** The account whose inbox opens at start; null: all inboxes. */
  startAccountId: string | null;
  /** How long an opened mail stays open before it counts as read, in ms; null: never by itself. */
  markReadDelay: number | null;
  /** The list shows conversations; off, every message is a row. */
  threads: boolean;
  /** Folder id or UNIFIED_INBOX. */
  selectedMailboxId: string;
  /**
   * The open row, and the fixed end of a Shift range. A row is named by its newest message (with
   * conversations, see `Conversation`); `followRows` keeps the selection on it when that changes.
   */
  selectedMessageId: string | null;
  /** All selected rows; just the open one unless Ctrl or Shift added more. */
  selectedIds: string[];
  /** The moving end of a Shift range (Shift+j/k, Shift+click). */
  selectionEnd: string | null;
  /** Messages the folder picker moves; null while it is closed. */
  moveDialogIds: string[] | null;
  /** Mail being written, by key; kept by the backend too (Composer saves, AppShell loads). */
  drafts: Record<string, Draft>;
  /** The draft the reader column shows instead of a mail; null while none. */
  composeKey: string | null;
  /** Messages waiting for "delete for good?"; null while nothing asks. */
  deleteForeverIds: string[] | null;
  /** Only used in the narrow layout, where the sidebar is an overlay. */
  sidebarOpen: boolean;
  accountDialogOpen: boolean;
  /** The open page of the settings window; null while it is closed. */
  settingsPage: SettingsPage | null;
  /**
   * Sidebar sections and folders the user collapsed (true) or opened (false), by node key;
   * missing means the default. Survives a restart.
   */
  collapsed: Record<string, boolean | undefined>;
  /** Text in the search field; while it is not empty, the list shows search results. */
  searchQuery: string;
  /** Switched on in the filter bar; reset when another mailbox opens. */
  listFilters: ListFilter[];
  /** From the last search on the servers; it counts while the search field holds its query. */
  serverHits: ServerHits | null;
  /** Mails whose external images the user allowed in this session. */
  remoteImagesAllowed: Record<string, boolean | undefined>;

  setMode: (mode: Mode) => void;
  setTheme: (themeId: ThemeId) => void;
  setContrast: (contrast: Contrast) => void;
  setStyle: (style: Style) => void;
  setFont: (fontId: FontId) => void;
  setDensity: (density: Density) => void;
  setLanguage: (language: LanguageSetting) => void;
  setNotifications: (on: boolean) => void;
  setNotificationPreview: (on: boolean) => void;
  setUpdateCheck: (on: boolean) => void;
  explainUpdateCheck: () => void;
  skipUpdate: (version: string) => void;
  setStartAccount: (accountId: string | null) => void;
  setMarkReadDelay: (delay: number | null) => void;
  setThreads: (on: boolean) => void;
  selectMailbox: (mailboxId: string) => void;
  selectMessage: (messageId: string | null) => void;
  /** Ctrl+click: adds the message or takes it out again. */
  toggleSelected: (messageId: string) => void;
  /** Shift+click and Shift+j/k: everything from the open message to `messageId`, in `order`. */
  selectRange: (order: string[], messageId: string) => void;
  selectAll: (messageIds: string[]) => void;
  /** Several selected messages at once, e.g. after undo. */
  setSelection: (messageIds: string[]) => void;
  /**
   * After the list changed: a selected id that is no longer a row moves to the row whose
   * conversation holds it, e.g. when a reply arrived or undo brought messages back.
   */
  followRows: (rows: { id: string; messageIds: string[] }[]) => void;
  openMoveDialog: (messageIds: string[]) => void;
  /** Shows a draft in the reader column; one with the same key that exists keeps its text. */
  openDraft: (draft: Draft) => void;
  updateDraft: (key: string, changes: Partial<Omit<Draft, "key">>) => void;
  /** Hides the composer; the draft stays. */
  closeCompose: () => void;
  discardDraft: (key: string) => void;
  /** The drafts kept from before, at start; open ones keep what was written since. */
  loadDrafts: (drafts: Draft[]) => void;
  closeMoveDialog: () => void;
  askDeleteForever: (messageIds: string[] | null) => void;
  setSidebarOpen: (open: boolean) => void;
  setAccountDialogOpen: (open: boolean) => void;
  openSettings: (page: SettingsPage) => void;
  closeSettings: () => void;
  allowRemoteImages: (messageId: string) => void;
  setSearchQuery: (query: string) => void;
  toggleListFilter: (filter: ListFilter) => void;
  clearListFilters: () => void;
  setServerHits: (hits: ServerHits | null) => void;
  setCollapsed: (key: string, collapsed: boolean) => void;
}

export const useUiStore = create<UiState>()(
  persist(
    (set) => ({
      mode: "dark",
      themeId: defaultThemeId,
      contrast: defaultContrast,
      style: defaultStyle,
      fontId: defaultFontId,
      density: "comfortable",
      language: "system",
      notifications: true,
      notificationPreview: true,
      updateCheck: true,
      updateCheckExplained: false,
      skippedUpdate: null,
      startAccountId: null,
      markReadDelay: 1000,
      threads: true,
      selectedMailboxId: UNIFIED_INBOX,
      selectedMessageId: null,
      selectedIds: [],
      selectionEnd: null,
      moveDialogIds: null,
      drafts: {},
      composeKey: null,
      deleteForeverIds: null,
      sidebarOpen: false,
      accountDialogOpen: false,
      settingsPage: null,
      remoteImagesAllowed: {},
      searchQuery: "",
      listFilters: [],
      serverHits: null,
      collapsed: {},

      setMode: (mode) => set({ mode }),
      setTheme: (themeId) => set({ themeId }),
      setContrast: (contrast) => set({ contrast }),
      setStyle: (style) => set({ style }),
      setFont: (fontId) => set({ fontId }),
      setDensity: (density) => set({ density }),
      setNotifications: (notifications) => set({ notifications }),
      setNotificationPreview: (notificationPreview) => set({ notificationPreview }),
      setUpdateCheck: (updateCheck) => set({ updateCheck, updateCheckExplained: true }),
      explainUpdateCheck: () => set({ updateCheckExplained: true }),
      skipUpdate: (version) => set({ skippedUpdate: version }),
      setStartAccount: (startAccountId) => set({ startAccountId }),
      setMarkReadDelay: (markReadDelay) => set({ markReadDelay }),
      setThreads: (threads) => set({ threads }),
      setLanguage: (language) => {
        set({ language });
        // The texts are read once; a reload shows them in the new language.
        window.location.reload();
      },
      selectMailbox: (mailboxId) =>
        set({
          selectedMailboxId: mailboxId,
          selectedMessageId: null,
          selectedIds: [],
          selectionEnd: null,
          sidebarOpen: false,
          searchQuery: "",
          listFilters: [],
          serverHits: null,
        }),
      // Opening a mail puts a draft aside; it waits in `drafts`.
      selectMessage: (messageId) =>
        set((state) => ({
          selectedMessageId: messageId,
          selectedIds: messageId === null ? [] : [messageId],
          selectionEnd: messageId,
          composeKey: messageId === null ? state.composeKey : null,
        })),
      openDraft: (draft) =>
        set((state) => ({
          drafts: state.drafts[draft.key] ? state.drafts : { ...state.drafts, [draft.key]: draft },
          composeKey: draft.key,
          sidebarOpen: false,
        })),
      updateDraft: (key, changes) =>
        set((state) => {
          const draft = state.drafts[key];
          return draft ? { drafts: { ...state.drafts, [key]: { ...draft, ...changes } } } : state;
        }),
      closeCompose: () => set({ composeKey: null }),
      loadDrafts: (loaded) =>
        set((state) => ({
          drafts: { ...Object.fromEntries(loaded.map((d) => [d.key, d])), ...state.drafts },
        })),
      discardDraft: (key) =>
        set((state) => {
          const { [key]: _gone, ...drafts } = state.drafts;
          return { drafts, composeKey: state.composeKey === key ? null : state.composeKey };
        }),
      toggleSelected: (messageId) =>
        set((state) => {
          if (!state.selectedIds.includes(messageId)) {
            return {
              selectedIds: [...state.selectedIds, messageId],
              selectedMessageId: messageId,
              selectionEnd: messageId,
            };
          }
          const selectedIds = state.selectedIds.filter((id) => id !== messageId);
          const open =
            state.selectedMessageId === messageId
              ? (selectedIds[selectedIds.length - 1] ?? null)
              : state.selectedMessageId;
          return { selectedIds, selectedMessageId: open, selectionEnd: open };
        }),
      selectRange: (order, messageId) =>
        set((state) => {
          const from = order.indexOf(state.selectedMessageId ?? messageId);
          const to = order.indexOf(messageId);
          if (from === -1 || to === -1) {
            return {
              selectedMessageId: messageId,
              selectedIds: [messageId],
              selectionEnd: messageId,
            };
          }
          return {
            selectedIds: order.slice(Math.min(from, to), Math.max(from, to) + 1),
            selectionEnd: messageId,
          };
        }),
      selectAll: (messageIds) =>
        set((state) => ({
          selectedIds: messageIds,
          selectedMessageId:
            state.selectedMessageId && messageIds.includes(state.selectedMessageId)
              ? state.selectedMessageId
              : (messageIds[0] ?? null),
          selectionEnd: messageIds[messageIds.length - 1] ?? null,
        })),
      setSelection: (messageIds) =>
        set({
          selectedIds: messageIds,
          selectedMessageId: messageIds[0] ?? null,
          selectionEnd: messageIds[messageIds.length - 1] ?? null,
        }),
      followRows: (rows) =>
        set((state) => {
          const ids = new Set(rows.map((row) => row.id));
          const rowOf = (id: string | null) =>
            id === null || ids.has(id)
              ? id
              : (rows.find((row) => row.messageIds.includes(id))?.id ?? id);
          const selectedIds = [...new Set(state.selectedIds.map(rowOf))].filter(
            (id): id is string => id !== null,
          );
          const selectedMessageId = rowOf(state.selectedMessageId);
          const selectionEnd = rowOf(state.selectionEnd);
          const unchanged =
            selectedMessageId === state.selectedMessageId &&
            selectionEnd === state.selectionEnd &&
            selectedIds.length === state.selectedIds.length &&
            selectedIds.every((id, index) => id === state.selectedIds[index]);
          return unchanged ? state : { selectedIds, selectedMessageId, selectionEnd };
        }),
      openMoveDialog: (messageIds) => set({ moveDialogIds: messageIds }),
      closeMoveDialog: () => set({ moveDialogIds: null }),
      askDeleteForever: (messageIds) => set({ deleteForeverIds: messageIds }),
      setSidebarOpen: (open) => set({ sidebarOpen: open }),
      setAccountDialogOpen: (open) => set({ accountDialogOpen: open, sidebarOpen: false }),
      openSettings: (page) => set({ settingsPage: page, sidebarOpen: false }),
      closeSettings: () => set({ settingsPage: null }),
      setSearchQuery: (query) => set({ searchQuery: query }),
      toggleListFilter: (filter) =>
        set((state) => ({
          listFilters: state.listFilters.includes(filter)
            ? state.listFilters.filter((f) => f !== filter)
            : [...state.listFilters, filter],
        })),
      clearListFilters: () => set({ listFilters: [] }),
      setServerHits: (serverHits) => set({ serverHits }),
      setCollapsed: (key, collapsed) =>
        set((state) => ({ collapsed: { ...state.collapsed, [key]: collapsed } })),
      allowRemoteImages: (messageId) =>
        set((state) => ({
          remoteImagesAllowed: { ...state.remoteImagesAllowed, [messageId]: true },
        })),
    }),
    {
      // Appearance survives a restart; selection does not. Moves into the settings store once
      // there is a backend for settings.
      name: "elevant-mail-ui",
      version: 3,
      storage: createJSONStorage(() => localStorage),
      partialize: (state) => ({
        mode: state.mode,
        themeId: state.themeId,
        contrast: state.contrast,
        style: state.style,
        fontId: state.fontId,
        density: state.density,
        language: state.language,
        notifications: state.notifications,
        notificationPreview: state.notificationPreview,
        updateCheck: state.updateCheck,
        updateCheckExplained: state.updateCheckExplained,
        skippedUpdate: state.skippedUpdate,
        startAccountId: state.startAccountId,
        markReadDelay: state.markReadDelay,
        threads: state.threads,
        collapsed: state.collapsed,
      }),
      // Version 1 stored the design comparison and other theme ids; version 2 font pairings and
      // the icon set, both gone since.
      migrate: (persisted, version) => {
        if (version < 2) return {};
        const {
          fontId: _fontId,
          iconSet: _iconSet,
          ...rest
        } = persisted as Record<string, unknown>;
        return rest as Partial<UiState>;
      },
    },
  ),
);
