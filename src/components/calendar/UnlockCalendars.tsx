import { Icon } from "@/components/common/Icon";
import { Button } from "@/components/ui/button";
import { useCalendarSync } from "@/hooks/use-calendar-sync";
import { t } from "@/lib/i18n";
import { useSignInAgain } from "@/queries/accounts";

/**
 * For a Microsoft account that signed in before the app asked for its calendars: signs in once
 * more in the browser, allowing them, and syncs.
 */
export function UnlockCalendars({ accountId }: { accountId: string }) {
  const signInAgain = useSignInAgain();
  const { syncCalendars } = useCalendarSync();
  return (
    <Button
      variant="outline"
      size="sm"
      className="justify-self-start"
      disabled={signInAgain.isPending}
      onClick={() =>
        signInAgain.mutate(accountId, { onSuccess: () => void syncCalendars(accountId) })
      }
    >
      <Icon
        name={signInAgain.isPending ? "spinner" : "signIn"}
        className={signInAgain.isPending ? "animate-spin" : undefined}
      />
      {signInAgain.isPending ? t("calendar.unlocking") : t("calendar.unlock")}
    </Button>
  );
}
