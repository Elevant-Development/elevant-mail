import { Avatar } from "@/components/common/Avatar";
import { Icon } from "@/components/common/Icon";
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

/** One message as a card; unread mail gets a dot on the avatar. */
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
        "relative flex w-full gap-3 rounded-xl border border-transparent bg-row px-3 py-3 text-left",
        "transition-colors select-none hover:bg-row-hover focus-visible:-outline-offset-2",
        selected && "border-selection-border bg-selection hover:bg-selection",
      )}
    >
      <Avatar address={message.from} badge={unread} className="mt-0.5 size-10" />
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className={cn("truncate", unread ? "font-semibold" : "text-foreground/85")}>
            {displayName(message.from)}
          </span>
          {accountColor && (
            <span
              aria-hidden
              className="size-1.5 shrink-0 rounded-full"
              style={{ backgroundColor: accountColor }}
            />
          )}
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
        <p className="mt-0.5 truncate text-xs leading-relaxed text-muted-foreground">
          {message.snippet}
        </p>
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
