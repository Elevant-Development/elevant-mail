import { useState } from "react";

import { Icon, type IconName } from "@/components/common/Icon";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { describeError, type UserError } from "@/lib/errors";
import { formatSize } from "@/lib/formatting";
import { locale, t } from "@/lib/i18n";
import { api, type Attachment } from "@/lib/tauri";
import { cn } from "@/lib/utils";

function iconFor(attachment: Attachment): IconName {
  const type = attachment.mimeType;
  if (!attachment.canOpen) return "shieldAlert";
  if (type.startsWith("image/")) return "fileImage";
  if (type === "application/pdf" || type.startsWith("text/")) return "fileText";
  if (/zip|compressed|x-tar|x-7z|rar/.test(type)) return "fileArchive";
  if (/spreadsheet|excel|csv/.test(type)) return "fileSheet";
  return "file";
}

type Busy = { id: string; action: "open" | "save" } | null;

/**
 * The attachments of a mail. A click opens one with the system's program (files that could run
 * code are saved instead), the arrow saves it. Either fetches it from the server the first time.
 */
export function AttachmentList({ attachments }: { attachments: Attachment[] }) {
  const [busy, setBusy] = useState<Busy>(null);
  const [error, setError] = useState<UserError | null>(null);

  const run = async (attachment: Attachment, action: "open" | "save") => {
    setBusy({ id: attachment.id, action });
    setError(null);
    try {
      if (action === "open") await api.openAttachment(attachment.id);
      else await api.saveAttachment(attachment.id);
    } catch (failure) {
      setError(describeError(failure));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="mt-5">
      <ul aria-label={t("attachments.label")} className="flex flex-wrap gap-2">
        {attachments.map((attachment) => {
          const working = busy?.id === attachment.id;
          const primary = attachment.canOpen ? "open" : "save";
          return (
            <li
              key={attachment.id}
              className="flex max-w-full min-w-0 items-stretch overflow-hidden rounded-xl border bg-row text-sm"
            >
              <Tooltip>
                <TooltipTrigger asChild>
                  <button
                    type="button"
                    disabled={busy !== null}
                    onClick={() => void run(attachment, primary)}
                    className="flex min-w-0 items-center gap-2.5 py-2 pr-2 pl-3 text-left transition-colors hover:bg-row-hover focus-visible:-outline-offset-2 disabled:cursor-default"
                  >
                    <Icon
                      name={working && busy?.action === primary ? "spinner" : iconFor(attachment)}
                      className={cn(
                        "size-4 shrink-0",
                        working && busy?.action === primary && "animate-spin",
                        attachment.canOpen ? "text-muted-foreground" : "text-amber-500",
                      )}
                    />
                    <span className="min-w-0">
                      <span className="block max-w-56 truncate font-medium">
                        {attachment.fileName}
                      </span>
                      <span className="block text-xs text-muted-foreground tabular-nums">
                        {formatSize(attachment.size, locale)}
                      </span>
                    </span>
                  </button>
                </TooltipTrigger>
                <TooltipContent>
                  {attachment.canOpen ? t("attachments.open") : t("attachments.onlySave")}
                </TooltipContent>
              </Tooltip>
              <Tooltip>
                <TooltipTrigger asChild>
                  <button
                    type="button"
                    aria-label={t("attachments.save", { name: attachment.fileName })}
                    disabled={busy !== null}
                    onClick={() => void run(attachment, "save")}
                    className="flex items-center border-l px-2.5 text-muted-foreground transition-colors hover:bg-row-hover hover:text-foreground focus-visible:-outline-offset-2 disabled:cursor-default"
                  >
                    <Icon
                      name={working && busy?.action === "save" ? "spinner" : "download"}
                      className={cn("size-4", working && busy?.action === "save" && "animate-spin")}
                    />
                  </button>
                </TooltipTrigger>
                <TooltipContent>{t("attachments.saveHint")}</TooltipContent>
              </Tooltip>
            </li>
          );
        })}
      </ul>
      {error && (
        <p role="alert" className="mt-2 text-xs text-destructive">
          {error.message}
          {error.action && <span className="text-foreground/80"> {error.action}</span>}
        </p>
      )}
    </div>
  );
}
