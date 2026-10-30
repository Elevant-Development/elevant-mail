import { useQuery } from "@tanstack/react-query";
import { type ClipboardEvent, type KeyboardEvent, type ReactNode, useId, useState } from "react";

import { Icon } from "@/components/common/Icon";
import { useDebouncedValue } from "@/hooks/use-debounced-value";
import { looksLikeAddress, parseAddresses } from "@/lib/compose";
import { displayName } from "@/lib/formatting";
import { t } from "@/lib/i18n";
import { type Address, api } from "@/lib/tauri";
import { cn } from "@/lib/utils";

interface AddressFieldProps {
  label: string;
  addresses: Address[];
  onChange: (addresses: Address[]) => void;
  autoFocus?: boolean;
  /** Next to the input, e.g. the switch for Cc and Bcc. */
  extra?: ReactNode;
}

/**
 * Recipients as chips. Typing offers addresses from the cached mail (↑/↓, Enter or Tab takes
 * one); a comma, semicolon, Enter or leaving the field turns what was typed into chips,
 * Backspace on an empty field takes the last one back. Pasted lists are split.
 */
export function AddressField({ label, addresses, onChange, autoFocus, extra }: AddressFieldProps) {
  const id = useId();
  const [text, setText] = useState("");
  const [active, setActive] = useState(0);
  const query = useDebouncedValue(text.trim(), 120);
  const { data: found = [] } = useQuery({
    queryKey: ["addressSuggestions", query],
    queryFn: () => api.suggestAddresses(query),
    enabled: query.length > 0,
    staleTime: 30_000,
  });
  const chosen = new Set(addresses.map((a) => a.address.toLowerCase()));
  const suggestions = text.trim() ? found.filter((a) => !chosen.has(a.address.toLowerCase())) : [];
  const current = Math.min(active, Math.max(suggestions.length - 1, 0));
  const open = suggestions.length > 0;

  const add = (more: Address[]) => {
    const fresh = more.filter((a) => a.address && !chosen.has(a.address.toLowerCase()));
    if (fresh.length > 0) onChange([...addresses, ...fresh]);
    setText("");
    setActive(0);
  };
  const commitText = () => {
    if (text.trim()) add(parseAddresses(text));
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (open && (event.key === "ArrowDown" || event.key === "ArrowUp")) {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      setActive(Math.min(Math.max(current + step, 0), suggestions.length - 1));
    } else if ((event.key === "Enter" || event.key === "Tab") && !event.ctrlKey && text.trim()) {
      const suggestion = suggestions[current];
      if (event.key === "Enter") event.preventDefault();
      if (suggestion) add([suggestion]);
      else commitText();
    } else if (event.key === "Enter" && event.ctrlKey) {
      // Ctrl+Enter sends; what was typed goes along.
      commitText();
    } else if (event.key === "," || event.key === ";") {
      event.preventDefault();
      commitText();
    } else if (event.key === "Backspace" && text === "" && addresses.length > 0) {
      onChange(addresses.slice(0, -1));
    } else if (event.key === "Escape" && open) {
      event.preventDefault();
      setText("");
    }
  };

  const onPaste = (event: ClipboardEvent<HTMLInputElement>) => {
    const pasted = event.clipboardData.getData("text");
    if (!/[,;\n]/.test(pasted)) return;
    event.preventDefault();
    add(parseAddresses(text + pasted));
  };

  return (
    <div className="relative flex min-h-10 items-start gap-2 px-4 py-1.5">
      <label htmlFor={id} className="w-14 shrink-0 pt-1.5 text-sm text-muted-foreground">
        {label}
      </label>
      <div className="flex min-w-0 flex-1 flex-wrap items-center gap-1">
        {addresses.map((address) => {
          const valid = looksLikeAddress(address.address);
          return (
            <span
              key={address.address}
              title={valid ? address.address : t("compose.invalidAddress")}
              className={cn(
                "flex h-7 max-w-full min-w-0 items-center gap-1 rounded-md border bg-muted/60 pr-1 pl-2 text-sm",
                !valid && "border-destructive/60 text-destructive",
              )}
            >
              <span className="truncate">{displayName(address)}</span>
              <button
                type="button"
                aria-label={t("compose.removeAddress", { address: address.address })}
                onClick={() => onChange(addresses.filter((a) => a !== address))}
                className="flex size-5 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-accent hover:text-foreground [&_svg]:size-3"
              >
                <Icon name="close" />
              </button>
            </span>
          );
        })}
        <input
          id={id}
          role="combobox"
          aria-expanded={open}
          aria-controls={`${id}-suggestions`}
          aria-activedescendant={open ? `${id}-option-${current}` : undefined}
          autoComplete="off"
          spellCheck={false}
          autoFocus={autoFocus}
          value={text}
          onChange={(event) => {
            setText(event.target.value);
            setActive(0);
          }}
          onKeyDown={onKeyDown}
          onPaste={onPaste}
          onBlur={commitText}
          className="h-7 min-w-32 flex-1 bg-transparent text-sm outline-none"
        />
      </div>
      {extra}
      {open && (
        <ul
          id={`${id}-suggestions`}
          role="listbox"
          aria-label={t("compose.suggestions")}
          className="absolute top-full right-4 left-20 z-30 -mt-1 overflow-hidden rounded-lg border bg-popover py-1 text-sm text-popover-foreground shadow-[0_4px_16px_var(--shadow-color)]"
        >
          {suggestions.map((suggestion, index) => (
            <li
              key={suggestion.address}
              id={`${id}-option-${index}`}
              role="option"
              aria-selected={index === current}
              // Before the input's blur, so the click still finds the suggestion.
              onMouseDown={(event) => {
                event.preventDefault();
                add([suggestion]);
              }}
              onMouseEnter={() => setActive(index)}
              className={cn(
                "flex cursor-default items-baseline gap-2 px-3 py-1.5",
                index === current && "bg-accent",
              )}
            >
              <span className="truncate font-medium">{displayName(suggestion)}</span>
              {suggestion.name && (
                <span className="truncate text-xs text-muted-foreground">{suggestion.address}</span>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
