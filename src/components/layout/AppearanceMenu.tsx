import { useState } from "react";

import { Icon } from "@/components/common/Icon";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { t } from "@/lib/i18n";

import { AppearanceSettings } from "./AppearanceSettings";

/** The appearance at hand in the title bar; the settings window shows the same controls. */
export function AppearanceMenu() {
  const [open, setOpen] = useState(false);

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <Tooltip>
        <TooltipTrigger asChild>
          <PopoverTrigger asChild>
            <Button variant="ghost" size="icon-sm" aria-label={t("appearance.open")}>
              <Icon name="palette" />
            </Button>
          </PopoverTrigger>
        </TooltipTrigger>
        <TooltipContent>{t("appearance.open")}</TooltipContent>
      </Tooltip>
      <PopoverContent
        align="end"
        className="max-h-(--radix-popover-content-available-height) w-80 overflow-y-auto p-3"
      >
        <AppearanceSettings />
      </PopoverContent>
    </Popover>
  );
}
