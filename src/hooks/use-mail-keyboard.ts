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
 * Enter opens the first mail or moves the focus into the reader, Escape goes back one level
 * (and finally clears a search), u toggles read, s the star, / jumps to the search field.
 */
export function useMailKeyboard({ enabled, isWide }: MailKeyboardOptions) {
  const { data: messages = [] } = useVisibleMessages();
  const { setRead, setFlagged } = useMessageActions();

  const onKeyDown = useEffectEvent((event: KeyboardEvent) => {
    if (!enabled || event.defaultPrevented) return;
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    if (isTypingTarget(event.target) || hasOpenOverlay()) return;

    const target = event.target instanceof Element ? event.target : null;
    const inList = target?.closest(`[${LIST_ATTRIBUTE}]`) != null;
    const inReader = target?.closest(`[${READER_ATTRIBUTE}]`) != null;
    const { selectedMessageId, selectMessage } = useUiStore.getState();

    const move = (step: 1 | -1) => {
      const next = adjacentMessageId(
        messages.map((m) => m.id),
        selectedMessageId,
        step,
      );
      if (next) selectMessage(next);
      event.preventDefault();
    };

    const selected = messages.find((m) => m.id === selectedMessageId);

    switch (event.key) {
      case "/":
        event.preventDefault();
        return focusSearch();
      case "u":
        if (!selected) return;
        event.preventDefault();
        return void setRead([selected.id], !selected.isRead);
      case "s":
        if (!selected) return;
        event.preventDefault();
        return void setFlagged([selected.id], !selected.isFlagged);
      case "j":
        return move(1);
      case "k":
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
        if (isWide && inReader && focusListItem(selectedMessageId)) return;
        selectMessage(null);
        return;
    }
  });

  useEffect(() => {
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}
