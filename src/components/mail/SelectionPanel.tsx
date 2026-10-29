import { Icon } from "@/components/common/Icon";
import { IconButton } from "@/components/common/IconButton";
import { Button } from "@/components/ui/button";
import { useMessageActions } from "@/hooks/use-message-actions";
import { t } from "@/lib/i18n";
import { canArchive, deletesForGood } from "@/lib/moves";
import { cn } from "@/lib/utils";
import { useFolders } from "@/queries/folders";
import { useVisibleMessages } from "@/queries/messages";
import { useUiStore } from "@/stores/ui";

interface SelectionPanelProps {
  className?: string;
  /** Narrow layout: back to the list. */
  onBack?: () => void;
}

/** In place of the reader while several messages are selected: what can be done with them. */
export function SelectionPanel({ className, onBack }: SelectionPanelProps) {
  const selectedIds = useUiStore((s) => s.selectedIds);
  const selectMessage = useUiStore((s) => s.selectMessage);
  const openMoveDialog = useUiStore((s) => s.openMoveDialog);
  const { data: messages = [] } = useVisibleMessages();
  const { data: folders = [] } = useFolders();
  const { archive, trash, setRead, setFlagged } = useMessageActions();

  const selected = messages.filter((m) => selectedIds.includes(m.id));
  const ids = selected.map((m) => m.id);
  const anyUnread = selected.some((m) => !m.isRead);
  const anyUnflagged = selected.some((m) => !m.isFlagged);
  const allForGood = selected.length > 0 && selected.every((m) => deletesForGood(m, folders));

  return (
    <section className={cn("flex min-h-0 min-w-0 flex-col", className)}>
      <div className="flex h-12 shrink-0 items-center gap-1 border-b px-3">
        {onBack && (
          <IconButton label={t("reader.back")} icon={<Icon name="back" />} onClick={onBack} />
        )}
      </div>
      <div className="flex flex-1 flex-col items-center justify-center gap-5 p-6 text-center">
        <Icon name="unified" className="size-8 text-muted-foreground" />
        <div className="grid gap-1">
          <h2 className="text-base font-medium">
            {t("selection.count", { count: selected.length })}
          </h2>
          <p className="max-w-xs text-sm text-muted-foreground">{t("selection.hint")}</p>
        </div>
        <div className="flex max-w-md flex-wrap justify-center gap-2">
          <Button
            variant="outline"
            disabled={!selected.some((m) => canArchive(m, folders))}
            onClick={() => void archive(ids)}
          >
            <Icon name="archive" />
            {t("reader.archive")}
          </Button>
          <Button variant="outline" onClick={() => void trash(ids)}>
            <Icon name="trash" />
            {allForGood ? t("reader.deleteForever") : t("reader.delete")}
          </Button>
          <Button variant="outline" onClick={() => openMoveDialog(ids)}>
            <Icon name="move" />
            {t("reader.move")}
          </Button>
          <Button variant="outline" onClick={() => void setRead(ids, anyUnread)}>
            <Icon name={anyUnread ? "mailOpen" : "mail"} />
            {anyUnread ? t("selection.markRead") : t("selection.markUnread")}
          </Button>
          <Button variant="outline" onClick={() => void setFlagged(ids, anyUnflagged)}>
            <Icon name="star" filled={!anyUnflagged} />
            {anyUnflagged ? t("selection.flag") : t("selection.unflag")}
          </Button>
        </div>
        <Button variant="ghost" size="sm" onClick={() => selectMessage(null)}>
          {t("selection.clear")}
        </Button>
      </div>
    </section>
  );
}
