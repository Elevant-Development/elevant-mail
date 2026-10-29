import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { t } from "@/lib/i18n";

import { LinkAddress } from "./LinkAddress";

export interface DeceptiveLink {
  url: string;
  text: string;
}

interface LinkWarningDialogProps {
  link: DeceptiveLink | null;
  onOpen: (url: string) => void;
  onClose: () => void;
}

/** Asks before opening a link whose text shows a different address than its target. */
export function LinkWarningDialog({ link, onOpen, onClose }: LinkWarningDialogProps) {
  return (
    <Dialog open={link !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t("link.warning.title")}</DialogTitle>
          <DialogDescription>{t("link.warning.description")}</DialogDescription>
        </DialogHeader>
        {link && (
          <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm">
            <dt className="text-muted-foreground">{t("link.warning.shown")}</dt>
            <dd className="min-w-0 break-all">{link.text.trim()}</dd>
            <dt className="text-muted-foreground">{t("link.warning.target")}</dt>
            <dd className="min-w-0 break-all">
              <LinkAddress url={link.url} />
            </dd>
          </dl>
        )}
        <DialogFooter>
          <Button variant="ghost" onClick={onClose} autoFocus>
            {t("link.warning.cancel")}
          </Button>
          <Button variant="destructive" onClick={() => link && onOpen(link.url)}>
            {t("link.warning.open")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
