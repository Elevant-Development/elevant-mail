import { useQuery } from "@tanstack/react-query";

import { api } from "@/lib/tauri";

export function useAccounts() {
  return useQuery({ queryKey: ["accounts"], queryFn: api.listAccounts });
}
