import { type ReactNode, useState } from "react";

import { Icon, type IconName } from "@/components/common/Icon";
import { Notice } from "@/components/common/Notice";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { addDays, HOUR_MS, localDay, startOfDay, utcDate } from "@/lib/calendar";
import { describeError } from "@/lib/errors";
import { locale, type MessageKey, t } from "@/lib/i18n";
import { formatAttendees, formatEventTime, formatRecurrence } from "@/lib/invitations";
import { reminderChoices, reminderLabel } from "@/lib/reminders";
import type { Account, Address, Calendar, EventDetails, EventInput, Frequency } from "@/lib/tauri";
import { AddressField } from "@/components/compose/AddressField";
import { useAccounts } from "@/queries/accounts";
import {
  useCalendars,
  useDeleteEvent,
  useEvent,
  useLoadSeries,
  useSaveEvent,
} from "@/queries/calendar";
import { useCalendarStore } from "@/stores/calendar";

import { calendarName } from "./event-style";

const DAY_MS = 24 * HOUR_MS;

/** "Arbeit · Kalender", or the name alone for the calendar on this computer. */
function fullName(calendar: Calendar, accounts: Account[]): string {
  const account = accounts.find((a) => a.id === calendar.accountId);
  const name = calendarName(calendar);
  return account && account.displayName !== name ? `${account.displayName} · ${name}` : name;
}

