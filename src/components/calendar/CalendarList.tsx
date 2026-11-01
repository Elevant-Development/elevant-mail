import { useState } from "react";
import { cn } from "cn";

import { Icon } from "@/components/common/Icon";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { accountColors } from "@/lib/account-colors";
import { t } from "@/lib/i18n";
import type { Calendar } from "@/lib/tauri";
import { useAccounts } from "@/queries/accounts";
import { useCalendars, useSetCalendarColor, useSetCalendarVisible } from "@/queries/calendar";
import { useCalendarStore } from "@/stores/calendar";
import { useUiStore } from "@/stores/ui";

import { UnlockCalendars } from "./UnlockCalendars";

import { calendarName } from "./event-style";

/** The colours a calendar can have: the accounts' ones, and back to its account's. */
function ColorChoice({ calendar }: { calendar: Calendar }) {
  const setColor = useSetCalendarColor();
  const [open, setOpen] = useState(false);
  const name = calendarName(calendar);
  const choose = (color: string | null) => {
    setColor.mutate({ calendarId: calendar.id, color });
    setOpen(false);
  };
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={t("calendar.color.choose", { name })}
          title={t("calendar.color.choose", { name })}
          className={cn(
            "flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground opacity-0 transition-opacity outline-none group-hover:opacity-100 hover:bg-sidebar-accent hover:text-foreground focus-visible:opacity-100 focus-visible:ring-2 focus-visible:ring-ring",
            open && "opacity-100",
          )}
        >
          <Icon name="palette" className="size-3.5" />
        </button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-auto gap-3 p-3">
        <p className="text-xs font-medium text-muted-foreground">
          {t("calendar.color.title", { name })}
        </p>
        <div
          role="radiogroup"
          aria-label={t("calendar.color.title", { name })}
          className="flex gap-2"
        >
          {accountColors.map((color) => {
            const selected = calendar.ownColor && calendar.color === color.value;
            return (
              <button
                key={color.value}
                type="button"
                role="radio"
                aria-checked={selected}
                aria-label={t(color.name)}
                title={t(color.name)}
                onClick={() => choose(color.value)}
                style={{ backgroundColor: color.value }}
                className={cn(
                  "flex size-6 items-center justify-center rounded-full ring-offset-2 ring-offset-popover transition-shadow outline-none hover:ring-2 hover:ring-foreground/20 focus-visible:ring-2 focus-visible:ring-ring",
                  selected && "ring-2 ring-foreground/70 hover:ring-foreground/70",
                )}
              >
                {selected && <Icon name="check" className="size-3 text-black/70" />}
              </button>
            );
          })}
        </div>
        {calendar.ownColor && (
          <button
            type="button"
            onClick={() => choose(null)}
            className="-mx-1 rounded-md px-1 py-0.5 text-left text-xs text-muted-foreground hover:text-foreground"
          >
            {calendar.accountId ? t("calendar.color.account") : t("calendar.color.standard")}
          </button>
        )}
      </PopoverContent>
    </Popover>
  );
}

function CalendarRow({ calendar }: { calendar: Calendar }) {
  const setVisible = useSetCalendarVisible();
  return (
    <div className="group flex h-8 items-center gap-1 rounded-md pr-1 hover:bg-sidebar-accent/70">
      <label className="flex h-full min-w-0 flex-1 cursor-pointer items-center gap-2.5 pl-2 text-sm select-none">
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
          <span className="shrink-0 text-[11px] text-muted-foreground">
            {t("calendar.readOnly")}
          </span>
        )}
      </label>
      <ColorChoice calendar={calendar} />
    </div>
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
