import { accountColors } from "@/lib/account-colors";
import { initials } from "@/lib/contacts";
import { cn } from "@/lib/utils";

/** A round picture with the contact's initials, in a colour that stays with the name. */
export function ContactAvatar({ name, className }: { name: string; className?: string }) {
  let hash = 0;
  for (const character of name) hash = (hash * 31 + character.charCodeAt(0)) >>> 0;
  const color = accountColors[hash % accountColors.length]?.value;
  return (
    <span
      aria-hidden
      className={cn(
        "flex size-9 shrink-0 items-center justify-center rounded-full text-sm font-semibold text-black/70 select-none",
        className,
      )}
      style={{ backgroundColor: color }}
    >
      {initials(name)}
    </span>
  );
}
