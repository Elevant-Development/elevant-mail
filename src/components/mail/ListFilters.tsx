import { Icon, type IconName } from "@/components/common/Icon";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { type ListFilter, useUiStore } from "@/stores/ui";

const filters: { filter: ListFilter; icon: IconName }[] = [
  { filter: "unread", icon: "mail" },
  { filter: "flagged", icon: "star" },
  { filter: "attachment", icon: "paperclip" },
];

/** Narrows the list to unread, flagged or attached mail; several together narrow it further. */
export function ListFilters() {
  const active = useUiStore((s) => s.listFilters);
  const toggle = useUiStore((s) => s.toggleListFilter);

  return (
    <div
      role="toolbar"
      aria-label={t("filter.label")}
      className="flex shrink-0 items-center gap-1.5 border-b px-3 py-2"
    >
      {filters.map(({ filter, icon }) => {
        const on = active.includes(filter);
        return (
          <button
            key={filter}
            type="button"
            aria-pressed={on}
            onClick={() => toggle(filter)}
            className={cn(
              "flex h-6 items-center gap-1 rounded-full border px-2.5 text-xs font-medium transition-colors [&_svg]:size-3",
              on
                ? "border-primary/40 bg-primary/15 text-foreground [&_svg]:text-primary"
                : "border-transparent bg-muted/60 text-muted-foreground hover:text-foreground",
            )}
          >
            <Icon name={icon} filled={on && filter === "flagged"} />
            {t(`filter.${filter}`)}
          </button>
        );
      })}
    </div>
  );
}
