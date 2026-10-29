import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { AppShell } from "@/components/layout/AppShell";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useBackgroundSync } from "@/hooks/use-sync";

// Data comes from the local cache, so refetching on window focus only adds noise.
const queryClient = new QueryClient({
  defaultOptions: {
    queries: { refetchOnWindowFocus: false, staleTime: 30_000 },
  },
});

function BackgroundSync() {
  useBackgroundSync();
  return null;
}

export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <TooltipProvider delayDuration={400}>
        <BackgroundSync />
        <AppShell />
      </TooltipProvider>
    </QueryClientProvider>
  );
}
