import { QueryClient } from "@tanstack/react-query";

/**
 * The one query cache of the app. In its own module so a hot reload of App.tsx during
 * development does not swap it for an empty one under running hooks.
 */
// Data comes from the local cache, so refetching on window focus only adds noise.
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: { refetchOnWindowFocus: false, staleTime: 30_000 },
  },
});
