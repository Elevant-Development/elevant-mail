/**
 * Applies the chosen theme and mode to <html>: the `dark` class for Tailwind and every design
 * token as a CSS custom property. Runs before the first render and on every change, so there
 * is no frame with missing colours.
 */
import { useUiStore } from "@/stores/ui";

import { findFont, fontTokens } from "./fonts";
import { findTheme, type ResolvedMode, themeTokens } from "./themes";

const darkQuery = window.matchMedia("(prefers-color-scheme: dark)");

function resolveMode(mode: "dark" | "light" | "system"): ResolvedMode {
  if (mode === "system") return darkQuery.matches ? "dark" : "light";
  return mode;
}

function apply() {
  const { mode, themeId, contrast, style, fontId } = useUiStore.getState();
  const resolved = resolveMode(mode);
  const root = document.documentElement;
  root.classList.toggle("dark", resolved === "dark");
  const tokens = {
    ...themeTokens(findTheme(themeId), { mode: resolved, contrast, style }),
    ...fontTokens(findFont(fontId)),
  };
  for (const [name, value] of Object.entries(tokens)) {
    root.style.setProperty(name, value);
  }
}

export function initAppearance() {
  apply();
  useUiStore.subscribe((state, previous) => {
    if (
      state.mode !== previous.mode ||
      state.themeId !== previous.themeId ||
      state.contrast !== previous.contrast ||
      state.style !== previous.style ||
      state.fontId !== previous.fontId
    ) {
      apply();
    }
  });
  darkQuery.addEventListener("change", () => {
    if (useUiStore.getState().mode === "system") apply();
  });
}
