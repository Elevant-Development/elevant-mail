import type { ResolvedMode } from "@/design/themes";
import { useUiStore } from "@/stores/ui";

import { useMediaQuery } from "./use-media-query";

/** Dark or light as shown right now, with "system" resolved. */
export function useResolvedMode(): ResolvedMode {
  const mode = useUiStore((s) => s.mode);
  const systemDark = useMediaQuery("(prefers-color-scheme: dark)");
  return mode === "dark" || (mode === "system" && systemDark) ? "dark" : "light";
}
