import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

interface EmptyStateProps {
  icon: ReactNode;
  title: string;
  hint?: string;
  className?: string;
  /** Below the hint, e.g. a button that does something about it. */
  children?: ReactNode;
}

export function EmptyState({ icon, title, hint, className, children }: EmptyStateProps) {
  return (
    <div
      className={cn(
        "flex flex-col items-center justify-center gap-2 p-8 text-center select-none",
        className,
      )}
    >
      <div className="mb-1 text-muted-foreground/50 [&_svg]:size-10 [&_svg]:stroke-[1.25]">
        {icon}
      </div>
      <p className="font-medium text-foreground/80">{title}</p>
      {hint && <p className="max-w-xs text-muted-foreground">{hint}</p>}
      {children}
    </div>
  );
}
