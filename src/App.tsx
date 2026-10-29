import { QueryClientProvider } from "@tanstack/react-query";

import { AppShell } from "@/components/layout/AppShell";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useBackgroundSync } from "@/hooks/use-sync";
import { queryClient } from "@/lib/query-client";

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
