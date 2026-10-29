import { useEffect, useEffectEvent, useRef, useState } from "react";

import { Avatar } from "@/components/common/Avatar";
import { Icon } from "@/components/common/Icon";
import { IconButton } from "@/components/common/IconButton";
import { Button } from "@/components/ui/button";
import { useMessageActions } from "@/hooks/use-message-actions";
import { useNow } from "@/hooks/use-now";
import { useResolvedMode } from "@/hooks/use-resolved-mode";
import { describeError } from "@/lib/errors";
import { displayName, formatFullDate, formatListDate } from "@/lib/formatting";
import { locale, t, tCount } from "@/lib/i18n";
import { isDeceptiveLink } from "@/lib/links";
import { canArchive, deletesForGood } from "@/lib/moves";
import { type Address, api, type MessageSummary } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useFolders } from "@/queries/folders";
import { useConversation, useFullBody, useMessage } from "@/queries/messages";
import { useUiStore } from "@/stores/ui";

import { AttachmentList } from "./AttachmentList";
import { EmptyState } from "./EmptyState";
import { LinkAddress } from "./LinkAddress";
import { type DeceptiveLink, LinkWarningDialog } from "./LinkWarningDialog";
import { type MailLook, SafeHtmlMail } from "./SafeHtmlMail";
import { SelectionPanel } from "./SelectionPanel";

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

/** Marks the user's own reply from "Sent" in another folder's conversation. */
function SentTag() {
  return (
    <span className="shrink-0 rounded-md bg-muted px-1.5 py-px text-[11px] font-medium text-muted-foreground">
      {t("conversation.sent")}
    </span>
  );
}

interface MailViewProps {
  summary: MessageSummary;
  sent: boolean;
  /** In a conversation of several: the header folds the message up again. */
  onFold: (() => void) | null;
  /** The other look than the default, light for adapted mail and dark for designed mail. */
  switched: boolean;
  onSwitchLook: () => void;
  onLink: (url: string, text: string | null) => void;
  onHoverLink: (url: string | null) => void;
}

/**
 * One message, open: who wrote to whom, attachments and content. HTML mail is shown in a
 * sandboxed frame (SafeHtmlMail), plain text as text; nothing from a mail body reaches the app's
 * DOM as markup.
 */
function MailView({
  summary,
  sent,
  onFold,
  switched,
  onSwitchLook,
  onLink,
  onHoverLink,
}: MailViewProps) {
  const { data: message, isError } = useMessage(summary.id);
  const fullBody = useFullBody(message);
  const imagesAllowed = useUiStore((s) => s.remoteImagesAllowed[summary.id] ?? false);
  const allowRemoteImages = useUiStore((s) => s.allowRemoteImages);
  const dark = useResolvedMode() === "dark";

  const look: MailLook = message?.htmlStyled
    ? switched
      ? "inverted"
      : "paper"
    : switched
      ? "paper"
      : "adapted";
  const showsHtml = message?.bodyComplete === true && message.bodyHtml !== null;

  // Phrasing content only: in a conversation the header is the button that folds it up.
  const header = (
    <span className="flex items-start gap-3">
      <Avatar address={summary.from} />
      <span className="block min-w-0 flex-1 text-sm">
        <span className="flex flex-wrap items-baseline gap-x-2">
          <span className="font-medium">{displayName(summary.from)}</span>
          <span className="truncate text-xs text-muted-foreground">{summary.from.address}</span>
          {sent && <SentTag />}
        </span>
        {message && (
          <span className="block truncate text-xs text-muted-foreground">
            {t("reader.to")}: {formatAddresses(message.to)}
          </span>
        )}
        {message && message.cc.length > 0 && (
          <span className="block truncate text-xs text-muted-foreground">
            {t("reader.cc")}: {formatAddresses(message.cc)}
          </span>
        )}
      </span>
      <time
        dateTime={new Date(summary.dateSent).toISOString()}
        className="hidden shrink-0 text-xs text-muted-foreground tabular-nums sm:block"
      >
        {formatFullDate(summary.dateSent, locale)}
      </time>
    </span>
  );

  let body;
  if (isError) {
    body = <p className="text-sm text-muted-foreground">{t("reader.error")}</p>;
  } else if (!message) {
    body = <BodySkeleton />;
  } else if (showsHtml && message.bodyHtml !== null) {
    body = (
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
          onHoverLink={onHoverLink}
        />
      </>
    );
  } else {
    body = (
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
    );
  }

  return (
    <div>
      <div className="flex items-start gap-1">
        {onFold ? (
          <button
            type="button"
            aria-expanded
            onClick={onFold}
            className="-m-2 min-w-0 flex-1 rounded-lg p-2 text-left hover:bg-row-hover focus-visible:-outline-offset-2"
          >
            {header}
          </button>
        ) : (
          <div className="min-w-0 flex-1">{header}</div>
        )}
        {showsHtml && dark && (
          <IconButton
            label={look === "paper" ? t("reader.view.dark") : t("reader.view.paper")}
            icon={<Icon name={look === "paper" ? "moon" : "sun"} />}
            className="-mt-1.5 ml-2"
            onClick={onSwitchLook}
          />
        )}
      </div>
      {message && message.attachments.length > 0 && (
        <AttachmentList attachments={message.attachments} />
      )}
      <div className={onFold ? "mt-5" : "mt-6 border-t pt-6"}>{body}</div>
    </div>
  );
}

