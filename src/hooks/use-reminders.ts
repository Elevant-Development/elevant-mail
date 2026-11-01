import { useEffect, useEffectEvent, useRef } from "react";

import { notificationsAllowed, showNotification } from "@/lib/desktop-notify";
import { locale } from "@/lib/i18n";
import { reminderNotice } from "@/lib/reminders";
import { api } from "@/lib/tauri";
import { useUiStore } from "@/stores/ui";

/** How often the app looks for reminders that came due. */
const CHECK_EVERY_MS = 30 * 1000;

/**
 * Shows a notification for each event reminder that comes due while the app runs, also with
 * the window in the tray. Reminders due while the app was closed are not caught up.
 */
export function useReminders() {
  // Until when reminders were looked at; null before the first look.
  const checkedUntil = useRef<number | null>(null);
  const shown = useRef(new Set<string>());

  const check = useEffectEvent(async () => {
    const now = Date.now();
    const from = checkedUntil.current ?? now - CHECK_EVERY_MS;
    checkedUntil.current = now;
    if (!useUiStore.getState().eventReminders) return;
    const due = await api.dueReminders(from, now).catch(() => []);
    if (due.length === 0 || !(await notificationsAllowed())) return;
    for (const reminder of due) {
      const key = `${reminder.eventId}:${reminder.at}`;
      if (shown.current.has(key)) continue;
      shown.current.add(key);
      showNotification(reminderNotice(reminder, locale));
    }
  });

  useEffect(() => {
    const timer = window.setInterval(() => void check(), CHECK_EVERY_MS);
    return () => window.clearInterval(timer);
  }, []);
}
