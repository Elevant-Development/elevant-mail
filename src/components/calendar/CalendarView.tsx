import { useEffect, useEffectEvent } from "react";

import { Icon } from "@/components/common/Icon";
import { IconButton } from "@/components/common/IconButton";
import { Button } from "@/components/ui/button";
import {
  addDays,
  type CalendarView as View,
  defaultCalendar,
  newEvent,
  nextFullHour,
  periodTitle,
  shiftAnchor,
  viewDays,
} from "@/lib/calendar";
import { locale, t } from "@/lib/i18n";
import { hasOpenOverlay, isTypingTarget } from "@/lib/keyboard";
import type { Occurrence } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useCalendars, useOccurrences } from "@/queries/calendar";
import { useCalendarStore } from "@/stores/calendar";
import { useUiStore } from "@/stores/ui";

import { MonthGrid } from "./MonthGrid";
import { TimeGrid } from "./TimeGrid";

const views: {
  value: View;
  label: "calendar.day" | "calendar.week" | "calendar.month";
  key: string;
}[] = [
  { value: "day", label: "calendar.day", key: "d" },
  { value: "week", label: "calendar.week", key: "w" },
  { value: "month", label: "calendar.month", key: "m" },
];

interface CalendarViewProps {
  className?: string;
}

/**
 * The calendar area: day, week or month with the events of the calendars switched on in the
 * sidebar. Keys: t today, ←/→ or j/k back and on, d/w/m the view, n a new event.
 */
export function CalendarView({ className }: CalendarViewProps) {
  const view = useCalendarStore((s) => s.view);
  const anchor = useCalendarStore((s) => s.anchor);
  const setView = useCalendarStore((s) => s.setView);
  const setAnchor = useCalendarStore((s) => s.setAnchor);
  const showDay = useCalendarStore((s) => s.showDay);
  const openEvent = useCalendarStore((s) => s.openEvent);
  const editEvent = useCalendarStore((s) => s.editEvent);
  const { data: calendars = [] } = useCalendars();

  const days = viewDays(view, anchor, locale);
  const { data: all = [] } = useOccurrences(days[0]!, addDays(days[days.length - 1]!, 1));
  const byId = new Map(calendars.map((calendar) => [calendar.id, calendar]));
  const occurrences = all.filter((occurrence) => byId.get(occurrence.calendarId)?.visible);
  const colorOf = (occurrence: Occurrence) =>
    byId.get(occurrence.calendarId)?.color ?? "var(--primary)";
  const target = defaultCalendar(calendars);

  const create = (start: number, allDay: boolean) => {
    if (target) {
      editEvent(newEvent(start, allDay, target.id, useUiStore.getState().defaultReminder));
    }
  };
  const open = (occurrence: Occurrence) => openEvent(occurrence.eventId, occurrence.instance);

  const onKeyDown = useEffectEvent((event: KeyboardEvent) => {
    if (event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey) return;
    if (isTypingTarget(event.target) || hasOpenOverlay()) return;
    const key = event.key;
    if (key === "ArrowLeft" || key === "k") setAnchor(shiftAnchor(view, anchor, -1));
    else if (key === "ArrowRight" || key === "j") setAnchor(shiftAnchor(view, anchor, 1));
    else if (key === "t") setAnchor(Date.now());
    else if (key === "n" || key === "c") create(nextFullHour(Date.now()), false);
    else {
      const chosen = views.find((option) => option.key === key);
      if (!chosen) return;
      setView(chosen.value);
    }
    event.preventDefault();
  });
  useEffect(() => {
    const listener = (event: KeyboardEvent) => onKeyDown(event);
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, []);

  return (
    <section aria-label={t("calendar.title")} className={cn("flex min-h-0 flex-col", className)}>
      <header className="flex shrink-0 flex-wrap items-center gap-2 border-b px-3 py-2">
        <Button variant="outline" size="sm" onClick={() => setAnchor(Date.now())}>
          {t("calendar.today")}
        </Button>
        <div className="flex">
          <IconButton
            label={t("calendar.previous")}
            icon={<Icon name="chevron" className="rotate-180" />}
            onClick={() => setAnchor(shiftAnchor(view, anchor, -1))}
          />
          <IconButton
            label={t("calendar.next")}
            icon={<Icon name="chevron" />}
            onClick={() => setAnchor(shiftAnchor(view, anchor, 1))}
          />
        </div>
        <h2 className="min-w-0 flex-1 truncate text-base font-semibold" aria-live="polite">
          {periodTitle(view, anchor, locale)}
        </h2>
        <div
          role="radiogroup"
          aria-label={t("calendar.view")}
          className="flex gap-1 rounded-lg bg-muted p-1"
        >
          {views.map((option) => (
            <button
              key={option.value}
              type="button"
              role="radio"
              aria-checked={view === option.value}
              title={`${t(option.label)} (${option.key})`}
              onClick={() => setView(option.value)}
              className={cn(
                "h-6 rounded-md px-2.5 text-xs font-medium transition-colors",
                view === option.value
                  ? "bg-card text-foreground shadow-sm"
                  : "text-muted-foreground hover:text-foreground",
              )}
            >
              {t(option.label)}
            </button>
          ))}
        </div>
      </header>
      {view === "month" ? (
        <MonthGrid
          days={days}
          anchor={anchor}
          occurrences={occurrences}
          colorOf={colorOf}
          onOpen={open}
          onCreate={create}
          onShowDay={showDay}
        />
      ) : (
        <TimeGrid
          // A new period scrolls to the working day again.
          key={view}
          days={days}
          occurrences={occurrences}
          colorOf={colorOf}
          onOpen={open}
          onCreate={create}
          onShowDay={view === "week" ? showDay : undefined}
        />
      )}
    </section>
  );
}
