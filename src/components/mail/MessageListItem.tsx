import { Paperclip, Star } from "lucide-react";

import { Avatar } from "@/components/common/Avatar";
import { displayName, formatListDate } from "@/lib/formatting";
import { locale, t } from "@/lib/i18n";
import type { MessageSummary } from "@/lib/tauri";
import { cn } from "@/lib/utils";

interface MessageListItemProps {
  message: MessageSummary;
  selected: boolean;
  now: number;
  /** Set in the unified inbox to tell accounts apart. */
  accountColor?: string;
  onSelect: () => void;
}

export function MessageListItem({
  message,
  selected,
  now,
  accountColor,
  onSelect,
}: MessageListItemProps) {
  const unread = !message.isRead;

  return (
    <button
      type="button"
      onClick={onSelect}
      aria-current={selected || undefined}
      className={cn(
        "relative flex w-full gap-3 border-b border-border/60 px-4 py-2.5 text-left select-none",
        "transition-colors hover:bg-accent/50 focus-visible:-outline-offset-2",
        selected && "bg-selection hover:bg-selection",
      )}
    >
      {unread && (
        <span
          aria-hidden
          className="absolute top-1/2 left-1.5 size-1.5 -translate-y-1/2 rounded-full bg-unread"
        />
      )}
      <Avatar address={message.from} className="mt-0.5" />
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          <span className={cn("truncate", unread ? "font-semibold" : "text-foreground/85")}>
            {displayName(message.from)}
          </span>
          {accountColor && (
            <span
              aria-hidden
              className="size-1.5 shrink-0 self-center rounded-full"
              style={{ backgroundColor: accountColor }}
            />
          )}
          <span
            className={cn(
              "ml-auto shrink-0 text-xs tabular-nums",
              unread ? "font-medium text-unread" : "text-muted-foreground",
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
            {message.hasAttachments && <Paperclip className="size-3.5" />}
            {message.isFlagged && <Star className="size-3.5 fill-amber-400 text-amber-400" />}
          </span>
        </div>
        <p className="truncate text-xs text-muted-foreground">{message.snippet}</p>
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
