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
export const READER_ATTRIBUTE = "data-reader-body";

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
