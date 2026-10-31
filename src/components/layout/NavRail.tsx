import { useEffect } from "react";

import { Icon, type IconName } from "@/components/common/Icon";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { unifiedInboxes } from "@/lib/folders";
import { type MessageKey, t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { useAccounts } from "@/queries/accounts";
import { useFolders } from "@/queries/folders";
import { type Area, useCalendarStore } from "@/stores/calendar";

/** The areas of the app, in order; Ctrl+1, Ctrl+2 … open them. */
const areas: { value: Area; icon: IconName; label: MessageKey }[] = [
  { value: "mail", icon: "mail", label: "area.mail" },
  { value: "calendar", icon: "calendar", label: "area.calendar" },
];

/** A narrow column of icons at the left edge: mail, calendar, and the areas still to come. */
export function NavRail({ className }: { className?: string }) {
  const area = useCalendarStore((s) => s.area);
  const setArea = useCalendarStore((s) => s.setArea);
  const { data: accounts = [] } = useAccounts();
  const { data: folders = [] } = useFolders();
  const unread = unifiedInboxes(folders, accounts).reduce((sum, f) => sum + f.unreadCount, 0);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.altKey || event.shiftKey) return;
      const chosen = areas[Number(event.key) - 1];
      if (!chosen) return;
      event.preventDefault();
      setArea(chosen.value);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [setArea]);

  return (
    <nav
      aria-label={t("area.label")}
      className={cn("flex flex-col items-center gap-1 pt-2", className)}
    >
      {areas.map((option, index) => {
        const selected = area === option.value;
        return (
          <Tooltip key={option.value}>
            <TooltipTrigger asChild>
              <button
                type="button"
                aria-label={t(option.label)}
                aria-current={selected ? "page" : undefined}
                onClick={() => setArea(option.value)}
                className={cn(
                  "relative flex size-10 items-center justify-center rounded-xl text-muted-foreground transition-colors hover:bg-sidebar-accent/70 hover:text-foreground [&_svg]:size-5",
                  selected && "bg-sidebar-accent text-foreground hover:bg-sidebar-accent",
                )}
              >
                {selected && (
                  <span
                    aria-hidden
                    className="absolute -left-2 h-5 w-1 rounded-r-full bg-primary"
                  />
                )}
                <Icon name={option.icon} />
                {option.value === "mail" && unread > 0 && (
                  <span className="absolute top-1 right-1 min-w-4 rounded-full bg-primary px-1 text-center text-[10px] leading-4 font-semibold text-primary-foreground tabular-nums">
                    {unread > 99 ? "99+" : unread}
                  </span>
                )}
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">
              {t(option.label)} <kbd className="ml-1 opacity-70">Ctrl+{index + 1}</kbd>
            </TooltipContent>
          </Tooltip>
        );
      })}
    </nav>
  );
}
