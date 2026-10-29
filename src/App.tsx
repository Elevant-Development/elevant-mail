import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useEffect } from "react";

import { AppShell } from "@/components/layout/AppShell";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useUiStore } from "@/stores/ui";

// Data comes from the local cache, so refetching on window focus only adds noise.
const queryClient = new QueryClient({
  defaultOptions: {
    queries: { refetchOnWindowFocus: false, staleTime: 30_000 },
  },
});

export default function App() {
  const theme = useUiStore((s) => s.theme);

  useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark");
  }, [theme]);

  return (
    <QueryClientProvider client={queryClient}>
      <TooltipProvider delayDuration={400}>
        <AppShell />
      </TooltipProvider>
    </QueryClientProvider>
  );
}
