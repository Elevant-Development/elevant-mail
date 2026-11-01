import { useQueryClient } from "@tanstack/react-query";

import { describeError } from "@/lib/errors";
import { oneAtATime } from "@/lib/one-at-a-time";
import { api, type Calendar } from "@/lib/tauri";
import { useCalendarStore } from "@/stores/calendar";

/** One sync per account at a time; asked for meanwhile, it runs once more after. */
const turn = oneAtATime();

/**
 * Syncs an account's calendars over CalDAV: the changes made here go out, the server's come
 * in, and the views follow. A failure is kept per account for the calendar list.
 */
export function useCalendarSync() {
  const queryClient = useQueryClient();

  const syncCalendars = (accountId: string) =>
    turn(accountId, async () => {
      const { setSyncError, setSyncing } = useCalendarStore.getState();
      setSyncing(accountId, true);
      try {
        await api.syncCalendars(accountId);
        setSyncError(accountId, null);
      } catch (error) {
        setSyncError(accountId, describeError(error));
      } finally {
        setSyncing(accountId, false);
        // Also after a failure: what came before it is in the cache.
        await Promise.all(
          ["calendars", "occurrences", "event"].map((key) =>
            queryClient.invalidateQueries({ queryKey: [key] }),
          ),
        );
      }
    });

  /** After a change in a calendar: sends it to that calendar's server right away. */
  const sendChanges = (calendarId: string) => {
    const calendar = queryClient
      .getQueryData<Calendar[]>(["calendars"])
      ?.find((c) => c.id === calendarId);
    if (calendar?.accountId) void syncCalendars(calendar.accountId);
  };

  return { syncCalendars, sendChanges };
}
