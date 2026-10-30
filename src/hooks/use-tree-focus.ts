import { type RefObject, useEffect } from "react";

import { TREE_ENTRY_ATTRIBUTE } from "@/lib/keyboard";

/**
 * Makes the entries of a tree (`data-tree-entry`, nesting in `data-depth`) one tab stop: Tab
 * reaches the focused entry while the focus is inside, otherwise the open one
 * (`aria-current="page"`). Up and Down move to the previous or next entry, Home and End to the
 * first or last. Right opens a closed entry (`data-expanded`) or moves into an open one; Left
 * closes it or moves to its parent. A row's `data-tree-toggle` button opens and closes it,
 * otherwise the entry itself does.
 */
export function useTreeFocus(ref: RefObject<HTMLElement | null>) {
  useEffect(() => {
    const tree = ref.current;
    if (!tree) return;
    const entries = () => [...tree.querySelectorAll<HTMLElement>(`[${TREE_ENTRY_ATTRIBUTE}]`)];
    const depth = (entry: HTMLElement) => Number(entry.dataset.depth ?? 0);

    const settle = () => {
      const all = entries();
      const stop =
        all.find((entry) => entry === document.activeElement) ??
        all.find((entry) => entry.getAttribute("aria-current") === "page") ??
        all[0];
      for (const entry of all) entry.tabIndex = entry === stop ? 0 : -1;
    };

    const toggle = (entry: HTMLElement) =>
      (entry.parentElement?.querySelector<HTMLElement>("[data-tree-toggle]") ?? entry).click();

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
      const entry =
        event.target instanceof HTMLElement
          ? event.target.closest<HTMLElement>(`[${TREE_ENTRY_ATTRIBUTE}]`)
          : null;
      if (!entry) return;
      const all = entries();
      const index = all.indexOf(entry);
      const expanded = entry.dataset.expanded;
      let next: HTMLElement | undefined;
      switch (event.key) {
        case "ArrowDown":
          next = all[index + 1];
          break;
        case "ArrowUp":
          next = all[index - 1];
          break;
        case "Home":
          next = all[0];
          break;
        case "End":
          next = all[all.length - 1];
          break;
        case "ArrowRight":
          if (expanded === "false") toggle(entry);
          else if (expanded === "true") next = all[index + 1];
          break;
        case "ArrowLeft":
          if (expanded === "true") toggle(entry);
          else next = all.slice(0, index).findLast((other) => depth(other) < depth(entry));
          break;
        default:
          return;
      }
      event.preventDefault();
      next?.focus();
    };

    settle();
    // Entries come and go as folders open and close or another one is selected.
    const observer = new MutationObserver(settle);
    observer.observe(tree, { childList: true, subtree: true, attributeFilter: ["aria-current"] });
    tree.addEventListener("focusin", settle);
    tree.addEventListener("keydown", onKeyDown);
    return () => {
      observer.disconnect();
      tree.removeEventListener("focusin", settle);
      tree.removeEventListener("keydown", onKeyDown);
    };
  }, [ref]);
}
