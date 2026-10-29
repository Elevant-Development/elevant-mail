import { hueFromString, initials } from "@/lib/formatting";
import type { Address } from "@/lib/tauri";
import { cn } from "@/lib/utils";

interface AvatarProps {
  address: Address;
  className?: string;
}

/** Initials on a colour derived from the address. No external avatar services. */
export function Avatar({ address, className }: AvatarProps) {
  const hue = hueFromString(address.address.toLowerCase());
  return (
    <span
      aria-hidden
      className={cn(
        "flex size-9 shrink-0 items-center justify-center rounded-full text-xs font-semibold text-white select-none",
        className,
      )}
      style={{ backgroundColor: `oklch(0.56 0.11 ${hue})` }}
    >
      {initials(address)}
    </span>
  );
}
