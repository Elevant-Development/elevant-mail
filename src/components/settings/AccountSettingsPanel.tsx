import { type FormEvent, type ReactNode, useState } from "react";

import { RemoveAccountDialog } from "@/components/accounts/RemoveAccountDialog";
import { Field, ServerFields } from "@/components/accounts/ServerFields";
import { Icon } from "@/components/common/Icon";
import { Notice } from "@/components/common/Notice";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useSyncActions } from "@/hooks/use-sync";
import { accountColors } from "@/lib/account-colors";
import { describeError, isCancelledSignIn, isSignInError } from "@/lib/errors";
import { formatServer } from "@/lib/formatting";
import { t } from "@/lib/i18n";
import { type Account, type AccountSettings, api, type ImapSettings } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import {
  useAccountSettings,
  useChangeLogin,
  useSetKeepDays,
  useSignInAgain,
  useUpdateAccount,
} from "@/queries/accounts";
import { useSyncStore } from "@/stores/sync";

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="grid gap-3 border-t pt-6">
      <h3 className="text-xs font-medium text-muted-foreground">{title}</h3>
      {children}
    </section>
  );
}

/** Everything about one account: name and colour, its sign-in, removing it. */
export function AccountSettingsPanel({ account }: { account: Account }) {
  const { data: settings, error } = useAccountSettings(account.id);
  const syncError = useSyncStore((s) => s.errors[account.id]);
  const [removing, setRemoving] = useState(false);

  return (
    <div className="grid gap-6">
      <header className="flex min-w-0 items-center gap-3">
        <span
          aria-hidden
          className="size-3 shrink-0 rounded-full"
          style={{ backgroundColor: account.color }}
        />
        <div className="min-w-0">
          <h2 className="truncate text-base font-medium">{account.displayName}</h2>
          <p className="truncate text-sm text-muted-foreground">{account.emailAddress}</p>
        </div>
      </header>

      <NameAndColor account={account} />

      <Section title={t("settings.account.signIn")}>
        {/* Arriving from a failed sync: what went wrong, right above what fixes it. */}
        {isSignInError(syncError) && syncError && (
          <Notice
            tone="error"
            text={syncError.message}
            // "Sign in again in the account settings" is said where it already is.
            detail={syncError.kind === "signInExpired" ? null : syncError.action}
          />
        )}
        {settings ? (
          settings.signIn === "microsoft" ? (
            <MicrosoftSignIn account={account} settings={settings} />
          ) : (
            <PasswordLogin account={account} settings={settings} />
          )
        ) : error ? (
          <Notice tone="error" {...errorNotice(error)} />
        ) : (
          <Icon name="spinner" className="size-4 animate-spin text-muted-foreground" />
        )}
      </Section>

      <Section title={t("settings.account.offline")}>
        {settings && <OfflinePeriod account={account} settings={settings} />}
      </Section>

      <Section title={t("settings.account.remove")}>
        <p className="text-sm text-muted-foreground">{t("settings.account.removeHint")}</p>
        <Button
          variant="destructive"
          className="justify-self-start"
          onClick={() => setRemoving(true)}
        >
          <Icon name="trash" />
          {t("settings.account.remove")}
        </Button>
        <RemoveAccountDialog
          account={removing ? account : null}
          onClose={() => setRemoving(false)}
        />
      </Section>
    </div>
  );
}

/** The choices for how far back mail stays on this device; `null` is everything. */
const periods: { days: number | null; label: "month" | "months" | "year" | "all" }[] = [
  { days: 30, label: "month" },
  { days: 90, label: "months" },
  { days: 365, label: "year" },
  { days: null, label: "all" },
];

