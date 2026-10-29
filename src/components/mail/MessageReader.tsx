import { Avatar } from "@/components/common/Avatar";
import { Icon } from "@/components/common/Icon";
import { IconButton } from "@/components/common/IconButton";
import { Button } from "@/components/ui/button";
import { describeError } from "@/lib/errors";
import { displayName, formatFullDate } from "@/lib/formatting";
import { locale, t } from "@/lib/i18n";
import type { Address } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useFullBody, useMessage } from "@/queries/messages";
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

function ReaderSkeleton() {
  return (
    <div aria-hidden className="mx-auto flex max-w-3xl animate-pulse flex-col gap-4 px-6 py-6">
      <div className="h-6 w-3/5 rounded bg-muted" />
      <div className="flex items-center gap-3">
        <div className="size-9 rounded-full bg-muted" />
        <div className="h-3 w-1/3 rounded bg-muted" />
      </div>
      <div className="mt-4 h-3 w-full rounded bg-muted/70" />
      <div className="h-3 w-11/12 rounded bg-muted/70" />
      <div className="h-3 w-4/5 rounded bg-muted/70" />
    </div>
  );
}

function BodySkeleton() {
  return (
    <div role="status" className="flex animate-pulse flex-col gap-3">
      <span className="sr-only">{t("reader.loadingBody")}</span>
      <div aria-hidden className="h-3 w-full rounded bg-muted/70" />
      <div aria-hidden className="h-3 w-11/12 rounded bg-muted/70" />
      <div aria-hidden className="h-3 w-4/5 rounded bg-muted/70" />
      <div aria-hidden className="h-3 w-2/3 rounded bg-muted/70" />
    </div>
  );
}

function BodyError({ error, onRetry }: { error: unknown; onRetry: () => void }) {
  const { message, action } = describeError(error);
  return (
    <div role="alert" className="flex flex-col items-start gap-1 font-sans text-sm">
      <p className="font-medium">{t("reader.bodyError")}</p>
      <p className="text-muted-foreground">
        {message}
        {action && ` ${action}`}
      </p>
      <Button variant="outline" size="sm" className="mt-2" onClick={onRetry}>
        <Icon name="refresh" />
        {t("sync.retry")}
      </Button>
    </div>
  );
}

/**
 * Shows the selected message as plain text. HTML mails get their sandboxed renderer
 * (SafeHtmlMail) later; until then nothing from a mail body reaches the DOM as markup.
 */
export function MessageReader({ className, onBack }: MessageReaderProps) {
  const selectedMessageId = useUiStore((s) => s.selectedMessageId);
  const { data: message, isPending, isError } = useMessage(selectedMessageId);
  const fullBody = useFullBody(message);

  if (!selectedMessageId) {
    return (
      <EmptyState
        icon={<Icon name="mail" />}
        title={t("reader.emptyTitle")}
        hint={t("reader.emptyHint")}
        className={className}
      />
    );
  }

  let body;
  if (isPending) {
    body = <ReaderSkeleton />;
  } else if (isError || !message) {
    body = <EmptyState icon={<Icon name="alert" />} title={t("reader.error")} className="h-full" />;
  } else {
    body = (
      <div className="mx-auto max-w-3xl px-6 py-6">
        <h1 className="font-heading text-xl font-semibold tracking-tight text-balance">
          {message.subject ?? t("message.noSubject")}
        </h1>

        <div className="mt-5 flex items-start gap-3">
          <Avatar address={message.from} />
          <div className="min-w-0 flex-1 text-sm">
            <div className="flex flex-wrap items-baseline gap-x-2">
              <span className="font-medium">{displayName(message.from)}</span>
              <span className="truncate text-xs text-muted-foreground">{message.from.address}</span>
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
            className="hidden shrink-0 text-xs text-muted-foreground tabular-nums sm:block"
          >
            {formatFullDate(message.dateSent, locale)}
          </time>
        </div>

        <div className="mt-6 max-w-[68ch] border-t pt-6 font-reading text-(length:--reading-size) leading-relaxed break-words whitespace-pre-wrap select-text">
          {message.bodyComplete ? (
            (message.bodyText ?? t("reader.noBody"))
          ) : fullBody.isError ? (
            <BodyError error={fullBody.error} onRetry={() => void fullBody.refetch()} />
          ) : (
            <BodySkeleton />
          )}
        </div>
      </div>
    );
  }

  return (
    <article className={cn("flex min-h-0 min-w-0 flex-col", className)}>
      <div className="flex h-12 shrink-0 items-center gap-1 border-b px-3">
        {onBack && (
          <IconButton label={t("reader.back")} icon={<Icon name="back" />} onClick={onBack} />
        )}
        {message && (
          <div className="ml-auto flex items-center gap-1">
            <IconButton
              label={message.isRead ? t("reader.markUnread") : t("reader.markRead")}
              icon={<Icon name={message.isRead ? "mail" : "mailOpen"} />}
              disabled
            />
            <IconButton
              label={message.isFlagged ? t("reader.unflag") : t("reader.flag")}
              icon={
                <Icon
                  name="star"
                  filled={message.isFlagged}
                  className={cn(message.isFlagged && "text-amber-400")}
                />
              }
              disabled
            />
            <IconButton label={t("reader.archive")} icon={<Icon name="archive" />} disabled />
            <IconButton label={t("reader.delete")} icon={<Icon name="trash" />} disabled />
          </div>
        )}
      </div>
      {/* READER_ATTRIBUTE: lets the shortcuts tell when the focus is in the reader. Focusable,
          so Enter can move there and the keyboard scrolls the mail. */}
      <div
        data-reader-body=""
        tabIndex={0}
        aria-label={t("reader.content")}
        className="min-h-0 flex-1 overflow-y-auto focus-visible:-outline-offset-2"
      >
        {body}
      </div>
    </article>
  );
}
