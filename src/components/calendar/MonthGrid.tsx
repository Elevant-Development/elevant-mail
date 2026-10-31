import { useNow } from "@/hooks/use-now";
import { isOnDay, startOfDay } from "@/lib/calendar";
import { locale, t, tCount } from "@/lib/i18n";
import type { Occurrence } from "@/lib/tauri";
import { cn } from "@/lib/utils";

import { eventStyle, formatTime } from "./event-style";

/** Events named in a day before "3 more". */
const SHOWN_PER_DAY = 3;

interface MonthGridProps {
  /** Six weeks of days, from the week of the month's first day. */
  days: number[];
  /** Any day of the month shown; days of other months are dimmed. */
  anchor: number;
  occurrences: Occurrence[];
  colorOf: (occurrence: Occurrence) => string;
  onOpen: (occurrence: Occurrence) => void;
  onCreate: (start: number, allDay: boolean) => void;
  onShowDay: (day: number) => void;
}

/** Month view: six weeks, a few events per day, the rest behind "more". */
export function MonthGrid({
  days,
  anchor,
  occurrences,
  colorOf,
  onOpen,
  onCreate,
  onShowDay,
}: MonthGridProps) {
  const today = startOfDay(useNow());
  const month = new Date(anchor).getMonth();
  const weekdays = days
    .slice(0, 7)
    .map((day) => new Intl.DateTimeFormat(locale, { weekday: "short" }).format(day));

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="grid shrink-0 grid-cols-7 border-b">
        {weekdays.map((weekday, index) => (
          <span
            key={index}
            className="border-l px-2 py-1.5 text-xs text-muted-foreground first:border-l-0"
          >
            {weekday}
          </span>
        ))}
      </div>
      <div className="grid min-h-0 flex-1 grid-cols-7 grid-rows-6">
        {days.map((day) => {
          const onDay = occurrences.filter((occurrence) => isOnDay(occurrence, day));
          const shown = onDay.slice(0, SHOWN_PER_DAY);
          const hidden = onDay.length - shown.length;
          const inMonth = new Date(day).getMonth() === month;
          return (
            <div
              key={day}
              onClick={(event) => {
                if (event.target !== event.currentTarget) return;
                const date = new Date(day);
                date.setHours(9);
                onCreate(date.getTime(), false);
              }}
              className={cn(
                "flex min-h-0 min-w-0 flex-col gap-0.5 overflow-hidden border-b border-l p-1 [&:nth-child(7n+1)]:border-l-0",
                !inMonth && "bg-muted/30",
              )}
            >
              <button
                type="button"
                onClick={() => onShowDay(day)}
                aria-label={new Intl.DateTimeFormat(locale, { dateStyle: "full" }).format(day)}
                className={cn(
                  "flex size-6 shrink-0 items-center justify-center self-start rounded-full text-xs font-medium tabular-nums hover:bg-row-hover",
                  !inMonth && "text-muted-foreground",
                  day === today && "bg-primary text-primary-foreground hover:bg-primary/80",
                )}
              >
                {new Date(day).getDate()}
              </button>
              {shown.map((occurrence) => {
                const color = colorOf(occurrence);
                const title = occurrence.title ?? t("invitation.untitled");
                return (
                  <button
                    key={`${occurrence.eventId}-${occurrence.start}`}
                    type="button"
                    title={title}
                    onClick={() => onOpen(occurrence)}
                    style={occurrence.allDay ? eventStyle(color) : undefined}
                    className={cn(
                      "flex h-5 min-w-0 shrink-0 items-center gap-1 rounded px-1 text-left text-[11px] leading-none hover:bg-row-hover",
                      occurrence.allDay && "font-medium hover:brightness-110",
                    )}
                  >
                    {!occurrence.allDay && (
                      <>
                        <span
                          aria-hidden
                          className="size-1.5 shrink-0 rounded-full"
                          style={{ backgroundColor: color }}
                        />
                        <span className="shrink-0 text-muted-foreground tabular-nums">
                          {formatTime(occurrence.start, locale)}
                        </span>
                      </>
                    )}
                    <span className="truncate">{title}</span>
                  </button>
                );
              })}
              {hidden > 0 && (
                <button
                  type="button"
                  onClick={() => onShowDay(day)}
                  className="h-5 shrink-0 rounded px-1 text-left text-[11px] text-muted-foreground hover:bg-row-hover"
                >
                  {tCount("calendar.more", hidden)}
                </button>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
