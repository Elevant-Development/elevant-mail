import { Avatar } from "@/components/common/Avatar";
import { Icon } from "@/components/common/Icon";
import { displayName, formatListDate } from "@/lib/formatting";
import { locale, t } from "@/lib/i18n";
import type { MouseEvent } from "react";

import type { MessageSummary } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/stores/ui";

interface MessageListItemProps {
  message: MessageSummary;
  selected: boolean;
  now: number;
  /** Set in the unified inbox: a stripe on the card's edge tells accounts apart. */
  accountColor?: string;
  /** With the click, so Ctrl and Shift can add to the selection. */
  onSelect: (event: MouseEvent) => void;
}

/** One message as a card; unread mail gets a dot on the avatar. */
export function MessageListItem({
  message,
  selected,
  now,
  accountColor,
  onSelect,
}: MessageListItemProps) {
  const compact = useUiStore((s) => s.density === "compact");
  const unread = !message.isRead;

  return (
    <button
      type="button"
      data-message-id={message.id}
      onClick={onSelect}
      aria-current={selected || undefined}
      className={cn(
        "relative flex w-full rounded-xl border border-transparent bg-row px-3 text-left",
        "transition-colors select-none hover:bg-row-hover focus-visible:-outline-offset-2",
        compact ? "items-center gap-2.5 py-2" : "gap-3 py-3",
        selected && "border-selection-border bg-selection hover:bg-selection",
      )}
    >
      {accountColor && (
        <span
          aria-hidden
          className="absolute top-2.5 bottom-2.5 left-0 w-[3px] rounded-r-full"
          style={{ backgroundColor: accountColor }}
        />
      )}
      <Avatar
        address={message.from}
        badge={unread}
        className={compact ? "size-8 text-[11px]" : "mt-0.5 size-10"}
      />
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className={cn("truncate", unread ? "font-semibold" : "text-foreground/85")}>
            {displayName(message.from)}
          </span>
          <span
            className={cn(
              "ml-auto shrink-0 text-xs tabular-nums",
              unread ? "font-medium text-unread-text" : "text-muted-foreground",
            )}
          >
            {formatListDate(message.dateSent, now, locale)}
          </span>
        </div>
        <div className="flex items-center gap-1.5">
          <span className={cn("truncate", unread ? "font-medium" : "text-foreground/75")}>
            {message.subject ?? t("message.noSubject")}
          </span>
          <span className="ml-auto flex shrink-0 items-center gap-1 text-muted-foreground">
            {message.hasAttachments && <Icon name="paperclip" className="size-3.5" />}
            {message.isFlagged && <Icon name="star" filled className="size-3.5 text-amber-400" />}
          </span>
        </div>
        {!compact && (
          <p className="mt-0.5 truncate text-xs leading-relaxed text-muted-foreground">
            {message.snippet}
          </p>
        )}
      </div>
      <span className="sr-only">
        {[
          unread && t("message.unread"),
          message.isFlagged && t("message.flagged"),
          message.hasAttachments && t("message.attachment"),
        ]
          .filter(Boolean)
          .join(", ")}
      </span>
    </button>
  );
}
