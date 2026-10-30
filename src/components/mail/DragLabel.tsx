import { createPortal } from "react-dom";

import { Icon } from "@/components/common/Icon";
import { tCount } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { useDragStore } from "@/stores/drag";

/** Follows the pointer while mail is dragged: how many, and whether the folder below takes it. */
export function DragLabel() {
  const drag = useDragStore((s) => s.drag);
  if (!drag) return null;
  return createPortal(
    <div
      aria-hidden
      className={cn(
        "pointer-events-none fixed top-0 left-0 z-50 flex items-center gap-1.5 rounded-lg border bg-popover px-2.5 py-1.5 text-sm font-medium text-popover-foreground shadow-lg",
        drag.overFolderId && "border-primary",
      )}
      style={{ transform: `translate(${drag.x + 14}px, ${drag.y + 10}px)` }}
    >
      <Icon name={drag.overFolderId ? "move" : "mail"} className="size-4 text-muted-foreground" />
      {tCount("drag.count", drag.messageIds.length)}
    </div>,
    document.body,
  );
}
