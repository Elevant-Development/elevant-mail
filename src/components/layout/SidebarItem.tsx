import { Badge } from "@/components/common/Badge";
import { Icon, type IconName } from "@/components/common/Icon";
import { cn } from "@/lib/utils";

interface SidebarItemProps {
  icon: IconName;
  label: string;
  unreadCount: number;
  selected: boolean;
  onSelect: () => void;
}

export function SidebarItem({ icon, label, unreadCount, selected, onSelect }: SidebarItemProps) {
  return (
    <button
      type="button"
      onClick={onSelect}
      aria-current={selected ? "page" : undefined}
      className={cn(
        "flex h-9 w-full items-center gap-2.5 rounded-md px-2 text-left select-none",
        "text-sidebar-foreground transition-colors hover:bg-sidebar-accent/70",
        "[&_svg]:size-4 [&_svg]:shrink-0 [&_svg]:text-muted-foreground",
        selected &&
          "bg-sidebar-accent text-sidebar-accent-foreground hover:bg-sidebar-accent [&_svg]:text-sidebar-primary",
        unreadCount > 0 && "font-medium",
      )}
    >
      <Icon name={icon} />
      <span className="min-w-0 flex-1 truncate">{label}</span>
      <Badge count={unreadCount} className={cn(selected && "text-sidebar-accent-foreground")} />
    </button>
  );
}
