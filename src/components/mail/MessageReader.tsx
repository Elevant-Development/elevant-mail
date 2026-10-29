import { Archive, ArrowLeft, Mail, MailOpen, Star, Trash2 } from "lucide-react";

import { Avatar } from "@/components/common/Avatar";
import { IconButton } from "@/components/common/IconButton";
import { displayName, formatFullDate } from "@/lib/formatting";
import { locale, t } from "@/lib/i18n";
import type { Address } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useMessage } from "@/queries/messages";
import { useUiStore } from "@/stores/ui";

import { EmptyState } from "./EmptyState";

interface MessageReaderProps {
  className?: string;
  /** Set in the narrow layout, where the reader replaces the list. */
  onBack?: () => void;
}

function formatAddresses(addresses: Address[]): string {
  return addresses.map(displayName).join(", ");
}

/**
 * Shows the selected message as plain text. HTML mails get their sandboxed renderer
 * (SafeHtmlMail) later; until then nothing from a mail body reaches the DOM as markup.
 */
export function MessageReader({ className, onBack }: MessageReaderProps) {
  const selectedMessageId = useUiStore((s) => s.selectedMessageId);
  const { data: message } = useMessage(selectedMessageId);

  if (!selectedMessageId) {
    return (
      <EmptyState
        icon={<Mail />}
        title={t("reader.emptyTitle")}
        hint={t("reader.emptyHint")}
        className={className}
      />
    );
  }

  return (
    <article className={cn("flex min-h-0 min-w-0 flex-col", className)}>
      <div className="flex h-12 shrink-0 items-center gap-1 border-b px-3">
        {onBack && <IconButton label={t("reader.back")} icon={<ArrowLeft />} onClick={onBack} />}
        {message && (
          <div className="ml-auto flex items-center gap-1">
            <IconButton
              label={message.isRead ? t("reader.markUnread") : t("reader.markRead")}
              icon={message.isRead ? <Mail /> : <MailOpen />}
              disabled
            />
            <IconButton
              label={message.isFlagged ? t("reader.unflag") : t("reader.flag")}
              icon={<Star className={cn(message.isFlagged && "fill-amber-400 text-amber-400")} />}
              disabled
            />
            <IconButton label={t("reader.archive")} icon={<Archive />} disabled />
            <IconButton label={t("reader.delete")} icon={<Trash2 />} disabled />
          </div>
        )}
      </div>

      {message && (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <div className="mx-auto max-w-3xl px-6 py-6">
            <h1 className="text-xl font-semibold text-balance">
              {message.subject ?? t("message.noSubject")}
            </h1>

            <div className="mt-5 flex items-start gap-3">
              <Avatar address={message.from} />
              <div className="min-w-0 flex-1 text-sm">
                <div className="flex flex-wrap items-baseline gap-x-2">
                  <span className="font-medium">{displayName(message.from)}</span>
                  <span className="truncate text-xs text-muted-foreground">
                    {message.from.address}
                  </span>
                </div>
                <div className="truncate text-xs text-muted-foreground">
                  {t("reader.to")}: {formatAddresses(message.to)}
                </div>
                {message.cc.length > 0 && (
                  <div className="truncate text-xs text-muted-foreground">
                    {t("reader.cc")}: {formatAddresses(message.cc)}
                  </div>
                )}
              </div>
              <time
                dateTime={new Date(message.dateSent).toISOString()}
                className="hidden shrink-0 text-xs text-muted-foreground sm:block"
              >
                {formatFullDate(message.dateSent, locale)}
              </time>
            </div>

            <div className="mt-6 border-t pt-6 text-[15px] leading-relaxed break-words whitespace-pre-wrap select-text">
              {message.bodyText ?? t("reader.noBody")}
            </div>
          </div>
        </div>
      )}
    </article>
  );
}
