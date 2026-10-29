import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useMessageActions } from "@/hooks/use-message-actions";
import { t, tCount } from "@/lib/i18n";
import { useUiStore } from "@/stores/ui";

/** The one question before deleting for good: from the trash, there is no way back. */
export function DeleteForeverDialog() {
  const messageIds = useUiStore((s) => s.deleteForeverIds);
  const ask = useUiStore((s) => s.askDeleteForever);
  const { deleteNow } = useMessageActions();
  const count = messageIds?.length ?? 0;

  const confirm = () => {
    if (!messageIds) return;
    ask(null);
    void deleteNow(messageIds);
  };

  return (
    <Dialog open={messageIds !== null} onOpenChange={(open) => !open && ask(null)}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{tCount("deleteForever.title", count)}</DialogTitle>
          <DialogDescription>{tCount("deleteForever.description", count)}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="ghost" onClick={() => ask(null)}>
            {t("account.add.cancel")}
          </Button>
          <Button variant="destructive" autoFocus onClick={confirm}>
            {t("reader.deleteForever")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
