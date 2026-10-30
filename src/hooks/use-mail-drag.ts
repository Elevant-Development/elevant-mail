import type { PointerEvent } from "react";

import { useMessageActions } from "@/hooks/use-message-actions";
import { moveTargets } from "@/lib/moves";
import type { Conversation } from "@/lib/tauri";
import { useFolders } from "@/queries/folders";
import { useDragStore } from "@/stores/drag";
import { useUiStore } from "@/stores/ui";

/** Folder rows in the sidebar carry their folder id in this attribute (SidebarItem). */
export const DROP_FOLDER_ATTRIBUTE = "data-drop-folder";

/** How far the pointer moves before a press becomes a drag, so clicks stay clicks. */
const DRAG_THRESHOLD = 6;

/**
 * Dragging rows of the list onto a folder in the sidebar moves them there, like the move dialog.
 * Built on pointer events: HTML drag and drop does not reach the page in Tauri's window on
 * Windows, which takes drops for itself (files to attach).
 */
export function useMailDrag(messages: Conversation[]) {
  const { data: folders = [] } = useFolders();
  const { moveTo } = useMessageActions();

  return (messageId: string, event: PointerEvent) => {
    if (event.button !== 0 || event.pointerType === "touch") return;
    const startX = event.clientX;
    const startY = event.clientY;
    const store = useDragStore.getState();
    let dragging = false;

    const begin = () => {
      const { selectedIds } = useUiStore.getState();
      // A selected row takes the whole selection along, another row only itself.
      const messageIds = selectedIds.includes(messageId) ? selectedIds : [messageId];
      const chosen = new Set(messageIds);
      const targets = moveTargets(
        messages.filter((m) => chosen.has(m.id)),
        folders,
      );
      dragging = true;
      document.documentElement.dataset.mailDrag = "";
      store.set({
        messageIds,
        targets: new Set((targets ?? []).map((target) => target.folder.id)),
        x: startX,
        y: startY,
        overFolderId: null,
      });
    };

    const onMove = (move: globalThis.PointerEvent) => {
      if (!dragging) {
        if (Math.hypot(move.clientX - startX, move.clientY - startY) < DRAG_THRESHOLD) return;
        begin();
      }
      const drag = useDragStore.getState().drag;
      if (!drag) return;
      const over = document
        .elementFromPoint(move.clientX, move.clientY)
        ?.closest(`[${DROP_FOLDER_ATTRIBUTE}]`)
        ?.getAttribute(DROP_FOLDER_ATTRIBUTE);
      store.set({
        ...drag,
        x: move.clientX,
        y: move.clientY,
        overFolderId: over && drag.targets.has(over) ? over : null,
      });
    };

    const end = (drop: boolean) => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onCancel);
      window.removeEventListener("keydown", onKey, true);
      if (!dragging) return;
      delete document.documentElement.dataset.mailDrag;
      const drag = useDragStore.getState().drag;
      store.set(null);
      // The click that ends a drag neither opens the row nor the folder.
      window.addEventListener("click", swallow, { capture: true, once: true });
      window.setTimeout(() => window.removeEventListener("click", swallow, true), 0);
      const folder = drop && folders.find((f) => f.id === drag?.overFolderId);
      if (drag && folder) void moveTo(drag.messageIds, folder);
    };
    const onUp = () => end(true);
    const onCancel = () => end(false);
    const onKey = (key: KeyboardEvent) => {
      if (key.key !== "Escape" || !dragging) return;
      key.preventDefault();
      key.stopPropagation();
      end(false);
    };

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onCancel);
    window.addEventListener("keydown", onKey, true);
  };
}

function swallow(event: MouseEvent) {
  event.preventDefault();
  event.stopPropagation();
}
