import { useEffect } from "react";

import { Icon } from "@/components/common/Icon";
import { useMessageActions } from "@/hooks/use-message-actions";
import { t } from "@/lib/i18n";
import { UNDO_WINDOW_MS } from "@/lib/tauri";
import { useUndoStore } from "@/stores/undo";

/** Gone a little before the move reaches the server, so "Undo" never comes too late. */
const SHOWN_MS = UNDO_WINDOW_MS - 2000;

/** What just happened, at the bottom of the window, with the way back (also z). */
export function UndoToast() {
  const notice = useUndoStore((s) => s.notice);
  const clear = useUndoStore((s) => s.clear);
  const { undo } = useMessageActions();

  const key = notice?.key;
  useEffect(() => {
    if (key === undefined) return;
    const timer = window.setTimeout(clear, SHOWN_MS);
    return () => window.clearTimeout(timer);
  }, [key, clear]);

  if (!notice) return null;
  return (
    <div className="pointer-events-none fixed inset-x-0 bottom-4 z-40 flex justify-center px-4">
      <div
        role="status"
        className="pointer-events-auto flex max-w-md min-w-0 items-center gap-3 rounded-xl border bg-popover py-2 pr-2 pl-4 text-sm text-popover-foreground shadow-[0_4px_16px_var(--shadow-color)]"
      >
        <span className="min-w-0 flex-1">{notice.text}</span>
        {notice.messageIds && (
          <button
            type="button"
            onClick={() => void undo()}
            className="shrink-0 rounded-md px-2 py-1 font-medium text-primary hover:bg-accent"
          >
            {t("undo.action")}
          </button>
        )}
        <button
          type="button"
          aria-label={t("dialog.close")}
          onClick={clear}
          className="flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground [&_svg]:size-3.5"
        >
          <Icon name="close" />
        </button>
      </div>
    </div>
  );
}