/** An older message of the conversation, folded to one line: who, the start, when. */
function FoldedMail({
  summary,
  sent,
  now,
  onUnfold,
}: {
  summary: MessageSummary;
  sent: boolean;
  now: number;
  onUnfold: () => void;
}) {
  const unread = !summary.isRead;
  return (
    <button
      type="button"
      aria-expanded={false}
      onClick={onUnfold}
      className="-mx-2 flex w-[calc(100%+1rem)] items-center gap-3 rounded-lg px-2 py-2 text-left text-sm hover:bg-row-hover focus-visible:-outline-offset-2"
    >
      <Avatar address={summary.from} badge={unread} className="size-7 text-[10px]" />
      <span
        className={cn(
          "w-36 shrink-0 truncate",
          unread ? "font-semibold" : "font-medium text-foreground/85",
        )}
      >
        {displayName(summary.from)}
      </span>
      {sent && <SentTag />}
      <span className="min-w-0 flex-1 truncate text-muted-foreground">{summary.snippet}</span>
      <time
        dateTime={new Date(summary.dateSent).toISOString()}
        className="shrink-0 text-xs text-muted-foreground tabular-nums"
      >
        {formatListDate(summary.dateSent, now, locale)}
      </time>
    </button>
  );
}

/**
 * The open row: a message, or a conversation with the newest message on top. The newest and
 * unread messages are open, the others folded to a line; the user's own replies from "Sent" are
 * part of it. The buttons act on the whole conversation in its folder.
 */