/** How far back the account keeps mail; a longer period is fetched right away. */
function OfflinePeriod({ account, settings }: { account: Account; settings: AccountSettings }) {
  const setKeepDays = useSetKeepDays();
  const { syncAccount } = useSyncActions();

  const choose = (days: number | null) => {
    if (days === settings.keepDays) return;
    setKeepDays.mutate(
      { accountId: account.id, days },
      { onSuccess: () => void syncAccount(account.id) },
    );
  };

  return (
    <div className="grid gap-2">
      <div
        role="radiogroup"
        aria-label={t("settings.account.offline")}
        aria-describedby="settings-account-offline-hint"
        className="grid auto-cols-fr grid-flow-col gap-1 rounded-lg bg-muted p-1"
      >
        {periods.map(({ days, label }) => (
          <button
            key={label}
            type="button"
            role="radio"
            aria-checked={settings.keepDays === days}
            onClick={() => choose(days)}
            className={cn(
              "flex h-7 items-center justify-center rounded-md text-xs font-medium transition-colors",
              settings.keepDays === days
                ? "bg-card text-foreground shadow-sm"
                : "text-muted-foreground hover:text-foreground",
            )}
          >
            {t(`settings.account.offline.${label}`)}
          </button>
        ))}
      </div>
      <p id="settings-account-offline-hint" className="text-sm text-muted-foreground">
        {t("settings.account.offlineHint")}
      </p>
      {setKeepDays.error && <Notice tone="error" {...errorNotice(setKeepDays.error)} />}
    </div>
  );
}

function errorNotice(error: unknown) {
  const { message, action } = describeError(error);
  return { text: message, detail: action };
}

/** Both apply at once: the name when the field is left, the colour on click. */
function NameAndColor({ account }: { account: Account }) {
  const update = useUpdateAccount();
  const [name, setName] = useState(account.displayName);

  const save = (color: string) => {
    if (name.trim() === account.displayName && color === account.color) return;
    update.mutate(
      { accountId: account.id, changes: { displayName: name, color } },
      // An empty name falls back to the address; show what was stored.
      { onSuccess: (updated) => setName(updated.displayName) },
    );
  };

  return (
    <div className="grid gap-4">
      <Field id="settings-account-name" label={t("account.field.displayName")}>
        <Input
          id="settings-account-name"
          value={name}
          placeholder={account.emailAddress}
          onChange={(event) => setName(event.target.value)}
          onBlur={() => save(account.color)}
          onKeyDown={(event) => {
            if (event.key === "Enter") save(account.color);
          }}
        />
      </Field>
      <div className="grid gap-1.5">
        <span id="settings-account-color" className="text-sm font-medium">
          {t("settings.account.color")}
        </span>
        <div
          role="radiogroup"
          aria-labelledby="settings-account-color"
          className="flex flex-wrap gap-2"
        >
          {accountColors.map((color) => {
            const selected = color.value === account.color;
            return (
              <button
                key={color.value}
                type="button"
                role="radio"
                aria-checked={selected}
                aria-label={t(color.name)}
                title={t(color.name)}
                onClick={() => save(color.value)}
                style={{ backgroundColor: color.value }}
                className={cn(
                  "flex size-7 items-center justify-center rounded-full ring-offset-2 ring-offset-popover transition-shadow outline-none hover:ring-2 hover:ring-foreground/20 focus-visible:ring-2 focus-visible:ring-ring",
                  selected && "ring-2 ring-foreground/70 hover:ring-foreground/70",
                )}
              >
                {selected && <Icon name="check" className="size-3.5 text-black/70" />}
              </button>
            );
          })}
        </div>
      </div>
      {update.error && <Notice tone="error" {...errorNotice(update.error)} />}
    </div>
  );
}

function sameServer(a: ImapSettings, b: ImapSettings) {
  return (
    a.host.trim() === b.host &&
    a.port === b.port &&
    a.security === b.security &&
    a.username.trim() === b.username
  );
}

