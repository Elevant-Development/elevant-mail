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

interface UiState {
  mode: Mode;
  themeId: ThemeId;
  contrast: Contrast;
  style: Style;
  fontId: FontId;
  /** Folder id or UNIFIED_INBOX. */
  selectedMailboxId: string;
  selectedMessageId: string | null;
  /** Only used in the narrow layout, where the sidebar is an overlay. */
  sidebarOpen: boolean;

  setMode: (mode: Mode) => void;
  setTheme: (themeId: ThemeId) => void;
  setContrast: (contrast: Contrast) => void;
  setStyle: (style: Style) => void;
  setFont: (fontId: FontId) => void;
  selectMailbox: (mailboxId: string) => void;
  selectMessage: (messageId: string | null) => void;
  setSidebarOpen: (open: boolean) => void;
}

export const useUiStore = create<UiState>()(
  persist(
    (set) => ({
      mode: "dark",
      themeId: defaultThemeId,
      contrast: defaultContrast,
      style: defaultStyle,
      fontId: defaultFontId,
      selectedMailboxId: UNIFIED_INBOX,
      selectedMessageId: null,
      sidebarOpen: false,

      setMode: (mode) => set({ mode }),
      setTheme: (themeId) => set({ themeId }),
      setContrast: (contrast) => set({ contrast }),
      setStyle: (style) => set({ style }),
      setFont: (fontId) => set({ fontId }),
      selectMailbox: (mailboxId) =>
        set({ selectedMailboxId: mailboxId, selectedMessageId: null, sidebarOpen: false }),
      selectMessage: (messageId) => set({ selectedMessageId: messageId }),
      setSidebarOpen: (open) => set({ sidebarOpen: open }),
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
