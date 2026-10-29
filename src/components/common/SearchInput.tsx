import { Icon } from "@/components/common/Icon";
import { Input } from "@/components/ui/input";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";

interface SearchInputProps {
  className?: string;
}

/** Search field. Not wired up yet: search over the local cache comes with the backend. */
export function SearchInput({ className }: SearchInputProps) {
  return (
    <div className={cn("relative", className)}>
      <Icon
        name="search"
        className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground"
      />
      <Input
        type="search"
        placeholder={t("search.placeholder")}
        aria-label={t("search.placeholder")}
        className="h-8 pr-8 pl-8"
        disabled
      />
      <kbd className="pointer-events-none absolute top-1/2 right-2 -translate-y-1/2 rounded border px-1.5 font-sans text-[11px] text-muted-foreground">
        /
      </kbd>
    </div>
  );
}
