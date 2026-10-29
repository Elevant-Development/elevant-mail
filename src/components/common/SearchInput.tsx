import { Icon } from "@/components/common/Icon";
import { Input } from "@/components/ui/input";
import { t } from "@/lib/i18n";
import { SEARCH_ATTRIBUTE } from "@/lib/keyboard";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/stores/ui";

interface SearchInputProps {
  className?: string;
}

/** Searches all cached mail; the list shows the results while there is text. `/` focuses it. */
export function SearchInput({ className }: SearchInputProps) {
  const query = useUiStore((s) => s.searchQuery);
  const setQuery = useUiStore((s) => s.setSearchQuery);

  const onKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Escape") {
      // First Escape clears, the second leaves the field.
      if (query) setQuery("");
      else event.currentTarget.blur();
      event.preventDefault();
    } else if (event.key === "Enter" || event.key === "ArrowDown") {
      // On to the results: the first one opens and the list keeps the focus for j/k.
      const first = document.querySelector<HTMLElement>("[data-message-list] [data-message-id]");
      if (!first) return;
      first.click();
      first.focus();
      event.preventDefault();
    }
  };

  return (
    <div className={cn("relative", className)}>
      <Icon
        name="search"
        className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground"
      />
      <Input
        type="search"
        {...{ [SEARCH_ATTRIBUTE]: "" }}
        value={query}
        onChange={(event) => setQuery(event.target.value)}
        onKeyDown={onKeyDown}
        placeholder={t("search.placeholder")}
        aria-label={t("search.placeholder")}
        className="h-8 pr-8 pl-8 [&::-webkit-search-cancel-button]:hidden"
      />
      {query ? (
        <button
          type="button"
          aria-label={t("search.clear")}
          onClick={() => setQuery("")}
          className="absolute top-1/2 right-1.5 flex size-5 -translate-y-1/2 items-center justify-center rounded text-muted-foreground hover:text-foreground"
        >
          <Icon name="close" className="size-3.5" />
        </button>
      ) : (
        <kbd className="pointer-events-none absolute top-1/2 right-2 -translate-y-1/2 rounded border px-1.5 font-sans text-[11px] text-muted-foreground">
          /
        </kbd>
      )}
    </div>
  );
}
