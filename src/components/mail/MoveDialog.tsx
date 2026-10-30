import { type KeyboardEvent, useState } from "react";

import { Icon } from "@/components/common/Icon";
import { Notice } from "@/components/common/Notice";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { useMessageActions } from "@/hooks/use-message-actions";
import { locale, t } from "@/lib/i18n";
import { moveTargets } from "@/lib/moves";
import { roleIcons } from "@/lib/folders";
import { cn } from "@/lib/utils";
import { useFolders } from "@/queries/folders";
import { useVisibleMessages } from "@/queries/messages";
import { useUiStore } from "@/stores/ui";

/** The folder picker (v): type to narrow the list, arrow keys and Enter to choose. */
export function MoveDialog() {
  const messageIds = useUiStore((s) => s.moveDialogIds);
  const close = useUiStore((s) => s.closeMoveDialog);

  return (
    <Dialog open={messageIds !== null} onOpenChange={(open) => !open && close()}>
      <DialogContent aria-describedby={undefined} className="gap-0 overflow-hidden p-0 sm:max-w-sm">
        {/* Mounted only while open, so every opening starts with an empty search. */}
        {messageIds && <FolderPicker messageIds={messageIds} onDone={close} />}
      </DialogContent>
    </Dialog>
  );
}

function FolderPicker({ messageIds, onDone }: { messageIds: string[]; onDone: () => void }) {
  const { data: messages = [] } = useVisibleMessages();
  const { data: folders = [] } = useFolders();
  const { moveTo } = useMessageActions();
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);

  const chosen = new Set(messageIds);
  const selected = messages.filter((m) => chosen.has(m.id));
  const targets = moveTargets(selected, folders);
  const needle = query.trim().toLocaleLowerCase(locale);
  const shown = (targets ?? []).filter((target) =>
    target.label.toLocaleLowerCase(locale).includes(needle),
  );
  const current = Math.min(active, Math.max(shown.length - 1, 0));

  const choose = (index: number) => {
    const target = shown[index];
    if (!target) return;
    onDone();
    void moveTo(messageIds, target.folder);
  };

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      setActive(Math.min(Math.max(current + step, 0), shown.length - 1));
    } else if (event.key === "Enter") {
      event.preventDefault();
      choose(current);
    }
  };

  return (
    <div className="grid">
      <DialogTitle className="px-4 pt-4 pb-3">{t("move.title")}</DialogTitle>
      {targets === null ? (
        <div className="px-4 pb-4">
          <Notice tone="info" text={t("move.mixedAccounts")} />
        </div>
      ) : (
        <>
          <div className="px-4 pb-3">
            <Input
              data-autofocus=""
              placeholder={t("move.search")}
              aria-label={t("move.search")}
              aria-controls="move-targets"
              aria-activedescendant={
                shown[current] ? `move-${shown[current].folder.id}` : undefined
              }
              value={query}
              onChange={(event) => {
                setQuery(event.target.value);
                setActive(0);
              }}
              onKeyDown={onKeyDown}
            />
          </div>
          <ul
            id="move-targets"
            role="listbox"
            aria-label={t("move.title")}
            className="max-h-80 overflow-y-auto border-t p-1"
          >
            {shown.map((target, index) => (
              <li
                key={target.folder.id}
                id={`move-${target.folder.id}`}
                role="option"
                aria-selected={index === current}
                onMouseEnter={() => setActive(index)}
                onClick={() => choose(index)}
                className={cn(
                  "flex h-8 cursor-default items-center gap-2 rounded-md px-2 text-sm [&_svg]:size-4 [&_svg]:shrink-0 [&_svg]:text-muted-foreground",
                  index === current && "bg-accent text-accent-foreground",
                )}
              >
                <Icon name={target.folder.role ? roleIcons[target.folder.role] : "folder"} />
                <span className="truncate">{target.label}</span>
              </li>
            ))}
            {shown.length === 0 && (
              <li className="px-2 py-3 text-sm text-muted-foreground">{t("move.empty")}</li>
            )}
          </ul>
        </>
      )}
    </div>
  );
}
