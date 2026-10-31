import { isTauri } from "@tauri-apps/api/core";

import { ElevantMark } from "@/components/common/ElevantMark";
import { Icon } from "@/components/common/Icon";
import { IconButton } from "@/components/common/IconButton";
import { SearchInput } from "@/components/common/SearchInput";
import { UpdateNotice } from "@/components/updates/UpdateNotice";
import { t } from "@/lib/i18n";
import { usesMockData } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useCalendarStore } from "@/stores/calendar";
import { useUiStore } from "@/stores/ui";

import { AppearanceMenu } from "./AppearanceMenu";
import { listWidth, navWidth } from "./columns";
import { WindowControls } from "./WindowControls";

interface TitleBarProps {
  /** Wide layout shows the app name above the sidebar, narrow layout a sidebar toggle. */
  isWide: boolean;
}

export function TitleBar({ isWide }: TitleBarProps) {
  const sidebarOpen = useUiStore((s) => s.sidebarOpen);
  const setSidebarOpen = useUiStore((s) => s.setSidebarOpen);
  const openSettings = useUiStore((s) => s.openSettings);
  // Searching is for mail; the calendar keeps the room empty.
  const mail = useCalendarStore((s) => s.area === "mail");

  return (
    // px-2 and the rail and sidebar columns mirror AppShell: the mark sits above the rail's icons
    // (8px outer, 12px into the 48px rail), the search field where the list starts.
    // The bar is also the window's title bar: empty areas drag the window, double-click maximises.
    <header data-tauri-drag-region className="flex h-12 shrink-0 items-center gap-2 bg-canvas px-2">
      {isWide ? (
        <div
          data-tauri-drag-region
          className={cn(
            "flex shrink-0 items-center gap-2.5 px-3 select-none *:pointer-events-none",
            navWidth,
          )}
        >
          <ElevantMark className="size-6 text-primary" />
          <span className="text-[15px] font-semibold tracking-tight">Elevant Mail</span>
        </div>
      ) : (
        <IconButton
          label={t("titlebar.toggleSidebar")}
          icon={<Icon name="sidebar" />}
          aria-expanded={sidebarOpen}
          onClick={() => setSidebarOpen(!sidebarOpen)}
        />
      )}
      {mail ? (
        <SearchInput className={isWide ? cn("shrink-0", listWidth) : "min-w-0 flex-1"} />
      ) : (
        <div data-tauri-drag-region className={isWide ? cn("shrink-0", listWidth) : "flex-1"} />
      )}
      <div data-tauri-drag-region className="ml-auto flex items-center gap-1">
        {usesMockData && (
          <span
            title={t("app.mockDataHint")}
            className="mr-1 rounded-md border px-2 py-0.5 text-xs whitespace-nowrap text-muted-foreground select-none"
          >
            {t("app.mockData")}
          </span>
        )}
        <UpdateNotice />
        <AppearanceMenu />
        <IconButton
          label={t("titlebar.settings")}
          icon={<Icon name="settings" />}
          onClick={() => openSettings({ section: "accounts", accountId: null })}
        />
        {isTauri() && <WindowControls />}
      </div>
    </header>
  );
}
