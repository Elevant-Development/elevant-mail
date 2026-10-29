import { hueFromString, initials } from "@/lib/formatting";
import type { Address } from "@/lib/tauri";
import { cn } from "@/lib/utils";

interface AvatarProps {
  address: Address;
  /** Small dot on the corner, used for unread mail. */
  badge?: boolean;
  className?: string;
}

/**
 * Initials on a tonal squircle: the hue comes from the address, lightness and chroma from the
 * active theme (CSS variables). No external avatar services.
 */
export function Avatar({ address, badge = false, className }: AvatarProps) {
  const hue = hueFromString(address.address.toLowerCase());
  return (
    <span
      aria-hidden
      className={cn(
        "relative flex size-9 shrink-0 items-center justify-center rounded-[32%] text-xs font-semibold select-none",
        className,
      )}
      style={{
        backgroundColor: `oklch(var(--avatar-bg-l) var(--avatar-bg-c) ${hue})`,
        color: `oklch(var(--avatar-fg-l) var(--avatar-fg-c) ${hue})`,
      }}
    >
      {initials(address)}
      {badge && (
        <span className="absolute -top-0.5 -left-0.5 size-3 rounded-full border-2 border-card bg-unread" />
      )}
    </span>
  );
}