/** Server settings and password; both only change after a login with them worked. */
function PasswordLogin({ account, settings }: { account: Account; settings: AccountSettings }) {
  const changeLogin = useChangeLogin();
  const { syncAccount } = useSyncActions();
  const [server, setServer] = useState(settings.imap);
  const [password, setPassword] = useState("");
  const [editing, setEditing] = useState(false);
  const [saved, setSaved] = useState(false);

  const busy = changeLogin.isPending;
  const changed = password !== "" || !sameServer(server, settings.imap);

  const reset = () => {
    setServer(settings.imap);
    setPassword("");
    setEditing(false);
    changeLogin.reset();
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (busy || !changed) return;
    setSaved(false);
    changeLogin.mutate(
      { accountId: account.id, imap: server, password: password === "" ? null : password },
      {
        onSuccess: () => {
          setPassword("");
          setEditing(false);
          setSaved(true);
          // Fetched now, so the account shows it works again.
          void syncAccount(account.id);
        },
      },
    );
  };

  return (
    <form onSubmit={submit} className="grid gap-3">
      <div className="flex items-start justify-between gap-2 rounded-lg border p-3">
        {/* What is stored; the fields below hold what is not checked yet. */}
        <div className="min-w-0">
          <p className="truncate font-medium">{formatServer(settings.imap)}</p>
          <p className="truncate text-xs text-muted-foreground">{settings.imap.username}</p>
        </div>
        {!editing && (
          <Button type="button" variant="ghost" size="sm" onClick={() => setEditing(true)}>
            {t("account.settings.edit")}
          </Button>
        )}
      </div>
      {editing && (
        <ServerFields
          idPrefix="settings-account"
          settings={server}
          readOnly={busy}
          onChange={(next) => {
            setServer(next);
            setSaved(false);
          }}
        />
      )}
      <Field id="settings-account-password" label={t("settings.account.passwordNew")}>
        <Input
          id="settings-account-password"
          type="password"
          autoComplete="current-password"
          placeholder={t("settings.account.passwordUnchanged")}
          value={password}
          readOnly={busy}
          onChange={(event) => {
            setPassword(event.target.value);
            setSaved(false);
          }}
        />
      </Field>

      {changeLogin.error ? (
        <Notice tone="error" {...errorNotice(changeLogin.error)} />
      ) : saved ? (
        <Notice tone="success" text={t("settings.account.loginSaved")} />
      ) : (
        changed && (
          <p className="text-xs text-muted-foreground">{t("settings.account.loginHint")}</p>
        )
      )}

      <div className="flex justify-end gap-2">
        {changed && !busy && (
          <Button type="button" variant="ghost" onClick={reset}>
            {t("account.add.cancel")}
          </Button>
        )}
        <Button type="submit" disabled={busy || !changed}>
          {busy && <Icon name="spinner" className="animate-spin" />}
          {busy ? t("settings.account.savingLogin") : t("settings.account.saveLogin")}
        </Button>
      </div>
    </form>
  );
}

/** Microsoft accounts have no password here: they sign in again in the browser. */
function MicrosoftSignIn({ account, settings }: { account: Account; settings: AccountSettings }) {
  const signInAgain = useSignInAgain();
  const { syncAccount } = useSyncActions();
  const busy = signInAgain.isPending;
  const error =
    signInAgain.error && !isCancelledSignIn(signInAgain.error)
      ? errorNotice(signInAgain.error)
      : null;

  const start = () =>
    signInAgain.mutate(account.id, { onSuccess: () => void syncAccount(account.id) });

  return (
    <div className="grid gap-3">
      <div className="grid gap-1 text-sm">
        <p>{t("settings.account.microsoft", { address: settings.imap.username })}</p>
        <p className="text-muted-foreground">{t("settings.account.signInAgainHint")}</p>
      </div>
      {busy && <Notice tone="info" text={t("account.microsoft.inBrowser")} />}
      {error && <Notice tone="error" {...error} />}
      {signInAgain.isSuccess && (
        <Notice tone="success" text={t("settings.account.signedInAgain")} />
      )}
      <div className="flex justify-end gap-2">
        {busy && (
          <Button type="button" variant="ghost" onClick={() => void api.cancelSignIn()}>
            {t("account.microsoft.cancel")}
          </Button>
        )}
        <Button type="button" onClick={start} disabled={busy}>
          <Icon name={busy ? "spinner" : "signIn"} className={cn(busy && "animate-spin")} />
          {busy ? t("account.microsoft.waiting") : t("settings.account.signInAgain")}
        </Button>
      </div>
    </div>
  );
}
