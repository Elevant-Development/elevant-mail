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
import { UNIFIED_INBOX } from "@/lib/tauri";

export type Mode = "dark" | "light" | "system";
export type Density = "comfortable" | "compact";

interface UiState {
  mode: Mode;
  themeId: ThemeId;
  contrast: Contrast;
  style: Style;
  fontId: FontId;
  density: Density;
  /** Folder id or UNIFIED_INBOX. */
  selectedMailboxId: string;
  selectedMessageId: string | null;
  /** Only used in the narrow layout, where the sidebar is an overlay. */
  sidebarOpen: boolean;
  accountDialogOpen: boolean;
  /** Text in the search field; while it is not empty, the list shows search results. */
  searchQuery: string;
  /** Mails whose external images the user allowed in this session. */
  remoteImagesAllowed: Record<string, boolean | undefined>;

  setMode: (mode: Mode) => void;
  setTheme: (themeId: ThemeId) => void;
  setContrast: (contrast: Contrast) => void;
  setStyle: (style: Style) => void;
  setFont: (fontId: FontId) => void;
  setDensity: (density: Density) => void;
  selectMailbox: (mailboxId: string) => void;
  selectMessage: (messageId: string | null) => void;
  setSidebarOpen: (open: boolean) => void;
  setAccountDialogOpen: (open: boolean) => void;
  allowRemoteImages: (messageId: string) => void;
  setSearchQuery: (query: string) => void;
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
      selectedMailboxId: UNIFIED_INBOX,
      selectedMessageId: null,
      sidebarOpen: false,
      accountDialogOpen: false,
      remoteImagesAllowed: {},
      searchQuery: "",

      setMode: (mode) => set({ mode }),
      setTheme: (themeId) => set({ themeId }),
      setContrast: (contrast) => set({ contrast }),
      setStyle: (style) => set({ style }),
      setFont: (fontId) => set({ fontId }),
      setDensity: (density) => set({ density }),
      selectMailbox: (mailboxId) =>
        set({
          selectedMailboxId: mailboxId,
          selectedMessageId: null,
          sidebarOpen: false,
          searchQuery: "",
        }),
      selectMessage: (messageId) => set({ selectedMessageId: messageId }),
      setSidebarOpen: (open) => set({ sidebarOpen: open }),
      setAccountDialogOpen: (open) => set({ accountDialogOpen: open, sidebarOpen: false }),
      setSearchQuery: (query) => set({ searchQuery: query }),
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
