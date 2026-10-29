import { Icon } from "@/components/common/Icon";
import { t } from "@/lib/i18n";
import { api } from "@/lib/tauri";
import { cn } from "@/lib/utils";

interface NoticeProps {
  tone: "info" | "success" | "error";
  text: string;
  /** The concrete next step, below the text. */
  detail?: string | null;
  /** A page that helps (where app passwords are made), opened in the system browser. */
  link?: string;
}

/** A tip, a confirmation or an error inside a form. */
export function Notice({ tone, text, detail, link }: NoticeProps) {
  return (
    <div
      role={tone === "error" ? "alert" : "status"}
      className={cn(
        "flex gap-2 rounded-lg px-3 py-2 text-sm",
        tone === "error" ? "bg-destructive/10 text-destructive" : "bg-muted text-muted-foreground",
        tone === "success" && "text-foreground",
      )}
    >
      <Icon
        name={tone === "error" ? "alert" : tone === "success" ? "check" : "info"}
        className={cn("mt-0.5 size-4 shrink-0", tone === "success" && "text-primary")}
      />
      <div className="grid gap-0.5">
        <p className={cn(tone === "error" && "font-medium")}>{text}</p>
        {detail && <p className="text-foreground/80">{detail}</p>}
        {link && (
          <button
            type="button"
            onClick={() => void api.openLink(link).catch(() => undefined)}
            className="justify-self-start font-medium text-foreground underline-offset-2 hover:underline"
          >
            {t("account.tip.openPage")}
          </button>
        )}
      </div>
    </div>
  );
}
