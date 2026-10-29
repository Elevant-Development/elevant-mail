/** The system's notifications (tauri-plugin-notification); with sample data only a log line. */
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

import type { Notice } from "./notify";
import { usesMockData } from "./tauri";

/** Asks the system once if it has not decided yet; false if notifications are not allowed. */
export async function notificationsAllowed(): Promise<boolean> {
  if (usesMockData) return true;
  if (await isPermissionGranted()) return true;
  return (await requestPermission()) === "granted";
}

export function showNotification(notice: Notice) {
  if (usesMockData) {
    console.info("notification", notice);
    return;
  }
  sendNotification(notice);
}
