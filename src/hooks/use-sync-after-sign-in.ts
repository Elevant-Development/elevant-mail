import { useCalendarSync } from "@/hooks/use-calendar-sync";
import { useContactsSync } from "@/hooks/use-contacts-sync";
import { useCalendarStore } from "@/stores/calendar";
import { useContactsStore } from "@/stores/contacts";

/**
 * Microsoft takes a few seconds after a sign-in until what was just allowed counts: a sync right
 * away may still be refused. Asked again after these pauses before the refusal stands.
 */
const RETRY_AFTER_MS = [2000, 5000, 10000, 20000];

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

async function retrying(run: () => Promise<void>, refused: () => boolean) {
  await run();
  for (const pause of RETRY_AFTER_MS) {
    if (!refused()) return;
    await wait(pause);
    await run();
  }
}

/** Syncs an account's calendars and contacts after it signed in or was added. */
export function useSyncAfterSignIn() {
  const { syncCalendars } = useCalendarSync();
  const { syncContacts } = useContactsSync();
  return (accountId: string) =>
    Promise.all([
      retrying(
        () => syncCalendars(accountId),
        () => useCalendarStore.getState().syncErrors[accountId]?.kind === "calendarAccessMissing",
      ),
      retrying(
        () => syncContacts(accountId),
        () => useContactsStore.getState().syncErrors[accountId]?.kind === "contactsAccessMissing",
      ),
    ]);
}
