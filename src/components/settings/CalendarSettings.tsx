import { type FormEvent, useState } from "react";

import { Field } from "@/components/accounts/ServerFields";
import { Icon } from "@/components/common/Icon";
import { Notice } from "@/components/common/Notice";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { describeError } from "@/lib/errors";
import { t } from "@/lib/i18n";
import type { Account, CaldavStatus } from "@/lib/tauri";
import { useCaldavStatus, useSetCaldav } from "@/queries/calendar";
import { useCalendarStore } from "@/stores/calendar";
import { UnlockCalendars } from "@/components/calendar/UnlockCalendars";

/**
 * The account's calendars over CalDAV: on or off, where they were found, and a field for the
 * server's address where the app does not find them by itself.
 */
export function CalendarSettings({ account }: { account: Account }) {
  const { data: status } = useCaldavStatus(account.id);
  if (!status) return <Icon name="spinner" className="size-4 animate-spin text-muted-foreground" />;
  if (status.microsoft) return <MicrosoftCalendars account={account} status={status} />;
  return <CaldavForm account={account} status={status} />;
}

function CaldavForm({ account, status }: { account: Account; status: CaldavStatus }) {
  const setCaldav = useSetCaldav(account.id);
  const syncError = useCalendarStore((s) => s.syncErrors[account.id]);
  const [url, setUrl] = useState(status.url ?? "");
  const [checked, setChecked] = useState(false);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    setCaldav.mutate(
      { enabled: true, url: url.trim() || null },
      { onSuccess: () => setChecked(true) },
    );
  };

  return (
    <div className="grid gap-3">
      <Label className="justify-between font-normal">
        {t("settings.calendar.sync")}
        <Switch
          checked={status.enabled}
          disabled={setCaldav.isPending}
          onCheckedChange={(enabled) => {
            setChecked(false);
            setCaldav.mutate({ enabled, url: status.url });
          }}
        />
      </Label>
      {status.enabled && (
        <>
          <p className="text-sm text-muted-foreground">
            {status.url
              ? t("settings.calendar.found", { url: status.url })
              : t("settings.calendar.searching")}
          </p>
          {syncError && <Notice tone="error" text={syncError.message} detail={syncError.action} />}
          <form onSubmit={submit} className="grid gap-2">
            <Field id={`caldav-${account.id}`} label={t("settings.calendar.server")}>
              <Input
                id={`caldav-${account.id}`}
                value={url}
                placeholder="https://mail.example.org/SOGo/dav/"
                spellCheck={false}
                onChange={(event) => {
                  setUrl(event.target.value);
                  setChecked(false);
                }}
              />
            </Field>
            <p className="text-xs text-muted-foreground">{t("settings.calendar.serverHint")}</p>
            <Button
              type="submit"
              variant="outline"
              size="sm"
              className="justify-self-start"
              disabled={setCaldav.isPending}
            >
              {setCaldav.isPending && <Icon name="spinner" className="animate-spin" />}
              {t("settings.calendar.check")}
            </Button>
          </form>
          {setCaldav.isError && (
            <Notice
              tone="error"
              text={describeError(setCaldav.error).message}
              detail={describeError(setCaldav.error).action}
            />
          )}
          {checked && !setCaldav.isError && (
            <Notice tone="success" text={t("settings.calendar.checked")} />
          )}
        </>
      )}
    </div>
  );
}

/** Microsoft 365 and Outlook.com: the calendars come over Microsoft Graph with the sign-in. */
function MicrosoftCalendars({ account, status }: { account: Account; status: CaldavStatus }) {
  const setCaldav = useSetCaldav(account.id);
  const syncError = useCalendarStore((s) => s.syncErrors[account.id]);
  return (
    <div className="grid gap-3">
      <Label className="justify-between font-normal">
        {t("settings.calendar.syncMicrosoft")}
        <Switch
          checked={status.enabled}
          disabled={setCaldav.isPending}
          onCheckedChange={(enabled) => setCaldav.mutate({ enabled, url: null })}
        />
      </Label>
      {status.enabled && (
        <p className="text-sm text-muted-foreground">{t("settings.calendar.microsoft")}</p>
      )}
      {status.enabled && syncError && (
        <Notice tone="error" text={syncError.message} detail={syncError.action} />
      )}
      {status.enabled && syncError?.kind === "calendarAccessMissing" && (
        <UnlockCalendars accountId={account.id} />
      )}
    </div>
  );
}
