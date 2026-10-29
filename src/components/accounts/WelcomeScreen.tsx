import { ElevantMark } from "@/components/common/ElevantMark";
import { Icon } from "@/components/common/Icon";
import { Button } from "@/components/ui/button";
import { t } from "@/lib/i18n";
import { useUiStore } from "@/stores/ui";

/** First start: no account yet. */
export function WelcomeScreen() {
  const setAccountDialogOpen = useUiStore((s) => s.setAccountDialogOpen);

  return (
    <div className="flex flex-1 items-center justify-center p-6">
      <div className="flex max-w-sm flex-col items-center gap-4 text-center">
        <ElevantMark className="size-14 text-primary" />
        <div className="grid gap-2">
          <h1 className="font-heading text-xl font-semibold tracking-tight">
            {t("welcome.title")}
          </h1>
          <p className="text-pretty text-muted-foreground">{t("welcome.text")}</p>
        </div>
        <Button size="lg" onClick={() => setAccountDialogOpen(true)}>
          <Icon name="plus" />
          {t("sidebar.addAccount")}
        </Button>
      </div>
    </div>
  );
}
