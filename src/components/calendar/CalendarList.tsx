import { Icon } from "@/components/common/Icon";
import { t } from "@/lib/i18n";
import type { Calendar } from "@/lib/tauri";
import { useAccounts } from "@/queries/accounts";
import { useCalendars, useSetCalendarVisible } from "@/queries/calendar";
import { useCalendarStore } from "@/stores/calendar";
import { useUiStore } from "@/stores/ui";

import { UnlockCalendars } from "./UnlockCalendars";

import { calendarName } from "./event-style";

function CalendarRow({ calendar }: { calendar: Calendar }) {
  const setVisible = useSetCalendarVisible();
  return (
    <label className="flex h-8 cursor-pointer items-center gap-2.5 rounded-md px-2 text-sm select-none hover:bg-sidebar-accent/70">
      <input
        type="checkbox"
        checked={calendar.visible}
        onChange={(input) =>
          setVisible.mutate({ calendarId: calendar.id, visible: input.target.checked })
        }
        className="size-4 shrink-0 cursor-pointer rounded"
        style={{ accentColor: calendar.color }}
      />
      <span className="min-w-0 flex-1 truncate">{calendarName(calendar)}</span>
      {calendar.readOnly && (
        <span className="shrink-0 text-[11px] text-muted-foreground">{t("calendar.readOnly")}</span>
      )}
    </label>
  );
}

/** The calendars per account, each shown or hidden with its box; the one on this computer last. */
export function CalendarList() {
  const { data: calendars = [] } = useCalendars();
  const { data: accounts = [] } = useAccounts();
  const errors = useCalendarStore((s) => s.syncErrors);
  const syncing = useCalendarStore((s) => s.syncing);
  const openSettings = useUiStore((s) => s.openSettings);
  const groups = [
    ...accounts.map((account) => ({
      key: account.id,
      accountId: account.id as string | null,
      label: account.displayName,
      calendars: calendars.filter((calendar) => calendar.accountId === account.id),
    })),
    {
      key: "local",
      accountId: null,
      label: t("calendar.thisComputer"),
      calendars: calendars.filter((calendar) => calendar.accountId === null),
    },
  ].filter((group) => group.calendars.length > 0 || (group.accountId && errors[group.accountId]));

  return (
    <div className="flex flex-col gap-4">
      {groups.map((group) => (
        <section key={group.key} aria-label={group.label}>
          <h3 className="mb-1 flex items-center gap-1.5 px-2 text-xs font-medium text-muted-foreground">
            {group.label}
            {group.accountId && syncing.includes(group.accountId) && (
              <span role="status" title={t("calendar.syncing")}>
                <Icon name="spinner" className="size-3 animate-spin" />
                <span className="sr-only">{t("calendar.syncing")}</span>
              </span>
            )}
          </h3>
          {group.calendars.map((calendar) => (
            <CalendarRow key={calendar.id} calendar={calendar} />
          ))}
          {group.accountId && errors[group.accountId]?.kind === "calendarAccessMissing" && (
            <div className="grid gap-2 px-2 py-1">
              <p className="text-xs text-muted-foreground">{t("calendar.unlockHint")}</p>
              <UnlockCalendars accountId={group.accountId} />
            </div>
          )}
          {group.accountId &&
            errors[group.accountId] &&
            errors[group.accountId]?.kind !== "calendarAccessMissing" && (
              <button
                type="button"
                onClick={() => openSettings({ section: "accounts", accountId: group.accountId })}
                className="mt-1 flex w-full items-start gap-2 rounded-md px-2 py-1 text-left text-xs text-destructive hover:bg-sidebar-accent/70"
              >
                <Icon name="alert" className="mt-px size-3.5 shrink-0" />
                <span>{errors[group.accountId]?.message}</span>
              </button>
            )}
        </section>
      ))}
      {accounts.length > 0 && calendars.every((calendar) => calendar.accountId === null) && (
        <p className="px-2 text-xs text-muted-foreground">{t("calendar.noAccountCalendars")}</p>
      )}
    </div>
  );
}
