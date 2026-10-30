import { useEffect, useState } from "react";

import { Icon } from "@/components/common/Icon";
import { Button } from "@/components/ui/button";
import { t } from "@/lib/i18n";
import { api, type Update, usesMockData } from "@/lib/tauri";
import { useUiStore } from "@/stores/ui";

const DAY_MS = 24 * 60 * 60 * 1000;
/** Not right at start, when the accounts sync. */
const FIRST_CHECK_MS = 10_000;

/**
 * While the setting allows it, asks once soon after start and then daily whether a newer version
 * was released. Errors stay quiet: offline is no news.
 */
function useNewerVersion(): Update | null {
  const enabled = useUiStore((s) => s.updateCheck);
  const [update, setUpdate] = useState<Update | null>(null);
  useEffect(() => {
    if (!enabled) return;
    const check = () =>
      void api
        .checkForUpdate()
        .then(setUpdate)
        .catch(() => undefined);
    const first = window.setTimeout(check, usesMockData ? 500 : FIRST_CHECK_MS);
    const daily = window.setInterval(check, DAY_MS);
    return () => {
      window.clearTimeout(first);
      window.clearInterval(daily);
    };
  }, [enabled]);
  // Switched off: an update found before stays hidden too.
  return enabled ? update : null;
}

/** In the title bar: a newer version, with the way to its download page. */
export function UpdateNotice() {
  const update = useNewerVersion();
  const skipped = useUiStore((s) => s.skippedUpdate);
  const skipUpdate = useUiStore((s) => s.skipUpdate);
  if (!update || update.version === skipped) return null;
  return (
    <div className="mr-1 flex items-center rounded-md border border-primary/40 bg-primary/10 text-xs">
      <button
        type="button"
        title={t("update.download")}
        onClick={() => void api.openLink(update.url)}
        className="flex items-center gap-1.5 rounded-l-md px-2 py-0.5 font-medium hover:bg-primary/15 [&_svg]:size-3.5 [&_svg]:text-primary"
      >
        <Icon name="download" />
        {t("update.available", { version: update.version })}
      </button>
      <button
        type="button"
        aria-label={t("update.later")}
        title={t("update.later")}
        onClick={() => skipUpdate(update.version)}
        className="flex h-full items-center rounded-r-md px-1.5 text-muted-foreground hover:bg-primary/15 hover:text-foreground [&_svg]:size-3"
      >
        <Icon name="close" />
      </button>
    </div>
  );
}

/** Once, on the first start: that the app asks GitHub for new versions, and how to stop it. */
export function UpdateCheckIntro() {
  const enabled = useUiStore((s) => s.updateCheck);
  const explained = useUiStore((s) => s.updateCheckExplained);
  const explain = useUiStore((s) => s.explainUpdateCheck);
  const setUpdateCheck = useUiStore((s) => s.setUpdateCheck);
  if (!enabled || explained) return null;
  return (
    <div className="pointer-events-none fixed right-4 bottom-4 z-40 flex justify-end">
      <div
        role="status"
        className="pointer-events-auto grid max-w-sm gap-3 rounded-xl border bg-popover p-4 text-sm text-popover-foreground shadow-[0_4px_16px_var(--shadow-color)]"
      >
        <p className="flex gap-2">
          <Icon name="download" className="mt-0.5 size-4 shrink-0 text-primary" />
          {t("update.intro")}
        </p>
        <div className="flex justify-end gap-2">
          <Button variant="ghost" size="sm" onClick={() => setUpdateCheck(false)}>
            {t("update.introOff")}
          </Button>
          <Button size="sm" onClick={explain}>
            {t("update.introOk")}
          </Button>
        </div>
      </div>
    </div>
  );
}
