import { Icon } from "@/components/common/Icon";
import { IconButton } from "@/components/common/IconButton";
import { Button } from "@/components/ui/button";
import { useNow } from "@/hooks/use-now";
import { useSyncActions } from "@/hooks/use-sync";
import { mailboxTitle } from "@/lib/folders";
import { t } from "@/lib/i18n";
import { type SyncProgress, UNIFIED_INBOX } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useAccounts } from "@/queries/accounts";
import { useFolders } from "@/queries/folders";
import { useMessages } from "@/queries/messages";
import { useSyncStore } from "@/stores/sync";
import { useUiStore } from "@/stores/ui";

import { EmptyState } from "./EmptyState";
import { MessageListItem } from "./MessageListItem";

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

/** Plain list for now; it becomes virtualized together with the real cache. */
export function MessageList({ className }: MessageListProps) {
  const mailboxId = useUiStore((s) => s.selectedMailboxId);
  const selectedMessageId = useUiStore((s) => s.selectedMessageId);
  const selectMessage = useUiStore((s) => s.selectMessage);
  const compact = useUiStore((s) => s.density === "compact");
  const { data: accounts = [] } = useAccounts();
  const { data: folders = [] } = useFolders();
  const { data: messages = [], isPending, isError } = useMessages(mailboxId);
  const now = useNow();
  const { syncAccount, syncFolder, loadOlder } = useSyncActions();

  const folder = folders.find((f) => f.id === mailboxId);
  // The unified inbox spans all accounts, a folder belongs to one.
  const accountIds = folder ? [folder.accountId] : accounts.map((a) => a.id);
  const progressById = useSyncStore((s) => s.progress);
  const errorsById = useSyncStore((s) => s.errors);
  const loadingOlder = useSyncStore((s) => (folder ? (s.loadingOlder[folder.id] ?? false) : false));
  const progress = accountIds.map((id) => progressById[id]).find(Boolean);
  const errorEntry = accountIds
    .map((id) => [id, errorsById[id]] as const)
    .find(([, error]) => error !== undefined);

  const title = mailboxTitle(mailboxId, folders, accounts);
  const unreadCount = messages.filter((m) => !m.isRead).length;
  const accountColors = new Map(accounts.map((a) => [a.id, a.color]));
  const showAccountColor = mailboxId === UNIFIED_INBOX && accounts.length > 1;
  const hasOlder = folder !== undefined && folder.totalCount > messages.length;

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
      <ul
        className={cn(
          "flex min-h-0 flex-1 flex-col overflow-y-auto p-2",
          compact ? "gap-1" : "gap-1.5",
        )}
      >
        {messages.map((message) => (
          <li key={message.id}>
            <MessageListItem
              message={message}
              selected={message.id === selectedMessageId}
              now={now}
              accountColor={showAccountColor ? accountColors.get(message.accountId) : undefined}
              onSelect={() => selectMessage(message.id)}
            />
          </li>
        ))}
        {hasOlder && (
          <li className="py-2">
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
          </li>
        )}
      </ul>
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
          {progress ? (
            <>
              <Icon name="spinner" className="size-3.5 animate-spin" />
              {progressLabel(progress)}
            </>
          ) : (
            unreadCount > 0 && t("list.unread", { count: unreadCount })
          )}
        </span>
        {!progress && (
          <IconButton label={t("sync.refresh")} icon={<Icon name="refresh" />} onClick={refresh} />
        )}
      </header>
      {errorEntry && (
        <div
          role="alert"
          className="flex items-start gap-2 border-b bg-destructive/10 px-4 py-2 text-xs text-destructive"
        >
          <Icon name="alert" className="mt-px size-3.5 shrink-0" />
          <p className="min-w-0 flex-1">
            <span className="font-medium">{errorEntry[1]?.message}</span>
            {errorEntry[1]?.action && (
              <span className="text-foreground/80"> {errorEntry[1].action}</span>
            )}
          </p>
          <button
            type="button"
            className="shrink-0 font-medium underline-offset-2 hover:underline"
            onClick={() => void syncAccount(errorEntry[0])}
          >
            {t("sync.retry")}
          </button>
        </div>
      )}
      {content}
    </section>
  );
}
