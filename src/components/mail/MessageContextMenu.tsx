import { type MouseEvent, type ReactNode, useState } from "react";

import { Icon } from "@/components/common/Icon";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuShortcut,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { useCompose } from "@/hooks/use-compose";
import { useMessageActions } from "@/hooks/use-message-actions";
import { t } from "@/lib/i18n";
import { MESSAGE_ATTRIBUTE } from "@/lib/keyboard";
import type { Conversation } from "@/lib/tauri";
import { useUiStore } from "@/stores/ui";

/**
 * A right-click on a mail in the list: what the keys do, as a menu. On a selected mail it acts on
 * the whole selection; another mail is selected first, as a click would.
 */
export function MessageContextMenu({
  messages,
  children,
}: {
  messages: Conversation[];
  children: ReactNode;
}) {
  const [targets, setTargets] = useState<string[]>([]);
  const { setRead, setFlagged, archive, trash } = useMessageActions();
  const { respond } = useCompose();
  const openMoveDialog = useUiStore((s) => s.openMoveDialog);

  const chosen = messages.filter((message) => targets.includes(message.id));
  const ids = chosen.map((message) => message.id);
  const one = chosen.length === 1 ? chosen[0] : undefined;
  const anyUnread = chosen.some((message) => !message.isRead);
  const anyUnflagged = chosen.some((message) => !message.isFlagged);

  const aim = (event: MouseEvent) => {
    const row = (event.target as Element).closest<HTMLElement>(`[${MESSAGE_ATTRIBUTE}]`);
    const id = row?.getAttribute(MESSAGE_ATTRIBUTE);
    // Beside the mails: no menu.
    if (!id) {
      event.preventDefault();
      return;
    }
    const { selectedIds, selectMessage } = useUiStore.getState();
    if (selectedIds.includes(id)) {
      setTargets(selectedIds);
    } else {
      selectMessage(id);
      setTargets([id]);
    }
  };

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild onContextMenu={aim}>
        {children}
      </ContextMenuTrigger>
      <ContextMenuContent>
        {one && (
          <>
            <ContextMenuItem onSelect={() => void respond(one.id, "reply")}>
              <Icon name="reply" />
              {t("mailMenu.reply")}
              <ContextMenuShortcut>R</ContextMenuShortcut>
            </ContextMenuItem>
            <ContextMenuItem onSelect={() => void respond(one.id, "replyAll")}>
              <Icon name="replyAll" />
              {t("mailMenu.replyAll")}
              <ContextMenuShortcut>A</ContextMenuShortcut>
            </ContextMenuItem>
            <ContextMenuItem onSelect={() => void respond(one.id, "forward")}>
              <Icon name="forward" />
              {t("mailMenu.forward")}
              <ContextMenuShortcut>F</ContextMenuShortcut>
            </ContextMenuItem>
            <ContextMenuSeparator />
          </>
        )}
        <ContextMenuItem onSelect={() => void setRead(ids, anyUnread)}>
          <Icon name={anyUnread ? "mailOpen" : "mail"} />
          {anyUnread ? t("reader.markRead") : t("reader.markUnread")}
          <ContextMenuShortcut>U</ContextMenuShortcut>
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => void setFlagged(ids, anyUnflagged)}>
          <Icon name="star" filled={!anyUnflagged} />
          {anyUnflagged ? t("reader.flag") : t("reader.unflag")}
          <ContextMenuShortcut>S</ContextMenuShortcut>
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem onSelect={() => void archive(ids)}>
          <Icon name="archive" />
          {t("reader.archive")}
          <ContextMenuShortcut>E</ContextMenuShortcut>
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => openMoveDialog(ids)}>
          <Icon name="move" />
          {t("mailMenu.move")}
          <ContextMenuShortcut>V</ContextMenuShortcut>
        </ContextMenuItem>
        <ContextMenuItem variant="destructive" onSelect={() => void trash(ids)}>
          <Icon name="trash" />
          {t("reader.delete")}
          <ContextMenuShortcut>{t("mailMenu.deleteKey")}</ContextMenuShortcut>
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}
