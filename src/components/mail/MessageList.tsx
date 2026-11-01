import { defaultRangeExtractor, useVirtualizer } from "@tanstack/react-virtual";
import { type MouseEvent, type ReactNode, useEffect, useEffectEvent, useMemo, useRef } from "react";

import { Icon } from "@/components/common/Icon";
import { IconButton } from "@/components/common/IconButton";
import { Button } from "@/components/ui/button";
import { useMailDrag } from "@/hooks/use-mail-drag";
import { useNow } from "@/hooks/use-now";
import { useSyncActions } from "@/hooks/use-sync";
import { isSignInError } from "@/lib/errors";
import { mailboxTitle, unifiedInboxes } from "@/lib/folders";
import { t } from "@/lib/i18n";
import { focusListItem, MESSAGE_ATTRIBUTE } from "@/lib/keyboard";
import { type Conversation, type SyncProgress, UNIFIED_INBOX } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useAccounts } from "@/queries/accounts";
import { useFolders } from "@/queries/folders";
import { useVisibleMessages } from "@/queries/messages";
import { useSyncStore } from "@/stores/sync";
import { useUiStore } from "@/stores/ui";

import { EmptyState } from "./EmptyState";
import { ListFilters } from "./ListFilters";
import { MessageContextMenu } from "./MessageContextMenu";
import { MessageListItem } from "./MessageListItem";
import { SearchOnServer } from "./SearchOnServer";

interface MessageListProps {
  className?: string;
}

function ListSkeleton() {
  return (
    <div aria-hidden className="flex flex-col gap-4 p-4">
      {Array.from({ length: 6 }, (_, index) => (
        <div key={index} className="flex animate-pulse flex-col gap-2">
          <div className="h-3 w-2/5 rounded bg-muted" />
          <div className="h-3 w-4/5 rounded bg-muted" />
          <div className="h-2.5 w-3/5 rounded bg-muted/70" />
        </div>
      ))}
    </div>
  );
}

function progressLabel(progress: SyncProgress): string {
  switch (progress.stage) {
    case "connecting":
      return t("sync.stage.connecting");
    case "checking":
      return t("sync.stage.checking");
    default:
      return progress.total > 0
        ? t("sync.stage.fetching", { done: progress.done, total: progress.total })
        : t("sync.loading");
  }
}

interface MessageRowsProps {
  messages: Conversation[];
  now: number;
  accountColor: (accountId: string) => string | undefined;
  /** The accounts' addresses, so the user's own replies read "me". */
  ownAddresses: Set<string>;
  /** Below the last message, e.g. the button for older mail. */
  footer: ReactNode;
  /** More rows are cached than loaded; `loadMore` gets the next page. */
  hasMore: boolean;
  loadMore: () => void;
}

/** Rows from the end at which the next page loads, so scrolling rarely reaches the end. */
const LOAD_AHEAD = 60;

/**
 * Only the rows in view are rendered, so a folder with thousands of mails scrolls smoothly; the
 * next page loads while scrolling towards the end.
 */
