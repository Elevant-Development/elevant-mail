import { Icon } from "@/components/common/Icon";
import { Button } from "@/components/ui/button";
import { t } from "@/lib/i18n";
import { useSignInAgain } from "@/queries/accounts";

/**
 * For a Microsoft account that signed in before the app asked for its calendars: signs in once
 * more in the browser, allowing them; the sign-in syncs them (useSignInAgain).
 */
export function UnlockCalendars({ accountId }: { accountId: string }) {
  const signInAgain = useSignInAgain();
  return (
    <Button
      variant="outline"
      size="sm"
      className="justify-self-start"
      disabled={signInAgain.isPending}
      onClick={() => signInAgain.mutate(accountId)}
    >
      <Icon
        name={signInAgain.isPending ? "spinner" : "signIn"}
        className={signInAgain.isPending ? "animate-spin" : undefined}
      />
      {signInAgain.isPending ? t("calendar.unlocking") : t("calendar.unlock")}
    </Button>
  );
}
