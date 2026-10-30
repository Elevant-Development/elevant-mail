import { useEffect, useRef } from "react";

import { useFolders } from "@/queries/folders";
import { useUiStore } from "@/stores/ui";

/**
 * Once the folders are known at start, opens the inbox of the account chosen in the settings
 * (Reading: "Open at start"). Without one, or when it is gone, all inboxes stay open.
 */
export function useStartMailbox() {
  const { data: folders } = useFolders();
  const started = useRef(false);
  useEffect(() => {
    if (started.current || !folders) return;
    started.current = true;
    const { startAccountId, selectMailbox } = useUiStore.getState();
    const inbox = folders.find((f) => f.accountId === startAccountId && f.role === "inbox");
    if (inbox) selectMailbox(inbox.id);
  }, [folders]);
}
