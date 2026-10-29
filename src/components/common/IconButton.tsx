import type { ComponentProps, ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

interface IconButtonProps extends Omit<ComponentProps<typeof Button>, "size" | "children"> {
  /** Shown as tooltip and used as accessible name. */
  label: string;
  icon: ReactNode;
}

export function IconButton({ label, icon, variant = "ghost", ...props }: IconButtonProps) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant={variant} size="icon-sm" aria-label={label} {...props}>
          {icon}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}
