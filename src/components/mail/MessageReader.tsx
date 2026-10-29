import { useEffect, useEffectEvent, useRef, useState } from "react";

import { Avatar } from "@/components/common/Avatar";
import { Icon } from "@/components/common/Icon";
import { IconButton } from "@/components/common/IconButton";
import { Button } from "@/components/ui/button";
import { useMessageActions } from "@/hooks/use-message-actions";
import { useResolvedMode } from "@/hooks/use-resolved-mode";
import { describeError } from "@/lib/errors";
import { displayName, formatFullDate } from "@/lib/formatting";
import { locale, t } from "@/lib/i18n";
import { isDeceptiveLink } from "@/lib/links";
import { type Address, api } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useFullBody, useMessage } from "@/queries/messages";
import { useUiStore } from "@/stores/ui";

import { AttachmentList } from "./AttachmentList";
import { EmptyState } from "./EmptyState";
import { LinkAddress } from "./LinkAddress";
import { type DeceptiveLink, LinkWarningDialog } from "./LinkWarningDialog";
import { type MailLook, SafeHtmlMail } from "./SafeHtmlMail";

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

/** How long a mail has to stay open before it counts as read. */
const MARK_READ_DELAY_MS = 1000;

function RemoteImagesBar({ count, onLoad }: { count: number; onLoad: () => void }) {
  return (
    <div className="mb-4 flex items-center gap-3 rounded-xl border bg-muted/40 px-3 py-2 font-sans text-xs text-muted-foreground">
      <Icon name="imageOff" className="size-4 shrink-0" />
      <p className="min-w-0 flex-1">{t("reader.remoteImages", { count })}</p>
      <Button variant="outline" size="sm" className="shrink-0" onClick={onLoad}>
        {t("reader.remoteImages.load")}
      </Button>
    </div>
  );
}

/**
 * The selected message. HTML mail is shown in a sandboxed frame (SafeHtmlMail), plain text as
 * text; nothing from a mail body reaches the app's DOM as markup.
 */
export function MessageReader({ className, onBack }: MessageReaderProps) {
  const selectedMessageId = useUiStore((s) => s.selectedMessageId);
  const { data: message, isPending, isError } = useMessage(selectedMessageId);
  const fullBody = useFullBody(message);
  const imagesAllowed = useUiStore((s) =>
    selectedMessageId ? (s.remoteImagesAllowed[selectedMessageId] ?? false) : false,
  );
  const allowRemoteImages = useUiStore((s) => s.allowRemoteImages);
  const { setRead, setFlagged } = useMessageActions();

  // Opening marks as read after a moment, so moving through the list with j/k does not. Once
  // per opening: marking it unread again (u) keeps it unread.
  const unreadId = message && !message.isRead ? message.id : null;
  const autoMarked = useRef<string | null>(null);
  const markRead = useEffectEvent((messageId: string) => {
    autoMarked.current = messageId;
    void setRead([messageId], true);
  });
  useEffect(() => {
    if (autoMarked.current !== selectedMessageId) autoMarked.current = null;
  }, [selectedMessageId]);
  useEffect(() => {
    if (unreadId === null || autoMarked.current === unreadId) return;
    const timer = window.setTimeout(() => markRead(unreadId), MARK_READ_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [unreadId]);
  const dark = useResolvedMode() === "dark";
  // Per mail and session: the other look than the default, light for adapted mail and dark
  // for designed mail.
  const [switchedLook, setSwitchedLook] = useState<Record<string, boolean>>({});
  const [hoveredLink, setHoveredLink] = useState<string | null>(null);
  const [deceptiveLink, setDeceptiveLink] = useState<DeceptiveLink | null>(null);
  const [linkFailed, setLinkFailed] = useState(false);

  const switched = message ? (switchedLook[message.id] ?? false) : false;
  const look: MailLook = message?.htmlStyled
    ? switched
      ? "inverted"
      : "paper"
    : switched
      ? "paper"
      : "adapted";
  const showsHtml = message?.bodyComplete === true && message.bodyHtml !== null;

  const openLink = (url: string) => {
    setDeceptiveLink(null);
    setLinkFailed(false);
    api.openLink(url).catch(() => setLinkFailed(true));
  };
  const onLink = (url: string, text: string | null) => {
    if (text !== null && isDeceptiveLink(url, text)) setDeceptiveLink({ url, text });
    else openLink(url);
  };

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

        {message.attachments.length > 0 && <AttachmentList attachments={message.attachments} />}

        <div className="mt-6 border-t pt-6">
          {showsHtml && message.bodyHtml !== null ? (
            <>
              {message.remoteImageCount > 0 && !imagesAllowed && (
                <RemoteImagesBar
                  count={message.remoteImageCount}
                  onLoad={() => allowRemoteImages(message.id)}
                />
              )}
              <SafeHtmlMail
                html={message.bodyHtml}
                look={look}
                onLink={onLink}
                onHoverLink={setHoveredLink}
              />
            </>
          ) : (
            <div className="max-w-[68ch] font-reading text-(length:--reading-size) leading-relaxed break-words whitespace-pre-wrap select-text">
              {message.bodyComplete ? (
                (message.bodyText ?? t("reader.noBody"))
              ) : fullBody.isError && !message.bodyText ? (
                <BodyError error={fullBody.error} onRetry={() => void fullBody.refetch()} />
              ) : message.bodyText ? (
                // Cached before HTML was kept: the text shows until the full mail is in.
                message.bodyText
              ) : (
                <BodySkeleton />
              )}
            </div>
          )}
        </div>
      </div>
    );
  }

  return (
    <article className={cn("relative flex min-h-0 min-w-0 flex-col", className)}>
      <div className="flex h-12 shrink-0 items-center gap-1 border-b px-3">
        {onBack && (
          <IconButton label={t("reader.back")} icon={<Icon name="back" />} onClick={onBack} />
        )}
        {message && (
          <div className="ml-auto flex items-center gap-1">
            {showsHtml && dark && (
              <IconButton
                label={look === "paper" ? t("reader.view.dark") : t("reader.view.paper")}
                icon={<Icon name={look === "paper" ? "moon" : "sun"} />}
                onClick={() =>
                  setSwitchedLook((choice) => ({ ...choice, [message.id]: !switched }))
                }
              />
            )}
            <IconButton
              label={message.isRead ? t("reader.markUnread") : t("reader.markRead")}
              icon={<Icon name={message.isRead ? "mail" : "mailOpen"} />}
              onClick={() => void setRead([message.id], !message.isRead)}
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
              onClick={() => void setFlagged([message.id], !message.isFlagged)}
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
      {(hoveredLink || linkFailed) && (
        <div
          role="status"
          className="pointer-events-none absolute bottom-2 left-2 max-w-[calc(100%-1rem)] truncate rounded-lg border bg-popover px-2.5 py-1 text-xs shadow-[0_1px_3px_var(--shadow-color)]"
        >
          {hoveredLink ? (
            <LinkAddress url={hoveredLink} />
          ) : (
            <span className="text-destructive">{t("link.openFailed")}</span>
          )}
        </div>
      )}
      <LinkWarningDialog
        link={deceptiveLink}
        onOpen={openLink}
        onClose={() => setDeceptiveLink(null)}
      />
    </article>
  );
}
