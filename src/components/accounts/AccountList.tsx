import { Icon, type IconName } from "@/components/common/Icon";
import { SidebarItem } from "@/components/layout/SidebarItem";
import { buildFolderTree, type FolderNode, shownUnread, unreadBelow } from "@/lib/folders";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { useSyncStore } from "@/stores/sync";
import { useUiStore } from "@/stores/ui";
import type { Account, Folder, FolderRole } from "@/lib/tauri";

const roleIcons: Record<FolderRole, IconName> = {
  inbox: "inbox",
  drafts: "drafts",
  sent: "sent",
  archive: "archive",
  junk: "junk",
  trash: "trash",
  all: "archive",
};

interface AccountListProps {
  accounts: Account[];
  folders: Folder[];
  selectedMailboxId: string;
  onSelectMailbox: (mailboxId: string) => void;
  onRemoveAccount: (account: Account) => void;
}

interface FolderTreeProps {
  nodes: FolderNode[];
  depth: number;
  selectedMailboxId: string;
  onSelectMailbox: (mailboxId: string) => void;
}

/**
 * Folders and their subfolders. Groups without mail of their own ("[Gmail]") start collapsed,
 * everything else open; what the user changes is remembered.
 */
function FolderTree({ nodes, depth, selectedMailboxId, onSelectMailbox }: FolderTreeProps) {
  const collapsed = useUiStore((s) => s.collapsed);
  const setCollapsed = useUiStore((s) => s.setCollapsed);

  return (
    <ul className="flex flex-col gap-px">
      {nodes.map((node) => {
        const hasChildren = node.children.length > 0;
        const isCollapsed = hasChildren && (collapsed[node.key] ?? node.folder === null);
        const folder = node.folder;
        // A collapsed folder also counts what is unread inside it.
        const unread = shownUnread(folder) + (isCollapsed ? unreadBelow(node) : 0);
        return (
          <li key={node.key}>
            <SidebarItem
              icon={folder?.role ? roleIcons[folder.role] : "folder"}
              label={node.label}
              detail={node.detail}
              unreadCount={unread}
              selected={folder?.id === selectedMailboxId}
              onSelect={folder ? () => onSelectMailbox(folder.id) : undefined}
              depth={depth}
              expanded={hasChildren ? !isCollapsed : undefined}
              onToggle={hasChildren ? () => setCollapsed(node.key, !isCollapsed) : undefined}
            />
            {hasChildren && !isCollapsed && (
              <FolderTree
                nodes={node.children}
                depth={depth + 1}
                selectedMailboxId={selectedMailboxId}
                onSelectMailbox={onSelectMailbox}
              />
            )}
          </li>
        );
      })}
    </ul>
  );
}

/** One collapsible section per account with its folder tree. */
export function AccountList({
  accounts,
  folders,
  selectedMailboxId,
  onSelectMailbox,
  onRemoveAccount,
}: AccountListProps) {
  const progress = useSyncStore((s) => s.progress);
  const errors = useSyncStore((s) => s.errors);
  const collapsed = useUiStore((s) => s.collapsed);
  const setCollapsed = useUiStore((s) => s.setCollapsed);

  return (
    <div className="flex flex-col gap-4">
      {accounts.map((account) => {
        const sectionKey = `account:${account.id}`;
        const sectionCollapsed = collapsed[sectionKey] ?? false;
        const accountFolders = folders.filter((f) => f.accountId === account.id);
        // Collapsed, the account shows what is unread in its inbox.
        const inboxUnread = accountFolders.find((f) => f.role === "inbox")?.unreadCount ?? 0;
        return (
          <section key={account.id} aria-labelledby={`account-${account.id}`}>
            <div className="group/account mb-1 flex h-6 items-center gap-1">
              <button
                type="button"
                aria-expanded={!sectionCollapsed}
                aria-controls={`folders-${account.id}`}
                onClick={() => setCollapsed(sectionKey, !sectionCollapsed)}
                className="flex h-full min-w-0 flex-1 items-center gap-2 rounded-md pr-1 text-left"
              >
                <Icon
                  name="chevron"
                  className={cn(
                    "ml-[3px] size-3.5 shrink-0 text-muted-foreground transition-transform",
                    !sectionCollapsed && "rotate-90",
                  )}
                />
                <h3
                  id={`account-${account.id}`}
                  title={account.emailAddress}
                  className="flex min-w-0 flex-1 items-center gap-2 text-xs font-medium text-muted-foreground select-none"
                >
                  <span
                    aria-hidden
                    className="size-2 shrink-0 rounded-full"
                    style={{ backgroundColor: account.color }}
                  />
                  <span className="truncate">{account.displayName}</span>
                  {progress[account.id] && (
                    <Icon name="spinner" className="size-3 shrink-0 animate-spin" />
                  )}
                  {!progress[account.id] && errors[account.id] && (
                    <span
                      title={[
                        t("sync.failed"),
                        errors[account.id]?.message,
                        errors[account.id]?.action,
                      ]
                        .filter(Boolean)
                        .join(" · ")}
                      className="flex text-destructive"
                    >
                      <Icon name="alert" className="size-3 shrink-0" />
                    </span>
                  )}
                  {sectionCollapsed && inboxUnread > 0 && (
                    <span className="ml-auto text-[11px] tabular-nums">{inboxUnread}</span>
                  )}
                </h3>
              </button>
              <button
                type="button"
                aria-label={t("account.remove.action", { name: account.displayName })}
                title={t("account.remove.action", { name: account.displayName })}
                onClick={() => onRemoveAccount(account)}
                className="flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground opacity-0 transition-opacity group-hover/account:opacity-100 hover:bg-sidebar-accent hover:text-foreground focus-visible:opacity-100 [&_svg]:size-3.5"
              >
                <Icon name="trash" />
              </button>
            </div>
            {!sectionCollapsed && (
              <div id={`folders-${account.id}`}>
                <FolderTree
                  nodes={buildFolderTree(accountFolders)}
                  depth={0}
                  selectedMailboxId={selectedMailboxId}
                  onSelectMailbox={onSelectMailbox}
                />
              </div>
            )}
          </section>
        );
      })}
    </div>
  );
}