function MessageRows({
  messages,
  now,
  accountColor,
  ownAddresses,
  footer,
  hasMore,
  loadMore,
}: MessageRowsProps) {
  const selectedIds = useUiStore((s) => s.selectedIds);
  // The end that moves with Shift+j/k; otherwise the open message.
  const focusedId = useUiStore((s) => s.selectionEnd ?? s.selectedMessageId);
  const selectMessage = useUiStore((s) => s.selectMessage);
  const toggleSelected = useUiStore((s) => s.toggleSelected);
  const selectRange = useUiStore((s) => s.selectRange);
  const compact = useUiStore((s) => s.density === "compact");
  const scrollRef = useRef<HTMLDivElement>(null);
  const selected = useMemo(() => new Set(selectedIds), [selectedIds]);
  // Tab reaches the list once, at the open (or last moved to) message, else at the first.
  const focusedIndex = focusedId === null ? -1 : messages.findIndex((m) => m.id === focusedId);
  const tabStopIndex = Math.max(focusedIndex, 0);

  // The React Compiler is not in use; it would skip this component anyway.
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count: messages.length,
    getScrollElement: () => scrollRef.current,
    // Rows are measured once rendered; the estimate only sizes the scrollbar before that.
    estimateSize: () => (compact ? 58 : 88),
    getItemKey: (index) => messages[index]?.id ?? index,
    gap: compact ? 4 : 6,
    paddingStart: 8,
    paddingEnd: 8,
    overscan: 6,
    // The tab stop stays rendered when scrolled out of view, so Tab still finds the list.
    rangeExtractor: (range) => {
      const indexes = defaultRangeExtractor(range);
      if (tabStopIndex < range.count && !indexes.includes(tabStopIndex)) {
        indexes.push(tabStopIndex);
        indexes.sort((a, b) => a - b);
      }
      return indexes;
    },
  });

  // Measured heights belong to the old density.
  useEffect(() => virtualizer.measure(), [compact, virtualizer]);

  const virtualRows = virtualizer.getVirtualItems();
  const lastShown = virtualizer.range?.endIndex ?? -1;
  const nearEnd = hasMore && lastShown >= messages.length - LOAD_AHEAD;
  const loadMoreNow = useEffectEvent(loadMore);
  useEffect(() => {
    if (nearEnd) loadMoreNow();
  }, [nearEnd, messages.length]);

  // A selection made with the keyboard may be out of view, and the focus follows it.
  const revealSelection = useEffectEvent(() => {
    if (focusedId === null || focusedIndex === -1) return;
    virtualizer.scrollToIndex(focusedIndex, { align: "auto" });
    if (!scrollRef.current?.contains(document.activeElement)) return;
    const frame = requestAnimationFrame(() => focusListItem(focusedId, { preventScroll: true }));
    return () => cancelAnimationFrame(frame);
  });
  useEffect(() => revealSelection(), [focusedId]);

  const startDrag = useMailDrag(messages);

  const select = (messageId: string, event: MouseEvent) => {
    if (event.shiftKey) {
      selectRange(
        messages.map((m) => m.id),
        messageId,
      );
    } else if (event.ctrlKey || event.metaKey) {
      toggleSelected(messageId);
    } else {
      selectMessage(messageId);
    }
  };

  return (
    <MessageContextMenu messages={messages}>
      <div
        ref={scrollRef}
        // LIST_ATTRIBUTE: lets the shortcuts tell when the focus is in the list.
        data-message-list=""
        data-focus-group=""
        className="min-h-0 flex-1 overflow-y-auto"
      >
        <ul className="relative" style={{ height: virtualizer.getTotalSize() }}>
          {virtualRows.map((row) => {
            const message = messages[row.index];
            if (!message) return null;
            return (
              <li
                key={row.key}
                ref={virtualizer.measureElement}
                data-index={row.index}
                {...{ [MESSAGE_ATTRIBUTE]: message.id }}
                // -1: the size is not known while more pages follow.
                aria-setsize={hasMore ? -1 : messages.length}
                aria-posinset={row.index + 1}
                className="absolute inset-x-2 top-0"
                style={{ transform: `translateY(${row.start}px)` }}
              >
                <MessageListItem
                  message={message}
                  selected={selected.has(message.id)}
                  tabbable={row.index === tabStopIndex}
                  now={now}
                  accountColor={accountColor(message.accountId)}
                  ownAddresses={ownAddresses}
                  onSelect={(event) => select(message.id, event)}
                  onPointerDown={(event) => startDrag(message.id, event)}
                />
              </li>
            );
          })}
        </ul>
        {hasMore ? (
          <div className="flex justify-center pb-3 text-muted-foreground">
            <Icon name="spinner" className="size-4 animate-spin" />
          </div>
        ) : (
          footer
        )}
      </div>
    </MessageContextMenu>
  );
}

