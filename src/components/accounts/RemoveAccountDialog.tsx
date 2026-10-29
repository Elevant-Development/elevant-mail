import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { describeError } from "@/lib/errors";
import { t } from "@/lib/i18n";
import type { Account } from "@/lib/tauri";
import { UNIFIED_INBOX } from "@/lib/tauri";
import { useRemoveAccount } from "@/queries/accounts";
import { useUiStore } from "@/stores/ui";

interface RemoveAccountDialogProps {
  account: Account | null;
  onClose: () => void;
}

export function RemoveAccountDialog({ account, onClose }: RemoveAccountDialogProps) {
  const removeAccount = useRemoveAccount();
  const selectMailbox = useUiStore((s) => s.selectMailbox);
  const error = removeAccount.error ? describeError(removeAccount.error) : null;

  const remove = async () => {
    if (!account) return;
    await removeAccount.mutateAsync(account.id);
    selectMailbox(UNIFIED_INBOX);
    onClose();
  };

  return (
    <Dialog open={account !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>
            {t("account.remove.title", { name: account?.displayName ?? "" })}
          </DialogTitle>
          <DialogDescription>{t("account.remove.description")}</DialogDescription>
        </DialogHeader>
        {error && (
          <p role="alert" className="text-sm text-destructive">
            {error.message}
          </p>
        )}
        <DialogFooter>
          <Button variant="ghost" onClick={onClose} disabled={removeAccount.isPending}>
            {t("account.add.cancel")}
          </Button>
          <Button
            variant="destructive"
            onClick={() => void remove().catch(() => undefined)}
            disabled={removeAccount.isPending}
          >
            {t("account.remove.confirm")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
