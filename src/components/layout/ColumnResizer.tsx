import { type KeyboardEvent, type PointerEvent, useRef, useState } from "react";

import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { type ColumnWidths, useUiStore } from "@/stores/ui";

import { columnLimits } from "./columns";

/** Arrow keys move the edge by this much. */
const KEY_STEP = 16;

/**
 * The edge right of the sidebar or the list: dragged, it sets the column's width; a double click
 * goes back to the default. It sits in the gap between the panels, the column left of it and the
 * reader as the last column of the same row.
 */
export function ColumnResizer({ column }: { column: keyof ColumnWidths }) {
  const width = useUiStore((s) => s.columnWidths[column]);
  const setWidth = useUiStore((s) => s.setColumnWidth);
  const ref = useRef<HTMLDivElement>(null);
  const [dragging, setDragging] = useState(false);
  const limits = columnLimits[column];

  /** The column's width now and the widest it may get with the reader keeping its minimum. */
  const measure = () => {
    const handle = ref.current;
    const current = handle?.previousElementSibling?.getBoundingClientRect().width ?? 0;
    const reader = handle?.parentElement?.lastElementChild?.getBoundingClientRect().width ?? 0;
    const max = Math.max(
      limits.min,
      Math.min(limits.max, current + reader - columnLimits.readerMin),
    );
    return { current, max };
  };
  const clamp = (value: number, max: number) =>
    Math.round(Math.min(Math.max(value, limits.min), max));

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    const { current, max } = measure();
    const startX = event.clientX;
    event.currentTarget.setPointerCapture(event.pointerId);
    setDragging(true);
    const onMove = (move: globalThis.PointerEvent) =>
      setWidth(column, clamp(current + move.clientX - startX, max));
    const onUp = () => {
      setDragging(false);
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
  };

  const onKeyDown = (event: KeyboardEvent) => {
    const step = event.key === "ArrowLeft" ? -KEY_STEP : event.key === "ArrowRight" ? KEY_STEP : 0;
    if (step !== 0) {
      event.preventDefault();
      const { current, max } = measure();
      setWidth(column, clamp(current + step, max));
    } else if (event.key === "Home" || event.key === "Escape") {
      setWidth(column, null);
    }
  };

  return (
    <div
      ref={ref}
      role="separator"
      aria-orientation="vertical"
      aria-label={t(column === "sidebar" ? "layout.resizeSidebar" : "layout.resizeList")}
      aria-valuemin={limits.min}
      aria-valuemax={limits.max}
      aria-valuenow={width ?? undefined}
      tabIndex={0}
      title={t("layout.resizeHint")}
      onPointerDown={onPointerDown}
      onDoubleClick={() => setWidth(column, null)}
      onKeyDown={onKeyDown}
      className="group/resizer relative -mx-2 w-2 shrink-0 cursor-col-resize touch-none outline-none"
    >
      <span
        aria-hidden
        className={cn(
          "absolute inset-y-3 left-1/2 w-0.5 -translate-x-1/2 rounded-full bg-primary opacity-0 transition-opacity",
          "group-hover/resizer:opacity-60 group-focus-visible/resizer:opacity-100",
          dragging && "opacity-100 group-hover/resizer:opacity-100",
        )}
      />
    </div>
  );
}
