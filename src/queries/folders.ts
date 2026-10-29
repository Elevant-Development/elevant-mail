import { useQuery } from "@tanstack/react-query";

import { api } from "@/lib/tauri";

export function useFolders() {
  return useQuery({ queryKey: ["folders"], queryFn: api.listFolders });
}
