import { Inbox } from "lucide-react";

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

/** Plain list for now; it becomes virtualized together with the real cache. */
export function MessageList({ className }: MessageListProps) {
  const mailboxId = useUiStore((s) => s.selectedMailboxId);
  const selectedMessageId = useUiStore((s) => s.selectedMessageId);
  const selectMessage = useUiStore((s) => s.selectMessage);
  const { data: accounts = [] } = useAccounts();
  const { data: folders = [] } = useFolders();
  const { data: messages = [], isPending } = useMessages(mailboxId);
  const now = useNow();

  const title = mailboxTitle(mailboxId, folders, accounts);
  const unreadCount = messages.filter((m) => !m.isRead).length;
  const accountColors = new Map(accounts.map((a) => [a.id, a.color]));
  const showAccountColor = mailboxId === UNIFIED_INBOX;

  return (
    <section aria-label={title} className={cn("flex min-h-0 flex-col", className)}>
      <header className="flex h-12 shrink-0 items-center gap-2 border-b px-4">
        <h2 className="truncate font-semibold">{title}</h2>
        {unreadCount > 0 && (
          <span className="ml-auto shrink-0 text-xs text-muted-foreground">
            {t("list.unread", { count: unreadCount })}
          </span>
        )}
      </header>
      {!isPending && messages.length === 0 ? (
        <EmptyState icon={<Inbox />} title={t("list.empty")} className="flex-1" />
      ) : (
        <ul className="min-h-0 flex-1 overflow-y-auto">
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
      )}
    </section>
  );
}
