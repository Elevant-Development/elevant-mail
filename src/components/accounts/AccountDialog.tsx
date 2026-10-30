import { useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useState } from "react";

import { Field, ServerFields } from "@/components/accounts/ServerFields";
import { Icon } from "@/components/common/Icon";
import { Notice } from "@/components/common/Notice";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { useSyncActions } from "@/hooks/use-sync";
import { describeError, isCancelledSignIn, type UserError } from "@/lib/errors";
import { formatServer } from "@/lib/formatting";
import { t, type MessageKey } from "@/lib/i18n";
import { api, type ImapSettings, type SuggestedSettings } from "@/lib/tauri";
import { useAddAccount } from "@/queries/accounts";
import { useUiStore } from "@/stores/ui";

export function AccountDialog() {
  const open = useUiStore((s) => s.accountDialogOpen);
  const setOpen = useUiStore((s) => s.setAccountDialogOpen);

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="sm:max-w-md">
        {/* Mounted only while open, so every opening starts with an empty form. */}
        {open && <AccountForm onDone={() => setOpen(false)} />}
      </DialogContent>
    </Dialog>
  );
}

function AccountForm({ onDone }: { onDone: () => void }) {
  const queryClient = useQueryClient();
  const selectMailbox = useUiStore((s) => s.selectMailbox);
  const { syncAccount } = useSyncActions();
  const addAccount = useAddAccount();

  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [suggestion, setSuggestion] = useState<SuggestedSettings | null>(null);
  const [settings, setSettings] = useState<ImapSettings | null>(null);
  const [editing, setEditing] = useState(false);
  const [detecting, setDetecting] = useState(false);
  const [error, setError] = useState<UserError | null>(null);

  const connecting = addAccount.isPending;
  // Inputs turn read-only, not disabled, while busy: disabling would drop the keyboard focus.
  const busy = detecting || connecting;

  const changeEmail = (value: string) => {
    setEmail(value);
    // New address, new settings.
    setSuggestion(null);
    setSettings(null);
    setEditing(false);
    setError(null);
  };

  // `quiet`: run when the address field is left, so provider tips (app passwords) show before
  // the password is typed. A failure then waits for the explicit "Weiter".
  const detect = async ({ quiet = false } = {}) => {
    setDetecting(true);
    setError(null);
    try {
      const found = await api.detectSettings(email);
      setSuggestion(found);
      setSettings(found.imap);
      setEditing(found.source === "guess");
    } catch (cause) {
      if (!quiet) setError(describeError(cause));
    } finally {
      setDetecting(false);
    }
  };

  const detectEarly = () => {
    if (!settings && !busy && /^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(email.trim())) {
      void detect({ quiet: true });
    }
  };

  const connect = async () => {
    if (!settings) return;
    setError(null);
    try {
      const account = await addAccount.mutateAsync({
        account: { displayName, emailAddress: email.trim(), imap: settings },
        // Microsoft: no password, the browser sign-in instead.
        password: microsoft ? null : password,
      });
      const folders = await queryClient.fetchQuery({
        queryKey: ["folders"],
        queryFn: api.listFolders,
      });
      const inbox = folders.find((f) => f.accountId === account.id && f.role === "inbox");
      if (inbox) selectMailbox(inbox.id);
      // Fetched now, not left to the background sync noticing the new account.
      void syncAccount(account.id);
      onDone();
    } catch (cause) {
      if (!isCancelledSignIn(cause)) setError(describeError(cause));
    }
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (busy) return;
    void (settings ? connect() : detect());
  };

  const providerTip = suggestion?.provider ? providerTips[suggestion.provider] : undefined;
  const microsoft = suggestion?.needsOauth ?? false;

  return (
    <form onSubmit={submit} className="grid gap-4">
      <DialogHeader>
        <DialogTitle>{t("account.add.title")}</DialogTitle>
        <DialogDescription>{t("account.add.description")}</DialogDescription>
      </DialogHeader>

      <div className="grid gap-3">
        <Field id="account-email" label={t("account.field.email")}>
          <Input
            id="account-email"
            type="email"
            autoComplete="email"
            data-autofocus=""
            required
            value={email}
            readOnly={busy}
            onChange={(event) => changeEmail(event.target.value)}
            onBlur={detectEarly}
          />
        </Field>
        {!microsoft && (
          <Field id="account-password" label={t("account.field.password")}>
            <Input
              id="account-password"
              type="password"
              autoComplete="current-password"
              required
              value={password}
              readOnly={busy}
              onChange={(event) => setPassword(event.target.value)}
            />
          </Field>
        )}
        <Field id="account-name" label={t("account.field.displayName")}>
          <Input
            id="account-name"
            placeholder={t("account.field.displayNamePlaceholder")}
            value={displayName}
            readOnly={busy}
            onChange={(event) => setDisplayName(event.target.value)}
          />
        </Field>
      </div>

      {settings && suggestion && (
        <section className="grid gap-3 rounded-lg border p-3">
          <div className="flex items-start justify-between gap-2">
            <div className="min-w-0">
              <p className="text-xs text-muted-foreground">
                {t(`account.source.${suggestion.source}` as MessageKey)}
              </p>
              <p className="truncate font-medium">{formatServer(settings)}</p>
            </div>
            {!editing && (
              <Button type="button" variant="ghost" size="sm" onClick={() => setEditing(true)}>
                {t("account.settings.edit")}
              </Button>
            )}
          </div>

          {editing && (
            <ServerFields
              idPrefix="account"
              settings={settings}
              readOnly={busy}
              onChange={setSettings}
            />
          )}

          {microsoft ? (
            <Notice
              tone="info"
              text={t(connecting ? "account.microsoft.inBrowser" : "account.tip.microsoft")}
            />
          ) : (
            providerTip && (
              <Notice
                tone="info"
                text={t(providerTip)}
                link={suggestion?.provider ? providerLinks[suggestion.provider] : undefined}
              />
            )
          )}
        </section>
      )}

      {error && <Notice tone="error" text={error.message} detail={error.action} />}

      <DialogFooter>
        {connecting && microsoft ? (
          // Waiting for the browser: cancelling ends the wait, not the dialog.
          <Button type="button" variant="ghost" onClick={() => void api.cancelSignIn()}>
            {t("account.microsoft.cancel")}
          </Button>
        ) : (
          <Button type="button" variant="ghost" onClick={onDone} disabled={connecting}>
            {t("account.add.cancel")}
          </Button>
        )}
        <Button type="submit" disabled={busy || !email || (!microsoft && !password)}>
          {busy && <Icon name="spinner" className="animate-spin" />}
          {connecting
            ? microsoft
              ? t("account.microsoft.waiting")
              : t("account.add.connecting")
            : detecting
              ? t("account.add.detecting")
              : microsoft
                ? t("account.microsoft.signIn")
                : settings
                  ? t("account.add.connect")
                  : t("account.add.next")}
        </Button>
      </DialogFooter>
    </form>
  );
}

const providerTips: Partial<Record<NonNullable<SuggestedSettings["provider"]>, MessageKey>> = {
  gmail: "account.tip.gmail",
  icloud: "account.tip.icloud",
  yahoo: "account.tip.yahoo",
  webDe: "account.tip.webDe",
  gmx: "account.tip.gmx",
};

/** Where the provider's app passwords are made, opened in the system browser. */
const providerLinks: Partial<Record<NonNullable<SuggestedSettings["provider"]>, string>> = {
  gmail: "https://myaccount.google.com/apppasswords",
  icloud: "https://account.apple.com/",
  yahoo: "https://login.yahoo.com/account/security",
};
