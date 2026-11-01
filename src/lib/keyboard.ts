/** Helpers for the keyboard shortcuts of the mail view. */

/** Fields where a letter key types text instead of triggering a shortcut. */
export function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  if (target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement) return true;
  if (!(target instanceof HTMLInputElement)) return false;
  const nonText = ["button", "checkbox", "radio", "range", "reset", "submit", "color", "file"];
  return !nonText.includes(target.type);
}

/**
 * The message `step` places away from `currentId`, staying at the ends of the list.
 * Without a current message (or one that is no longer listed) it starts at the first.
 */
export function adjacentMessageId(
  ids: readonly string[],
  currentId: string | null,
  step: 1 | -1,
): string | null {
  if (ids.length === 0) return null;
  const index = currentId === null ? -1 : ids.indexOf(currentId);
  if (index === -1) return ids[0] ?? null;
  return ids[Math.min(Math.max(index + step, 0), ids.length - 1)] ?? null;
}

/** Dialogs and menus handle their own keys; the mail shortcuts pause while one is open. */
export function hasOpenOverlay(): boolean {
  return document.querySelector('[role="dialog"], [role="alertdialog"], [role="menu"]') !== null;
}

/** Marks the scrollable message list and reader body, so shortcuts know where the focus is. */
export const LIST_ATTRIBUTE = "data-message-list";
/** A row of the list, with the message it shows: the context menu finds it. */
export const MESSAGE_ATTRIBUTE = "data-message-id";
export const READER_ATTRIBUTE = "data-reader-body";
export const SEARCH_ATTRIBUTE = "data-search-input";
/** The sidebar's folder tree, and each of its entries (`useTreeFocus`). */
export const FOLDER_TREE_ATTRIBUTE = "data-folder-tree";
export const TREE_ENTRY_ATTRIBUTE = "data-tree-entry";
/**
 * A group with one tab stop (`tabindex="0"`) among its entries: the folder tree, the list. A
 * dialog returns the focus there when the entry it came from is gone, e.g. a moved mail.
 */
export const FOCUS_GROUP_ATTRIBUTE = "data-focus-group";

export function focusSearch() {
  const input = document.querySelector<HTMLInputElement>(`[${SEARCH_ATTRIBUTE}]`);
  input?.focus();
  input?.select();
}

export function focusReader() {
  document.querySelector<HTMLElement>(`[${READER_ATTRIBUTE}]`)?.focus();
}

/** Focuses the list entry of a message; false if it is not rendered (scrolled out of view). */
export function focusListItem(messageId: string, options?: FocusOptions): boolean {
  const item = document.querySelector<HTMLElement>(
    `[${LIST_ATTRIBUTE}] [data-message-id="${CSS.escape(messageId)}"]`,
  );
  item?.focus(options);
  return item !== null;
}

/**
 * F6 and Shift+F6: the focus moves between search field, folders, list and reader, to where
 * each would be reached with Tab. Parts not shown (the reader without a mail, the sidebar in a
 * narrow window) are skipped.
 */
export function focusNextRegion(step: 1 | -1) {
  const regions = [
    SEARCH_ATTRIBUTE,
    FOLDER_TREE_ATTRIBUTE,
    LIST_ATTRIBUTE,
    READER_ATTRIBUTE,
  ].flatMap((attribute) => {
    const region = document.querySelector<HTMLElement>(`[${attribute}]`);
    if (!region || region.getClientRects().length === 0) return [];
    const target = region.hasAttribute(FOCUS_GROUP_ATTRIBUTE)
      ? region.querySelector<HTMLElement>('[tabindex="0"]')
      : region;
    return target ? [{ region, target }] : [];
  });
  if (regions.length === 0) return;
  const current = regions.findIndex(({ region }) => region.contains(document.activeElement));
  const next =
    current === -1
      ? step === 1
        ? 0
        : regions.length - 1
      : (current + step + regions.length) % regions.length;
  regions[next]?.target.focus();
}
