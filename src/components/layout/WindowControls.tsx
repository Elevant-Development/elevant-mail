import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";

import { Icon, type IconName } from "@/components/common/Icon";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";

function ControlButton({
  icon,
  label,
  onClick,
  danger = false,
}: {
  icon: IconName;
  label: string;
  onClick: () => void;
  danger?: boolean;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className={cn(
        "flex size-8 items-center justify-center rounded-md text-muted-foreground transition-colors [&_svg]:size-4",
        danger
          ? "hover:bg-destructive hover:text-white"
          : "hover:bg-accent hover:text-accent-foreground",
      )}
    >
      <Icon name={icon} />
    </button>
  );
}

/**
 * Minimise, maximise and close for the frameless window (the title bar replaces the system's).
 * Only rendered inside Tauri.
 */
export function WindowControls() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const appWindow = getCurrentWindow();
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    const sync = () =>
      void appWindow.isMaximized().then((value) => {
        if (!cancelled) setMaximized(value);
      });
    sync();
    void appWindow.onResized(sync).then((stop) => {
      if (cancelled) stop();
      else unlisten = stop;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const appWindow = getCurrentWindow();
  return (
    <div className="ml-1 flex items-center gap-0.5">
      <ControlButton
        icon="minimize"
        label={t("window.minimize")}
        onClick={() => void appWindow.minimize()}
      />
      <ControlButton
        icon={maximized ? "restore" : "maximize"}
        label={maximized ? t("window.restore") : t("window.maximize")}
        onClick={() => void appWindow.toggleMaximize()}
      />
      <ControlButton
        icon="close"
        label={t("window.close")}
        onClick={() => void appWindow.close()}
        danger
      />
    </div>
  );
}
