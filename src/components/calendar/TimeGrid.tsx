import { type MouseEvent, useEffect, useRef } from "react";

import { useNow } from "@/hooks/use-now";
import { isOnDay, layoutDay, MINUTE_MS, startOfDay } from "@/lib/calendar";
import { locale, t } from "@/lib/i18n";
import type { Occurrence } from "@/lib/tauri";
import { cn } from "@/lib/utils";

import { eventStyle, formatTime, formatTimeRange } from "./event-style";

/** Height of an hour in px. */
const HOUR_HEIGHT = 48;
const PX_PER_MINUTE = HOUR_HEIGHT / 60;
/** A click on an empty hour starts an event at the half hour above it. */
const SNAP_MINUTES = 30;

interface TimeGridProps {
  days: number[];
  occurrences: Occurrence[];
  colorOf: (occurrence: Occurrence) => string;
  onOpen: (occurrence: Occurrence) => void;
  /** A click on an empty slot: a new event starting then. */
  onCreate: (start: number, allDay: boolean) => void;
  /** A click on a day's heading, in the week view. */
  onShowDay?: (day: number) => void;
}

function EventBlock({
  occurrence,
  color,
  compact,
  onOpen,
}: {
  occurrence: Occurrence;
  color: string;
  compact: boolean;
  onOpen: () => void;
}) {
  const title = occurrence.title ?? t("invitation.untitled");
  return (
    <button
      type="button"
      onClick={(event) => {
        event.stopPropagation();
        onOpen();
      }}
      title={`${title}, ${formatTimeRange(occurrence, locale)}`}
      style={eventStyle(color)}
      className="flex h-full w-full flex-col overflow-hidden rounded-md px-1.5 py-0.5 text-left text-xs leading-tight ring-1 ring-card hover:brightness-110 focus-visible:outline-2 focus-visible:outline-ring"
    >
      <span className="truncate font-medium">{title}</span>
      {!compact && (
        <span className="truncate tabular-nums opacity-75">
          {formatTimeRange(occurrence, locale)}
        </span>
      )}
      {!compact && occurrence.location && (
        <span className="truncate opacity-75">{occurrence.location}</span>
      )}
    </button>
  );
}

