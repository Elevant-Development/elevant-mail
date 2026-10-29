import { Icon } from "@/components/common/Icon";
import { useNow } from "@/hooks/use-now";
import { mailboxTitle } from "@/lib/folders";
import { t } from "@/lib/i18n";
import { UNIFIED_INBOX } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useAccounts } from "@/queries/accounts";
import { useFolders } from "@/queries/folders";
import { useMessages } from "@/queries/messages";
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

/** Plain list for now; it becomes virtualized together with the real cache. */
export function MessageList({ className }: MessageListProps) {
  const mailboxId = useUiStore((s) => s.selectedMailboxId);
  const selectedMessageId = useUiStore((s) => s.selectedMessageId);
  const selectMessage = useUiStore((s) => s.selectMessage);
  const { data: accounts = [] } = useAccounts();
  const { data: folders = [] } = useFolders();
  const { data: messages = [], isPending, isError } = useMessages(mailboxId);
  const now = useNow();

  const title = mailboxTitle(mailboxId, folders, accounts);
  const unreadCount = messages.filter((m) => !m.isRead).length;
  const accountColors = new Map(accounts.map((a) => [a.id, a.color]));
  const showAccountColor = mailboxId === UNIFIED_INBOX;

  let content;
  if (isPending) {
    content = <ListSkeleton />;
  } else if (isError) {
    content = (
      <EmptyState icon={<Icon name="alert" />} title={t("list.error")} className="flex-1" />
    );
  } else if (messages.length === 0) {
    content = (
      <EmptyState icon={<Icon name="inbox" />} title={t("list.empty")} className="flex-1" />
    );
  } else {
    content = (
      <ul className="flex min-h-0 flex-1 flex-col gap-1.5 overflow-y-auto p-2">
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
      </ul>
    );
  }

  return (
    <section aria-label={title} className={cn("flex min-h-0 flex-col", className)}>
      <header className="flex h-12 shrink-0 items-center gap-2 border-b px-4">
        <h2 className="truncate font-heading font-semibold tracking-tight">{title}</h2>
        {unreadCount > 0 && (
          <span className="ml-auto shrink-0 text-xs text-muted-foreground tabular-nums">
            {t("list.unread", { count: unreadCount })}
          </span>
        )}
      </header>
      {content}
    </section>
  );
}
