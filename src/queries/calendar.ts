import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { useCalendarSync } from "@/hooks/use-calendar-sync";
import { api, type Calendar, type EventInput } from "@/lib/tauri";
import { useCalendarStore } from "@/stores/calendar";

/** While calendars sync, what already arrived shows: asked for again every 2 seconds. */
const WHILE_SYNCING_MS = 2000;

function useRefreshWhileSyncing() {
  return useCalendarStore((s) => (s.syncing.length > 0 ? WHILE_SYNCING_MS : false));
}

export function useCalendars() {
  const refetchInterval = useRefreshWhileSyncing();
  return useQuery({ queryKey: ["calendars"], queryFn: api.listCalendars, refetchInterval });
}

/** The dates of all events in `from..to`; the view keeps the old ones while it moves on. */
export function useOccurrences(from: number, to: number) {
  const refetchInterval = useRefreshWhileSyncing();
  return useQuery({
    queryKey: ["occurrences", from, to],
    queryFn: () => api.listOccurrences(from, to),
    placeholderData: keepPreviousData,
    refetchInterval,
  });
}

export function useEvent(eventId: string | null) {
  return useQuery({
    queryKey: ["event", eventId],
    queryFn: () => (eventId ? api.getEvent(eventId) : null),
    enabled: eventId !== null,
    // From the cache: a missing event stays missing.
    retry: false,
  });
}

/** Shows or hides a calendar at once; the backend keeps it for the next start. */
export function useSetCalendarVisible() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ calendarId, visible }: { calendarId: string; visible: boolean }) =>
      api.setCalendarVisible(calendarId, visible),
    onMutate: ({ calendarId, visible }) => {
      queryClient.setQueryData<Calendar[]>(["calendars"], (calendars) =>
        calendars?.map((calendar) =>
          calendar.id === calendarId ? { ...calendar, visible } : calendar,
        ),
      );
    },
    onSettled: () => queryClient.invalidateQueries({ queryKey: ["calendars"] }),
  });
}

function useEventsChanged() {
  const queryClient = useQueryClient();
  return () =>
    Promise.all([
      queryClient.invalidateQueries({ queryKey: ["occurrences"] }),
      queryClient.invalidateQueries({ queryKey: ["event"] }),
    ]);
}

export function useSaveEvent() {
  const changed = useEventsChanged();
  const { sendChanges } = useCalendarSync();
  return useMutation({
    mutationFn: (event: EventInput) => api.saveEvent(event),
    onSuccess: (_, event) => {
      sendChanges(event.calendarId);
      return changed();
    },
  });
}

export function useDeleteEvent() {
  const queryClient = useQueryClient();
  const { sendChanges } = useCalendarSync();
  return useMutation({
    mutationFn: ({ eventId }: { eventId: string; calendarId: string }) => api.deleteEvent(eventId),
    // The open dialog closes; asking for the deleted event again would only fail.
    onSuccess: (_, { calendarId }) => {
      sendChanges(calendarId);
      void queryClient.invalidateQueries({ queryKey: ["occurrences"] });
      void queryClient.invalidateQueries({ queryKey: ["message"] });
    },
  });
}

export function useCaldavStatus(accountId: string) {
  return useQuery({
    queryKey: ["caldav", accountId],
    queryFn: () => api.caldavStatus(accountId),
  });
}

/** Switches an account's calendars on or off, or sets their server; then syncs them. */
export function useSetCaldav(accountId: string) {
  const queryClient = useQueryClient();
  const { syncCalendars } = useCalendarSync();
  return useMutation({
    mutationFn: ({ enabled, url }: { enabled: boolean; url: string | null }) =>
      api.setCaldav(accountId, enabled, url),
    onSuccess: (status) => {
      queryClient.setQueryData(["caldav", accountId], status);
      void queryClient.invalidateQueries({ queryKey: ["calendars"] });
      void queryClient.invalidateQueries({ queryKey: ["occurrences"] });
      if (status.enabled) void syncCalendars(accountId);
    },
  });
}
