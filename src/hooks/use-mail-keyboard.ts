import { useEffect, useEffectEvent } from "react";

import {
  adjacentMessageId,
  focusListItem,
  focusReader,
  focusSearch,
  hasOpenOverlay,
  isTypingTarget,
  LIST_ATTRIBUTE,
  READER_ATTRIBUTE,
} from "@/lib/keyboard";
import { useMessageActions } from "@/hooks/use-message-actions";
import { useVisibleMessages } from "@/queries/messages";
import { useUiStore } from "@/stores/ui";

interface MailKeyboardOptions {
  /** Off while something covers the mail view, like the sidebar overlay. */
  enabled: boolean;
  /** List and reader side by side; in the narrow layout the reader replaces the list. */
  isWide: boolean;
}

/**
 * Shortcuts of the mail view: j/k (and the arrow keys outside the reader) move through the list,
 * with Shift they widen the selection; Ctrl+A selects the whole list. Enter opens the first mail
 * or moves the focus into the reader, Escape goes back one level (and finally clears a search).
 * On the selection: u toggles read, s the star, e archives, # and Delete delete, v moves; z takes
 * the last archiving, deleting or moving back. / jumps to the search field.
 */
export function useMailKeyboard({ enabled, isWide }: MailKeyboardOptions) {
  const { data: messages = [] } = useVisibleMessages();
  const { setRead, setFlagged, archive, trash, undo } = useMessageActions();

  const onKeyDown = useEffectEvent((event: KeyboardEvent) => {
    if (!enabled || event.defaultPrevented || event.altKey) return;
    if (isTypingTarget(event.target) || hasOpenOverlay()) return;

    const target = event.target instanceof Element ? event.target : null;
    const inList = target?.closest(`[${LIST_ATTRIBUTE}]`) != null;
    const inReader = target?.closest(`[${READER_ATTRIBUTE}]`) != null;
    const ui = useUiStore.getState();
    const { selectedMessageId, selectedIds, selectMessage } = ui;
    const order = messages.map((m) => m.id);

    if (event.ctrlKey || event.metaKey) {
      // In the reader Ctrl+A selects the text, as everywhere.
      if (event.key.toLowerCase() === "a" && !inReader) {
        event.preventDefault();
        ui.selectAll(order);
      }
      return;
    }

    const move = (step: 1 | -1) => {
      event.preventDefault();
      // From the end that moved last; with Shift the open message stays the other end.
      const next = adjacentMessageId(order, ui.selectionEnd ?? selectedMessageId, step);
      if (!next) return;
      if (event.shiftKey) ui.selectRange(order, next);
      else selectMessage(next);
    };

    const selected = messages.filter((m) => selectedIds.includes(m.id));
    const ids = selected.map((m) => m.id);
    const onSelection = (action: () => unknown) => {
      if (ids.length === 0) return;
      event.preventDefault();
      void action();
    };

    switch (event.key) {
      case "/":
        event.preventDefault();
        return focusSearch();
      case "u":
        return onSelection(() =>
          setRead(
            ids,
            selected.some((m) => !m.isRead),
          ),
        );
      case "s":
        return onSelection(() =>
          setFlagged(
            ids,
            selected.some((m) => !m.isFlagged),
          ),
        );
      case "e":
        return onSelection(() => archive(ids));
      case "#":
      case "Delete":
        return onSelection(() => trash(ids));
      case "v":
        return onSelection(() => ui.openMoveDialog(ids));
      case "z":
        event.preventDefault();
        return void undo();
      case "j":
      case "J":
        return move(1);
      case "k":
      case "K":
        return move(-1);
      case "ArrowDown":
      case "ArrowUp":
        // In the reader the arrow keys scroll the mail, elsewhere they belong to the control.
        if (inReader || (!inList && target !== document.body)) return;
        return move(event.key === "ArrowDown" ? 1 : -1);
      case "Enter": {
        // Buttons outside the list keep their own Enter; on a list entry the click selects it.
        if (!inList && target?.closest("button, a, [role]") && target !== document.body) return;
        if (!inList && selectedMessageId === null) {
          const first = messages[0];
          if (!first) return;
          selectMessage(first.id);
        }
        // The reader renders the new selection first.
        requestAnimationFrame(focusReader);
        return;
      }
      case "Escape":
        if (selectedMessageId === null) {
          const { searchQuery, setSearchQuery } = useUiStore.getState();
          if (searchQuery) setSearchQuery("");
          return;
        }
        event.preventDefault();
        if (selectedIds.length <= 1 && isWide && inReader && focusListItem(selectedMessageId)) {
          return;
        }
        selectMessage(null);
        return;
    }
  });

  useEffect(() => {
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}