/** Day and week view: a column per day with its hours, all-day events above. */
export function TimeGrid({
  days,
  occurrences,
  colorOf,
  onOpen,
  onCreate,
  onShowDay,
}: TimeGridProps) {
  const now = useNow();
  const today = startOfDay(now);
  const scroller = useRef<HTMLDivElement>(null);

  // Opens at the working day, or an hour before now when that is later.
  useEffect(() => {
    const element = scroller.current;
    if (!element) return;
    const hour = Math.max(7, Math.min(new Date().getHours() - 1, 17));
    element.scrollTop = hour * HOUR_HEIGHT;
  }, []);

  const createAt = (day: number) => (event: MouseEvent<HTMLDivElement>) => {
    if (event.target !== event.currentTarget) return;
    const offset = event.nativeEvent.offsetY / PX_PER_MINUTE;
    const minutes = Math.floor(offset / SNAP_MINUTES) * SNAP_MINUTES;
    const date = new Date(day);
    date.setMinutes(minutes);
    onCreate(date.getTime(), false);
  };

  const hours = Array.from({ length: 24 }, (_, hour) => hour);
  const hourLabel = (hour: number) => {
    const date = new Date(days[0]!);
    date.setHours(hour);
    return formatTime(date.getTime(), locale);
  };
  const allDay = days.map((day) => occurrences.filter((o) => o.allDay && isOnDay(o, day)));
  const hasAllDay = allDay.some((list) => list.length > 0);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 [scrollbar-gutter:stable] overflow-y-hidden border-b">
        <div className="w-14 shrink-0" />
        {days.map((day) => {
          const isToday = day === today;
          const heading = (
            <>
              <span className="text-xs text-muted-foreground">
                {new Intl.DateTimeFormat(locale, { weekday: "short" }).format(day)}
              </span>
              <span
                className={cn(
                  "flex size-7 items-center justify-center rounded-full text-sm font-semibold tabular-nums",
                  isToday && "bg-primary text-primary-foreground",
                )}
              >
                {new Date(day).getDate()}
              </span>
            </>
          );
          return onShowDay ? (
            <button
              key={day}
              type="button"
              onClick={() => onShowDay(day)}
              aria-label={new Intl.DateTimeFormat(locale, { dateStyle: "full" }).format(day)}
              className="flex min-w-0 flex-1 flex-col items-center gap-0.5 border-l py-1.5 hover:bg-row-hover"
            >
              {heading}
            </button>
          ) : (
            <div
              key={day}
              className="flex min-w-0 flex-1 flex-col items-center gap-0.5 border-l py-1.5"
            >
              {heading}
            </div>
          );
        })}
      </div>

      <div className="flex shrink-0 [scrollbar-gutter:stable] overflow-y-hidden border-b">
        <div className="flex w-14 shrink-0 items-start justify-end px-2 py-1.5 text-[10px] text-muted-foreground">
          {t("calendar.allDay")}
        </div>
        {days.map((day, index) => (
          <div
            key={day}
            onClick={(event) => {
              if (event.target === event.currentTarget) onCreate(day, true);
            }}
            className={cn(
              "flex min-w-0 flex-1 flex-col gap-0.5 border-l p-0.5",
              hasAllDay ? "min-h-8" : "min-h-6",
            )}
          >
            {allDay[index]!.map((occurrence) => (
              <div key={`${occurrence.eventId}-${occurrence.start}`} className="h-6">
                <EventBlock
                  occurrence={occurrence}
                  color={colorOf(occurrence)}
                  compact
                  onOpen={() => onOpen(occurrence)}
                />
              </div>
            ))}
          </div>
        ))}
      </div>

      <div
        ref={scroller}
        className="relative min-h-0 flex-1 [scrollbar-gutter:stable] overflow-y-auto"
      >
        <div className="relative flex" style={{ height: 24 * HOUR_HEIGHT }}>
          <div className="relative w-14 shrink-0">
            {hours.slice(1).map((hour) => (
              <span
                key={hour}
                className="absolute right-2 -translate-y-1/2 text-[10px] text-muted-foreground tabular-nums"
                style={{ top: hour * HOUR_HEIGHT }}
              >
                {hourLabel(hour)}
              </span>
            ))}
          </div>
          {days.map((day) => {
            const placed = layoutDay(occurrences, day);
            const isToday = day === today;
            return (
              <div
                key={day}
                onClick={createAt(day)}
                className={cn(
                  "relative min-w-0 flex-1 border-l bg-[repeating-linear-gradient(to_bottom,transparent_0,transparent_47px,var(--border)_47px,var(--border)_48px)]",
                  isToday && "bg-primary/[0.03]",
                )}
              >
                {placed.map((item) => (
                  <div
                    key={`${item.occurrence.eventId}-${item.occurrence.start}`}
                    className="absolute px-0.5"
                    style={{
                      top: item.top * PX_PER_MINUTE,
                      height: item.height * PX_PER_MINUTE,
                      left: `${(item.column / item.columns) * 100}%`,
                      width: `${100 / item.columns}%`,
                    }}
                  >
                    <EventBlock
                      occurrence={item.occurrence}
                      color={colorOf(item.occurrence)}
                      compact={item.height < 40}
                      onOpen={() => onOpen(item.occurrence)}
                    />
                  </div>
                ))}
                {isToday && (
                  <div
                    aria-hidden
                    className="pointer-events-none absolute inset-x-0 z-10 flex items-center"
                    style={{ top: ((now - day) / MINUTE_MS) * PX_PER_MINUTE }}
                  >
                    <span className="-ml-1 size-2 rounded-full bg-destructive" />
                    <span className="h-px flex-1 bg-destructive" />
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