const pad = (value: number) => String(value).padStart(2, "0");
const dateValue = (date: Date) =>
  `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
const timeValue = (date: Date) => `${pad(date.getHours())}:${pad(date.getMinutes())}`;

/** The value of a date field and a time field as a local moment. */
function fromFields(date: string, time: string): number | null {
  const [year, month, day] = date.split("-").map(Number);
  const [hours, minutes] = time.split(":").map(Number);
  if (!year || !month || !day || hours === undefined || minutes === undefined) return null;
  if (Number.isNaN(hours) || Number.isNaN(minutes)) return null;
  return new Date(year, month - 1, day, hours, minutes).getTime();
}

/** A date field for an all-day date (midnight UTC); the last day of an event is the day before its end. */
const allDayValue = (utcMidnight: number) => dateValue(new Date(localDay(utcMidnight)));

export function EventDialog() {
  const dialog = useCalendarStore((s) => s.dialog);
  const closeDialog = useCalendarStore((s) => s.closeDialog);
  return (
    <Dialog open={dialog !== null} onOpenChange={(open) => !open && closeDialog()}>
      <DialogContent aria-describedby={undefined} className="gap-0 p-0 sm:max-w-lg">
        {dialog?.mode === "view" && (
          <EventView eventId={dialog.eventId} instance={dialog.instance} />
        )}
        {dialog?.mode === "edit" && (
          // A new key per event, so the form starts from it.
          <EventForm
            key={`${dialog.event.id ?? `new-${dialog.event.start}`} ${dialog.event.instance}`}
            initial={dialog.event}
            ownEvent={dialog.ownEvent}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

function Row({ icon, label, children }: { icon: IconName; label: string; children: ReactNode }) {
  return (
    <div className="flex items-start gap-3">
      <Icon name={icon} className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
      <span className="sr-only">{label}: </span>
      <div className="min-w-0 flex-1 break-words">{children}</div>
    </div>
  );
}

function toInput(event: EventDetails): EventInput {
  return {
    id: event.id,
    calendarId: event.calendarId,
    title: event.title ?? "",
    location: event.location ?? "",
    description: event.description ?? "",
    start: event.start,
    end: event.end,
    allDay: event.allDay,
    repeat: event.recurrence?.frequency ?? null,
    attendees: event.attendees.map((attendee) => attendee.address),
    reminder: event.reminder,
    instance: null,
  };
}

/** What the buttons below an event ask about: deleting it, or which dates of a series. */
type Asking = "delete" | "edit" | null;

function EventView({ eventId, instance }: { eventId: string; instance: number | null }) {
  const { data: event, isError, error } = useEvent(eventId, instance);
  const { data: calendars = [] } = useCalendars();
  const { data: accounts = [] } = useAccounts();
  const editEvent = useCalendarStore((s) => s.editEvent);
  const closeDialog = useCalendarStore((s) => s.closeDialog);
  const remove = useDeleteEvent();
  const loadSeries = useLoadSeries();
  const [asking, setAsking] = useState<Asking>(null);
  const [loadError, setLoadError] = useState<unknown>(null);

  if (isError) {
    return (
      <div className="grid gap-3 p-5">
        <DialogTitle>{t("calendar.event")}</DialogTitle>
        <Notice tone="error" text={describeError(error).message} />
      </div>
    );
  }
  if (!event) {
    return (
      <div className="p-5">
        <DialogTitle className="sr-only">{t("calendar.event")}</DialogTitle>
        <div aria-hidden className="h-5 w-2/3 rounded bg-muted" />
      </div>
    );
  }
  const calendar = calendars.find((c) => c.id === event.calendarId);
  const people = event.organizer !== null || event.attendees.length > 0;
  // Opened as one date of a series: changes can be for it alone or for all dates.
  const oneDate = event.recurrence !== null && event.instance !== null;
  const deleteDates = (only: boolean) =>
    remove.mutate(
      { eventId: event.id, calendarId: event.calendarId, instance: only ? event.instance : null },
      { onSuccess: closeDialog },
    );
  const editSeries = () => {
    setLoadError(null);
    loadSeries(event.id).then(
      (series) => editEvent(toInput(series), series.ownEvent),
      setLoadError,
    );
  };

  return (
    <div className="grid gap-4 p-5">
      <div className="flex items-start gap-3 pr-8">
        <span
          aria-hidden
          className="mt-1.5 size-3 shrink-0 rounded"
          style={{ backgroundColor: calendar?.color }}
        />
        <DialogTitle className="text-lg leading-snug break-words">
          {event.title ?? t("invitation.untitled")}
        </DialogTitle>
      </div>
      <div className="grid gap-2.5 text-sm">
        <Row icon="calendar" label={t("invitation.when")}>
          {formatEventTime(event, locale)}
        </Row>
        {event.recurrence && (
          <Row icon="repeat" label={t("invitation.repeats")}>
            {formatRecurrence(event.recurrence, locale)}
          </Row>
        )}
        {event.location && (
          <Row icon="mapPin" label={t("invitation.where")}>
            {event.location}
          </Row>
        )}
        {people && (
          <Row icon="users" label={t("invitation.who")}>
            {formatAttendees(event, locale)}
          </Row>
        )}
        {event.reminder !== null && (
          <Row icon="bell" label={t("reminder.label")}>
            {reminderLabel(event.reminder, locale)}
          </Row>
        )}
        {calendar && (
          <Row icon="inbox" label={t("calendar.calendar")}>
            {fullName(calendar, accounts)}
          </Row>
        )}
        {event.description && (
          <p className="mt-1 max-h-60 overflow-y-auto border-t pt-3 whitespace-pre-wrap text-muted-foreground">
            {event.description}
          </p>
        )}
      </div>
      {remove.isError && <Notice tone="error" text={describeError(remove.error).message} />}
      {loadError !== null && <Notice tone="error" text={describeError(loadError).message} />}
      {!event.readOnly && (
        <div className="flex flex-wrap items-center justify-end gap-2 border-t pt-4">
          {asking === "delete" && oneDate && (
            <>
              <p className="mr-auto text-sm text-muted-foreground">{t("calendar.deleteWhich")}</p>
              <Button variant="ghost" size="sm" onClick={() => setAsking(null)}>
                {t("account.add.cancel")}
              </Button>
              <Button
                variant="outline"
                size="sm"
                disabled={remove.isPending}
                onClick={() => deleteDates(false)}
              >
                {t("calendar.allDates")}
              </Button>
              <Button
                variant="destructive"
                size="sm"
                disabled={remove.isPending}
                onClick={() => deleteDates(true)}
              >
                <Icon name="trash" />
                {t("calendar.onlyThisDate")}
              </Button>
            </>
          )}
          {asking === "delete" && !oneDate && (
            <>
              <p className="mr-auto text-sm text-muted-foreground">
                {event.recurrence ? t("calendar.deleteSeries") : t("calendar.deleteQuestion")}
              </p>
              <Button variant="ghost" size="sm" onClick={() => setAsking(null)}>
                {t("account.add.cancel")}
              </Button>
              <Button
                variant="destructive"
                size="sm"
                disabled={remove.isPending}
                onClick={() => deleteDates(false)}
              >
                <Icon name="trash" />
                {t("calendar.delete")}
              </Button>
            </>
          )}
          {asking === "edit" && (
            <>
              <p className="mr-auto text-sm text-muted-foreground">{t("calendar.editWhich")}</p>
              <Button variant="ghost" size="sm" onClick={() => setAsking(null)}>
                {t("account.add.cancel")}
              </Button>
              <Button variant="outline" size="sm" onClick={editSeries}>
                {t("calendar.allDates")}
              </Button>
              <Button
                size="sm"
                data-autofocus=""
                onClick={() =>
                  editEvent({ ...toInput(event), instance: event.instance }, event.ownEvent)
                }
              >
                {t("calendar.onlyThisDate")}
              </Button>
            </>
          )}
          {asking === null && (
            <>
              <Button variant="ghost" size="sm" onClick={() => setAsking("delete")}>
                <Icon name="trash" />
                {t("calendar.delete")}
              </Button>
              <Button
                size="sm"
                data-autofocus=""
                onClick={() =>
                  oneDate ? setAsking("edit") : editEvent(toInput(event), event.ownEvent)
                }
              >
                <Icon name="compose" />
                {t("calendar.edit")}
              </Button>
            </>
          )}
        </div>
      )}
    </div>
  );
}

const repeats: { value: Frequency | "none"; label: MessageKey }[] = [
  { value: "none", label: "calendar.repeat.none" },
  { value: "daily", label: "invitation.repeat.daily.one" },
  { value: "weekly", label: "invitation.repeat.weekly.one" },
  { value: "monthly", label: "invitation.repeat.monthly.one" },
  { value: "yearly", label: "invitation.repeat.yearly.one" },
];

const fieldClass =
  "h-8 w-full min-w-0 rounded-lg border border-input bg-transparent px-2.5 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 dark:bg-input/30";

/**
 * Attendees for the event: only where the calendar's server sends the invitations, and only for
 * events the account organizes (an invitation keeps the organizer's list).
 */
function Attendees({
  event,
  calendar,
  ownEvent,
  onChange,
}: {
  event: EventInput;
  calendar: Calendar | undefined;
  ownEvent: boolean;
  onChange: (attendees: Address[]) => void;
}) {
  if (!ownEvent) return null;
  if (!calendar?.canInvite) {
    return event.attendees.length > 0 || !calendar ? null : (
      <p className="text-xs text-muted-foreground">{t("calendar.noInvitations")}</p>
    );
  }
  return (
    <div className="grid gap-1">
      <AddressField
        label={t("calendar.field.attendees")}
        addresses={event.attendees}
        onChange={onChange}
      />
      {event.attendees.length > 0 && (
        <p className="text-xs text-muted-foreground">{t("calendar.invitationsHint")}</p>
      )}
    </div>
  );
}

function EventForm({ initial, ownEvent }: { initial: EventInput; ownEvent: boolean }) {
  const { data: calendars = [] } = useCalendars();
  const { data: accounts = [] } = useAccounts();
  const closeDialog = useCalendarStore((s) => s.closeDialog);
  const save = useSaveEvent();
  const [event, setEvent] = useState(initial);
  const writable = calendars.filter((calendar) => !calendar.readOnly);
  const change = (changes: Partial<EventInput>) => setEvent((e) => ({ ...e, ...changes }));

  // Fields show local dates and times; an all-day event its first and last day.
  const startDate = event.allDay ? allDayValue(event.start) : dateValue(new Date(event.start));
  const endDate = event.allDay
    ? allDayValue(Math.max(event.start, event.end - DAY_MS))
    : dateValue(new Date(event.end));
  const backwards = event.end < event.start || (event.allDay && event.end <= event.start);
  // One date of a series: its repetition, calendar and attendees are the series'.
  const oneDate = event.instance !== null;

  const setStart = (date: string, time: string) => {
    if (event.allDay) {
      const day = fromFields(date, "00:00");
      if (day === null) return;
      // The event keeps its length.
      const start = utcDate(day);
      change({ start, end: start + (event.end - event.start) });
      return;
    }
    const start = fromFields(date, time);
    if (start !== null) change({ start, end: start + (event.end - event.start) });
  };
  const setEnd = (date: string, time: string) => {
    if (event.allDay) {
      const day = fromFields(date, "00:00");
      if (day !== null) change({ end: utcDate(addDays(day, 1)) });
      return;
    }
    const end = fromFields(date, time);
    if (end !== null) change({ end });
  };
  const setAllDay = (allDay: boolean) => {
    if (allDay) {
      const first = startOfDay(event.start);
      const last = startOfDay(Math.max(event.start, event.end - 1));
      change({ allDay, start: utcDate(first), end: utcDate(addDays(last, 1)) });
    } else {
      const start = localDay(event.start) + 9 * HOUR_MS;
      change({ allDay, start, end: start + HOUR_MS });
    }
  };

  return (
    <form
      className="grid gap-4 p-5"
      onSubmit={(submit) => {
        submit.preventDefault();
        if (!backwards) save.mutate(event, { onSuccess: closeDialog });
      }}
    >
      <DialogTitle>
        {oneDate
          ? t("calendar.editDate")
          : event.id
            ? t("calendar.editEvent")
            : t("calendar.newEvent")}
      </DialogTitle>
      <Input
        data-autofocus=""
        aria-label={t("calendar.field.title")}
        placeholder={t("calendar.field.title")}
        value={event.title}
        onChange={(input) => change({ title: input.target.value })}
        className="h-9 text-base"
      />

      <div className="grid gap-3">
        <Label className="justify-between font-normal">
          {t("calendar.allDay")}
          <Switch checked={event.allDay} onCheckedChange={setAllDay} />
        </Label>
        <div className="grid grid-cols-[4rem_1fr_auto] items-center gap-2">
          <span className="text-sm text-muted-foreground">{t("calendar.field.start")}</span>
          <input
            type="date"
            aria-label={t("calendar.field.startDate")}
            className={fieldClass}
            value={startDate}
            onChange={(input) => setStart(input.target.value, timeValue(new Date(event.start)))}
          />
          {!event.allDay && (
            <input
              type="time"
              aria-label={t("calendar.field.startTime")}
              className={`${fieldClass} w-28`}
              value={timeValue(new Date(event.start))}
              onChange={(input) => setStart(startDate, input.target.value)}
            />
          )}
          <span className="text-sm text-muted-foreground">{t("calendar.field.end")}</span>
          <input
            type="date"
            aria-label={t("calendar.field.endDate")}
            aria-invalid={backwards}
            className={fieldClass}
            value={endDate}
            onChange={(input) => setEnd(input.target.value, timeValue(new Date(event.end)))}
          />
          {!event.allDay && (
            <input
              type="time"
              aria-label={t("calendar.field.endTime")}
              aria-invalid={backwards}
              className={`${fieldClass} w-28`}
              value={timeValue(new Date(event.end))}
              onChange={(input) => setEnd(endDate, input.target.value)}
            />
          )}
        </div>
        {backwards && <p className="text-xs text-destructive">{t("calendar.endBeforeStart")}</p>}
      </div>

      <div className={oneDate ? "hidden" : "grid grid-cols-2 gap-3"}>
        <label className="grid gap-1.5 text-xs text-muted-foreground">
          {t("calendar.field.repeat")}
          <select
            className={fieldClass}
            value={event.repeat ?? "none"}
            onChange={(input) =>
              change({
                repeat: input.target.value === "none" ? null : (input.target.value as Frequency),
              })
            }
          >
            {repeats.map((option) => (
              <option key={option.value} value={option.value}>
                {t(option.label)}
              </option>
            ))}
            {initial.repeat === "other" && (
              <option value="other">{t("calendar.repeat.keep")}</option>
            )}
          </select>
        </label>
        <label className="grid gap-1.5 text-xs text-muted-foreground">
          {t("calendar.calendar")}
          <select
            className={fieldClass}
            value={event.calendarId}
            onChange={(input) => change({ calendarId: input.target.value })}
          >
            {writable.map((calendar) => (
              <option key={calendar.id} value={calendar.id}>
                {fullName(calendar, accounts)}
              </option>
            ))}
          </select>
        </label>
      </div>

      <label className="grid gap-1.5 text-xs text-muted-foreground">
        {t("reminder.label")}
        <select
          className={fieldClass}
          value={event.reminder ?? "none"}
          onChange={(input) =>
            change({
              reminder: input.target.value === "none" ? null : Number(input.target.value),
            })
          }
        >
          {[
            ...reminderChoices,
            ...(reminderChoices.includes(initial.reminder) ? [] : [initial.reminder]),
          ].map((minutes) => (
            <option key={minutes ?? "none"} value={minutes ?? "none"}>
              {reminderLabel(minutes, locale)}
            </option>
          ))}
        </select>
      </label>
      {!oneDate && (
        <Attendees
          event={event}
          calendar={calendars.find((calendar) => calendar.id === event.calendarId)}
          ownEvent={ownEvent}
          onChange={(attendees) => change({ attendees })}
        />
      )}
      <Input
        aria-label={t("calendar.field.location")}
        placeholder={t("calendar.field.location")}
        value={event.location}
        onChange={(input) => change({ location: input.target.value })}
      />
      <textarea
        aria-label={t("calendar.field.description")}
        placeholder={t("calendar.field.description")}
        rows={3}
        value={event.description}
        onChange={(input) => change({ description: input.target.value })}
        className="min-h-20 w-full resize-y rounded-lg border border-input bg-transparent px-2.5 py-1.5 text-sm outline-none placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 dark:bg-input/30"
      />

      {save.isError && (
        <Notice
          tone="error"
          text={describeError(save.error).message}
          detail={describeError(save.error).action}
        />
      )}
      <div className="flex justify-end gap-2 border-t pt-4">
        <Button type="button" variant="ghost" size="sm" onClick={closeDialog}>
          {t("account.add.cancel")}
        </Button>
        <Button type="submit" size="sm" disabled={backwards || save.isPending}>
          {save.isPending && <Icon name="spinner" className="animate-spin" />}
          {t("calendar.save")}
        </Button>
      </div>
    </form>
  );
}
