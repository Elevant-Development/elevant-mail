import { Moon, PanelLeft, Settings, Sun } from "lucide-react";

import { IconButton } from "@/components/common/IconButton";
import { SearchInput } from "@/components/common/SearchInput";
import { t } from "@/lib/i18n";
import { usesMockData } from "@/lib/tauri";
import { useUiStore } from "@/stores/ui";

interface TitleBarProps {
  /** Wide layout shows the app name above the sidebar, narrow layout a sidebar toggle. */
  isWide: boolean;
}

export function TitleBar({ isWide }: TitleBarProps) {
  const theme = useUiStore((s) => s.theme);
  const toggleTheme = useUiStore((s) => s.toggleTheme);
  const sidebarOpen = useUiStore((s) => s.sidebarOpen);
  const setSidebarOpen = useUiStore((s) => s.setSidebarOpen);

  return (
    <header className="flex h-12 shrink-0 items-center gap-2 border-b bg-sidebar px-3">
      {isWide ? (
        <div className="w-58 shrink-0 px-1 font-semibold tracking-tight select-none">
          Elevant Mail
        </div>
      ) : (
        <IconButton
          label={t("titlebar.toggleSidebar")}
          icon={<PanelLeft />}
          aria-expanded={sidebarOpen}
          onClick={() => setSidebarOpen(!sidebarOpen)}
        />
      )}
      <SearchInput className="w-full max-w-sm" />
      <div className="ml-auto flex items-center gap-1">
        {usesMockData && (
          <span
            title={t("app.mockDataHint")}
            className="mr-1 rounded-full border px-2 py-0.5 text-xs whitespace-nowrap text-muted-foreground select-none"
          >
            {t("app.mockData")}
          </span>
        )}
        <IconButton
          label={theme === "dark" ? t("titlebar.themeLight") : t("titlebar.themeDark")}
          icon={theme === "dark" ? <Sun /> : <Moon />}
          onClick={toggleTheme}
        />
        <IconButton label={t("titlebar.settings")} icon={<Settings />} disabled />
      </div>
    </header>
  );
}
