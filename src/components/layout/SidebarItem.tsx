import { Badge } from "@/components/common/Badge";
import { Icon, type IconName } from "@/components/common/Icon";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";

interface SidebarItemProps {
  icon: IconName;
  label: string;
  /** Muted after the label, e.g. the server's name of a special folder. */
  detail?: string;
  unreadCount: number;
  selected: boolean;
  /** Without it the row only opens and closes its subfolders (a group without mail). */
  onSelect?: () => void;
  /** Nesting level in the folder tree. */
  depth?: number;
  /** Set when the row has subfolders: whether they show. */
  expanded?: boolean;
  onToggle?: () => void;
}

/** Indent per tree level; the chevron column keeps rows aligned with and without subfolders. */
const INDENT_REM = 0.875;

export function SidebarItem({
  icon,
  label,
  detail,
  unreadCount,
  selected,
  onSelect,
  depth = 0,
  expanded,
  onToggle,
}: SidebarItemProps) {
  const expandable = expanded !== undefined && onToggle !== undefined;
  return (
    <div
      className={cn(
        "group/item flex h-9 items-center rounded-md text-sidebar-foreground transition-colors select-none hover:bg-sidebar-accent/70",
        selected && "bg-sidebar-accent text-sidebar-accent-foreground hover:bg-sidebar-accent",
      )}
      style={{ paddingLeft: `${depth * INDENT_REM}rem` }}
    >
      {expandable ? (
        <button
          type="button"
          aria-expanded={expanded}
          aria-label={t(expanded ? "sidebar.collapse" : "sidebar.expand", { name: label })}
          onClick={onToggle}
          className="flex h-full w-5 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:text-foreground"
        >
          <Icon
            name="chevron"
            className={cn("size-3.5 transition-transform", expanded && "rotate-90")}
          />
        </button>
      ) : (
        <span aria-hidden className="w-5 shrink-0" />
      )}
      <button
        type="button"
        onClick={onSelect ?? onToggle}
        aria-current={selected ? "page" : undefined}
        aria-expanded={onSelect ? undefined : expanded}
        className={cn(
          "flex h-full min-w-0 flex-1 items-center gap-2.5 rounded-md pr-2 text-left",
          "[&_svg]:size-4 [&_svg]:shrink-0 [&_svg]:text-muted-foreground",
          selected && "[&_svg]:text-sidebar-primary",
          unreadCount > 0 && "font-medium",
        )}
      >
        <Icon name={icon} />
        <span className="min-w-0 flex-1 truncate">
          {label}
          {detail && <span className="font-normal text-muted-foreground"> · {detail}</span>}
        </span>
        <Badge count={unreadCount} className={cn(selected && "text-sidebar-accent-foreground")} />
      </button>
    </div>
  );
}
