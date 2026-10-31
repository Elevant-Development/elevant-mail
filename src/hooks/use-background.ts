import { useEffect, useEffectEvent } from "react";

import { useCompose } from "@/hooks/use-compose";
import { notificationsAllowed, showNotification } from "@/lib/desktop-notify";
import { unifiedInboxes } from "@/lib/folders";
import { t, tCount } from "@/lib/i18n";
import { api } from "@/lib/tauri";
import { useAccounts } from "@/queries/accounts";
import { useFolders } from "@/queries/folders";
import { useUiStore } from "@/stores/ui";

/**
 * The app behind its tray icon: tells the backend whether closing keeps it running and gives
 * the tray menu its words and the unread count; "New mail" from the menu opens the composer.
 * The first time closing only hides the window, a notification says where the app went.
 */
export function useBackground() {
  const keepRunning = useUiStore((s) => s.keepRunning);
  const { data: accounts = [] } = useAccounts();
  const { data: folders = [] } = useFolders();
  const { compose } = useCompose();
  const unread = unifiedInboxes(folders, accounts).reduce((sum, f) => sum + f.unreadCount, 0);

  useEffect(() => {
    void api
      .configureBackground(keepRunning, {
        open: t("tray.open"),
        compose: t("compose.new"),
        quit: t("tray.quit"),
        tooltip: unread > 0 ? tCount("tray.unread", unread) : "Elevant Mail",
      })
      .catch(() => undefined);
  }, [keepRunning, unread]);

  const onCompose = useEffectEvent(() => void compose());
  const onHidden = useEffectEvent(async () => {
    const { backgroundExplained, explainBackground } = useUiStore.getState();
    if (backgroundExplained) return;
    explainBackground();
    if (await notificationsAllowed()) {
      showNotification({ title: "Elevant Mail", body: t("tray.hiddenNotice") });
    }
  });
  useEffect(() => {
    const stops = [api.onTrayCompose(onCompose), api.onWindowHidden(() => void onHidden())];
    return () => stops.forEach((stop) => void stop.then((unlisten) => unlisten()));
  }, []);
}