export function MessageReader({ className, onBack }: MessageReaderProps) {
  const selectedMessageId = useUiStore((s) => s.selectedMessageId);
  const several = useUiStore((s) => s.selectedIds.length > 1);
  const openMoveDialog = useUiStore((s) => s.openMoveDialog);
  const {
    data: conversation = [],
    isPending,
    isError,
    isPlaceholderData,
  } = useConversation(selectedMessageId);
  const { data: folders = [] } = useFolders();
  const { setRead, setFlagged, archive, trash } = useMessageActions();
  const now = useNow();

  // The conversation's messages in the opened folder, newest first: what the buttons act on.
  // While a reply that just arrived loads, the conversation shown is the one before it.
  const folderId = (conversation.find((m) => m.id === selectedMessageId) ?? conversation[0])
    ?.folderId;
  const own = conversation.filter((m) => m.folderId === folderId);
  const newest = own[0];
  const anyUnread = own.some((m) => !m.isRead);
  const anyFlagged = own.some((m) => m.isFlagged);
  const ids = selectedMessageId ? [selectedMessageId] : [];

  // Opening marks as read after a moment, so moving through the list with j/k does not. Once
  // per opening: marking it unread again (u) keeps it unread.
  const unreadId = anyUnread && !several && !isPlaceholderData ? selectedMessageId : null;
  const autoMarked = useRef<string | null>(null);
  const markRead = useEffectEvent((rowId: string) => {
    autoMarked.current = rowId;
    void setRead([rowId], true);
  });
  useEffect(() => {
    if (autoMarked.current !== selectedMessageId) autoMarked.current = null;
  }, [selectedMessageId]);
  useEffect(() => {
    if (unreadId === null || autoMarked.current === unreadId) return;
    const timer = window.setTimeout(() => markRead(unreadId), MARK_READ_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [unreadId]);

  // Open or folded, per message and session. Seen for the first time, the newest and the unread
  // ones open; reading them later does not fold them away.
  const [unfolded, setUnfolded] = useState<Record<string, boolean>>({});
  const unseen = isPlaceholderData ? [] : conversation.filter((m) => unfolded[m.id] === undefined);
  if (unseen.length > 0) {
    setUnfolded((current) => ({
      ...current,
      ...Object.fromEntries(unseen.map((m) => [m.id, m === conversation[0] || !m.isRead])),
    }));
  }
  const toggle = (messageId: string) =>
    setUnfolded((current) => ({ ...current, [messageId]: !current[messageId] }));

  const [switchedLook, setSwitchedLook] = useState<Record<string, boolean>>({});
  const [hoveredLink, setHoveredLink] = useState<string | null>(null);
  const [deceptiveLink, setDeceptiveLink] = useState<DeceptiveLink | null>(null);
  const [linkFailed, setLinkFailed] = useState(false);

  const openLink = (url: string) => {
    setDeceptiveLink(null);
    setLinkFailed(false);
    api.openLink(url).catch(() => setLinkFailed(true));
  };
  const onLink = (url: string, text: string | null) => {
    if (text !== null && isDeceptiveLink(url, text)) setDeceptiveLink({ url, text });
    else openLink(url);
  };

  if (several) return <SelectionPanel className={className} onBack={onBack} />;

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
  } else if (isError || !newest) {
    body = <EmptyState icon={<Icon name="alert" />} title={t("reader.error")} className="h-full" />;
  } else {
    const single = conversation.length === 1;
    // The conversation's first subject, without the "Re:" of the replies.
    const subject = own[own.length - 1]?.subject ?? newest.subject;
    body = (
      <div className="mx-auto max-w-3xl px-6 py-6">
        <h1 className="font-heading text-xl font-semibold tracking-tight text-balance">
          {subject ?? t("message.noSubject")}
        </h1>
        {!single && (
          <p className="mt-1 text-xs text-muted-foreground">
            {tCount("conversation.count", conversation.length)}
          </p>
        )}
        <ol className={cn("mt-5", !single && "divide-y")}>
          {conversation.map((summary) => {
            const sent = summary.folderId !== folderId;
            const open = single || (unfolded[summary.id] ?? summary === conversation[0]);
            return (
              <li key={summary.id} className={cn(!single && (open ? "py-5" : "py-1.5"))}>
                {open ? (
                  <MailView
                    summary={summary}
                    sent={sent}
                    onFold={single ? null : () => toggle(summary.id)}
                    switched={switchedLook[summary.id] ?? false}
                    onSwitchLook={() =>
                      setSwitchedLook((choice) => ({
                        ...choice,
                        [summary.id]: !choice[summary.id],
                      }))
                    }
                    onLink={onLink}
                    onHoverLink={setHoveredLink}
                  />
                ) : (
                  <FoldedMail
                    summary={summary}
                    sent={sent}
                    now={now}
                    onUnfold={() => toggle(summary.id)}
                  />
                )}
              </li>
            );
          })}
        </ol>
      </div>
    );
  }

  return (
    <article className={cn("relative flex min-h-0 min-w-0 flex-col", className)}>
      <div className="flex h-12 shrink-0 items-center gap-1 border-b px-3">
        {onBack && (
          <IconButton label={t("reader.back")} icon={<Icon name="back" />} onClick={onBack} />
        )}
        {newest && (
          <div className="ml-auto flex items-center gap-1">
            <IconButton
              label={anyUnread ? t("reader.markRead") : t("reader.markUnread")}
              icon={<Icon name={anyUnread ? "mailOpen" : "mail"} />}
              onClick={() => void setRead(ids, anyUnread)}
            />
            <IconButton
              label={anyFlagged ? t("reader.unflag") : t("reader.flag")}
              icon={
                <Icon
                  name="star"
                  filled={anyFlagged}
                  className={cn(anyFlagged && "text-amber-400")}
                />
              }
              onClick={() => void setFlagged(ids, !anyFlagged)}
            />
            <IconButton
              label={t("reader.move")}
              icon={<Icon name="move" />}
              onClick={() => openMoveDialog(ids)}
            />
            <IconButton
              label={t("reader.archive")}
              icon={<Icon name="archive" />}
              disabled={!canArchive(newest, folders)}
              onClick={() => void archive(ids)}
            />
            <IconButton
              label={
                deletesForGood(newest, folders) ? t("reader.deleteForever") : t("reader.delete")
              }
              icon={<Icon name="trash" />}
              onClick={() => void trash(ids)}
            />
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