export function MessageList({ className }: MessageListProps) {
  const mailboxId = useUiStore((s) => s.selectedMailboxId);
  const { data: accounts = [] } = useAccounts();
  const { data: folders = [] } = useFolders();
  const {
    data,
    isPending,
    isError,
    searching,
    filtered,
    hasNextPage,
    isFetchingNextPage,
    fetchNextPage,
  } = useVisibleMessages();
  const clearListFilters = useUiStore((s) => s.clearListFilters);
  const messages = useMemo(() => data ?? [], [data]);
  const searchQuery = useUiStore((s) => s.searchQuery.trim());
  const now = useNow();
  const { syncAccount, syncFolder, loadOlder } = useSyncActions();

  const folder = folders.find((f) => f.id === mailboxId);
  // The unified inbox spans all accounts, a folder belongs to one.
  const accountIds = folder ? [folder.accountId] : accounts.map((a) => a.id);
  const progressById = useSyncStore((s) => s.progress);
  const errorsById = useSyncStore((s) => s.errors);
  const openSettings = useUiStore((s) => s.openSettings);
  const loadingOlder = useSyncStore((s) => (folder ? (s.loadingOlder[folder.id] ?? false) : false));
  const progress = accountIds.map((id) => progressById[id]).find(Boolean);
  const errorEntry = accountIds
    .map((id) => [id, errorsById[id]] as const)
    .find(([, error]) => error !== undefined);

  // A reply that arrived or mail brought back by undo: the selection stays on its conversation.
  const followRows = useUiStore((s) => s.followRows);
  const selectedIds = useUiStore((s) => s.selectedIds);
  useEffect(() => {
    if (data) followRows(data);
  }, [data, selectedIds, followRows]);

  const title = searching ? t("search.title") : mailboxTitle(mailboxId, folders, accounts);
  // From the folders: the list may not have loaded all of it.
  const unreadCount = (folder ? [folder] : unifiedInboxes(folders, accounts)).reduce(
    (sum, f) => sum + f.unreadCount,
    0,
  );
  const accountColors = new Map(accounts.map((a) => [a.id, a.color]));
  // Search results span all accounts, the unified inbox those in it.
  const showAccountColor = searching
    ? accounts.length > 1
    : mailboxId === UNIFIED_INBOX && accounts.filter((a) => a.inUnifiedInbox).length > 1;
  const listedCount = messages.reduce((sum, row) => sum + row.messageIds.length, 0);
  // Older mail on the server, once the list shows all that is cached; not while filtering,
  // when the list shows less than is cached.
  const hasOlder =
    !searching &&
    !filtered &&
    !hasNextPage &&
    folder !== undefined &&
    folder.totalCount > listedCount;
  const ownAddresses = new Set(accounts.map((a) => a.emailAddress.toLowerCase()));

  const refresh = () => {
    if (folder && folder.role !== "inbox") {
      void syncFolder(folder, { force: true });
    } else {
      for (const id of accountIds) void syncAccount(id);
    }
  };

  let content;
  if (isPending) {
    content = <ListSkeleton />;
  } else if (isError) {
    content = (
      <EmptyState icon={<Icon name="alert" />} title={t("list.error")} className="flex-1" />
    );
  } else if (messages.length === 0 && searching) {
    content = (
      <EmptyState
        icon={<Icon name="search" />}
        title={t("search.empty")}
        hint={t("search.emptyHint")}
        className="flex-1"
      >
        <SearchOnServer />
      </EmptyState>
    );
  } else if (messages.length === 0 && filtered) {
    content = (
      <EmptyState icon={<Icon name="mail" />} title={t("filter.empty")} className="flex-1">
        <Button variant="ghost" size="sm" onClick={clearListFilters}>
          {t("filter.clear")}
        </Button>
      </EmptyState>
    );
  } else if (messages.length === 0) {
    content = progress ? (
      <EmptyState
        icon={<Icon name="spinner" className="animate-spin" />}
        title={progressLabel(progress)}
        className="flex-1"
      />
    ) : (
      <EmptyState icon={<Icon name="inbox" />} title={t("list.empty")} className="flex-1" />
    );
  } else {
    content = (
      <MessageRows
        // A new mailbox or search starts at the top.
        key={searching ? `search:${searchQuery}` : mailboxId}
        messages={messages}
        now={now}
        accountColor={(accountId) => (showAccountColor ? accountColors.get(accountId) : undefined)}
        ownAddresses={ownAddresses}
        hasMore={hasNextPage}
        loadMore={() => {
          if (!isFetchingNextPage) void fetchNextPage();
        }}
        footer={
          searching ? (
            <SearchOnServer />
          ) : (
            hasOlder && (
              <div className="px-2 pb-2">
                <Button
                  variant="ghost"
                  size="sm"
                  className="w-full"
                  disabled={loadingOlder}
                  onClick={() => void loadOlder(folder)}
                >
                  {loadingOlder && <Icon name="spinner" className="animate-spin" />}
                  {t("sync.loadOlder")}
                </Button>
              </div>
            )
          )
        }
      />
    );
  }

  return (
    <section aria-label={title} className={cn("flex min-h-0 flex-col", className)}>
      <header className="flex h-12 shrink-0 items-center gap-2 border-b pr-2 pl-4">
        <h2 className="truncate font-heading font-semibold tracking-tight">{title}</h2>
        <span
          aria-live="polite"
          className="ml-auto flex shrink-0 items-center gap-1.5 text-xs text-muted-foreground tabular-nums"
        >
          {searching ? (
            !isPending && t("search.results", { count: messages.length })
          ) : progress ? (
            <>
              <Icon name="spinner" className="size-3.5 animate-spin" />
              {progressLabel(progress)}
            </>
          ) : (
            unreadCount > 0 && t("list.unread", { count: unreadCount })
          )}
        </span>
        {!progress && !searching && (
          <IconButton label={t("sync.refresh")} icon={<Icon name="refresh" />} onClick={refresh} />
        )}
      </header>
      {errorEntry && (
        <div
          role="alert"
          className="flex items-start gap-2 border-b bg-destructive/10 px-4 py-2 text-xs text-destructive"
        >
          <Icon name="alert" className="mt-px size-3.5 shrink-0" />
          <div className="grid min-w-0 flex-1 gap-1">
            <p>
              {accounts.length > 1 && (
                <span className="font-semibold">
                  {accounts.find((a) => a.id === errorEntry[0])?.displayName}:{" "}
                </span>
              )}
              <span className="font-medium">{errorEntry[1]?.message}</span>
              {errorEntry[1]?.action && (
                <span className="text-foreground/80"> {errorEntry[1].action}</span>
              )}
            </p>
            <div className="flex flex-wrap gap-x-3">
              {isSignInError(errorEntry[1]) && (
                <button
                  type="button"
                  className="font-medium underline-offset-2 hover:underline"
                  onClick={() => openSettings({ section: "accounts", accountId: errorEntry[0] })}
                >
                  {t("settings.account.open")}
                </button>
              )}
              <button
                type="button"
                className="font-medium underline-offset-2 hover:underline"
                onClick={() => void syncAccount(errorEntry[0])}
              >
                {t("sync.retry")}
              </button>
            </div>
          </div>
        </div>
      )}
      <ListFilters />
      {content}
    </section>
  );
}
