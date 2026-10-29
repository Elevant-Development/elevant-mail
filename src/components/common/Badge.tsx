import { cn } from "@/lib/utils";

interface BadgeProps {
  count: number;
  className?: string;
}

/** Small unread counter. Renders nothing for zero. */
export function Badge({ count, className }: BadgeProps) {
  if (count <= 0) return null;
  return (
    <span
      className={cn(
        "min-w-5 rounded-full px-1.5 text-center text-xs leading-5 font-medium text-muted-foreground tabular-nums",
        className,
      )}
    >
      {count > 999 ? "999+" : count}
    </span>
  );
}
