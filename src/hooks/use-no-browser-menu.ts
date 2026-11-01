import { useEffect } from "react";

/**
 * The web view's own context menu ("Reload", "Back") makes the app feel like a web page: it stays
 * only where it helps, in text fields and on selected text. The built app does not reload on F5
 * or Ctrl+R either; in development it does. (The mail itself shows in a frame without scripts,
 * which keeps its menu for copying and links.)
 */
export function useNoBrowserMenu() {
  useEffect(() => {
    const onContextMenu = (event: MouseEvent) => {
      const target = event.target instanceof Element ? event.target : null;
      const editable = target?.closest(
        "input, textarea, [contenteditable='true'], [contenteditable='']",
      );
      const selected = (window.getSelection()?.toString() ?? "") !== "";
      if (!editable && !selected) event.preventDefault();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      const reload =
        event.key === "F5" ||
        ((event.ctrlKey || event.metaKey) && !event.altKey && event.key.toLowerCase() === "r");
      if (reload && import.meta.env.PROD) event.preventDefault();
    };
    window.addEventListener("contextmenu", onContextMenu);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("contextmenu", onContextMenu);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, []);
}
