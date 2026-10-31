/** UI state of the calendar: which area is open, the view, and the event dialog. */
import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

import type { CalendarView } from "@/lib/calendar";
import type { UserError } from "@/lib/errors";
import type { EventInput } from "@/lib/tauri";

export type Area = "mail" | "calendar";

/** The event dialog: one event to look at, or one being written (new or changed). */
export type EventDialog =
  | { mode: "view"; eventId: string }
  /** `ownEvent`: the account organizes it (or nobody does), so its attendees can change. */
  | { mode: "edit"; event: EventInput; ownEvent: boolean };

interface CalendarState {
  area: Area;
  view: CalendarView;
  /** A moment in the period shown; today at start. */
  anchor: number;
  dialog: EventDialog | null;
  /** Why the last calendar sync of an account failed; missing after one that worked. */
  syncErrors: Record<string, UserError | undefined>;
  /** Accounts whose calendars are syncing now. */
  syncing: string[];

  setSyncing: (accountId: string, syncing: boolean) => void;
  setSyncError: (accountId: string, error: UserError | null) => void;
  setArea: (area: Area) => void;
  setView: (view: CalendarView) => void;
  setAnchor: (anchor: number) => void;
  /** A day in the day view, e.g. from the month grid. */
  showDay: (day: number) => void;
  openEvent: (eventId: string) => void;
  editEvent: (event: EventInput, ownEvent?: boolean) => void;
  closeDialog: () => void;
}

export const useCalendarStore = create<CalendarState>()(
  persist(
    (set) => ({
      area: "mail",
      view: "week",
      anchor: Date.now(),
      dialog: null,
      syncErrors: {},
      syncing: [],

      setSyncing: (accountId, syncing) =>
        set((state) => ({
          syncing: syncing
            ? [...state.syncing.filter((id) => id !== accountId), accountId]
            : state.syncing.filter((id) => id !== accountId),
        })),
      setSyncError: (accountId, error) =>
        set((state) => ({
          syncErrors: { ...state.syncErrors, [accountId]: error ?? undefined },
        })),
      setArea: (area) => set({ area }),
      setView: (view) => set({ view }),
      setAnchor: (anchor) => set({ anchor }),
      showDay: (day) => set({ view: "day", anchor: day }),
      openEvent: (eventId) => set({ dialog: { mode: "view", eventId } }),
      editEvent: (event, ownEvent = true) => set({ dialog: { mode: "edit", event, ownEvent } }),
      closeDialog: () => set({ dialog: null }),
    }),
    {
      name: "elevant-mail-calendar",
      storage: createJSONStorage(() => localStorage),
      // The area and the date start fresh: mail and today.
      partialize: (state) => ({ view: state.view }),
    },
  ),
);
