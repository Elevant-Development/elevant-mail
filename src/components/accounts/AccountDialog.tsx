import { useQueryClient } from "@tanstack/react-query";
import { type FormEvent, type ReactNode, useState } from "react";

import { Icon } from "@/components/common/Icon";
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
import { Label } from "@/components/ui/label";
import { useSyncActions } from "@/hooks/use-sync";
import { describeError, type UserError } from "@/lib/errors";
import { t, type MessageKey } from "@/lib/i18n";
import { api, type ImapSettings, type Security, type SuggestedSettings } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useAddAccount } from "@/queries/accounts";
import { useUiStore } from "@/stores/ui";

/** Standard ports; switching the security moves the port along unless it was changed by hand. */
const defaultPorts: Record<Security, number> = { tls: 993, startTls: 143 };

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

function Field({ id, label, children }: { id: string; label: string; children: ReactNode }) {
  return (
    <div className="grid gap-1.5">
      <Label htmlFor={id}>{label}</Label>
      {children}
    </div>
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

  const updateSettings = (patch: Partial<ImapSettings>) =>
    setSettings((current) => (current ? { ...current, ...patch } : current));

  const changeSecurity = (security: Security) => {
    if (!settings) return;
    const port =
      settings.port === defaultPorts[settings.security] ? defaultPorts[security] : settings.port;
    updateSettings({ security, port });
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
        password,
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
      setError(describeError(cause));
    }
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (busy) return;
    void (settings ? connect() : detect());
  };

  const providerTip = suggestion?.provider ? providerTips[suggestion.provider] : undefined;
  const blocked = suggestion?.needsOauth ?? false;

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
            autoFocus
            required
            value={email}
            readOnly={busy}
            onChange={(event) => changeEmail(event.target.value)}
            onBlur={detectEarly}
          />
        </Field>
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
              <p className="truncate font-medium">
                {settings.host} · {settings.port} · {securityLabel(settings.security)}
              </p>
            </div>
            {!editing && (
              <Button type="button" variant="ghost" size="sm" onClick={() => setEditing(true)}>
                {t("account.settings.edit")}
              </Button>
            )}
          </div>

          {editing && (
            <div className="grid gap-3">
              <div className="grid grid-cols-[1fr_6rem] gap-2">
                <Field id="account-host" label={t("account.field.host")}>
                  <Input
                    id="account-host"
                    required
                    value={settings.host}
                    readOnly={busy}
                    onChange={(event) => updateSettings({ host: event.target.value })}
                  />
                </Field>
                <Field id="account-port" label={t("account.field.port")}>
                  <Input
                    id="account-port"
                    type="number"
                    min={1}
                    max={65535}
                    required
                    value={settings.port}
                    readOnly={busy}
                    onChange={(event) => updateSettings({ port: Number(event.target.value) })}
                  />
                </Field>
              </div>
              <div className="grid gap-1.5">
                <span className="text-sm font-medium">{t("account.field.security")}</span>
                <div role="radiogroup" className="grid grid-cols-2 gap-1 rounded-lg bg-muted p-1">
                  {(["tls", "startTls"] as const).map((security) => (
                    <button
                      key={security}
                      type="button"
                      role="radio"
                      aria-checked={settings.security === security}
                      disabled={busy}
                      onClick={() => changeSecurity(security)}
                      className={cn(
                        "h-7 rounded-md text-xs font-medium transition-colors",
                        settings.security === security
                          ? "bg-card text-foreground shadow-sm"
                          : "text-muted-foreground hover:text-foreground",
                      )}
                    >
                      {securityLabel(security)}
                    </button>
                  ))}
                </div>
              </div>
              <Field id="account-username" label={t("account.field.username")}>
                <Input
                  id="account-username"
                  autoComplete="username"
                  required
                  value={settings.username}
                  readOnly={busy}
                  onChange={(event) => updateSettings({ username: event.target.value })}
                />
              </Field>
            </div>
          )}

          {blocked ? (
            <Notice tone="info" text={t("account.tip.microsoft")} />
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
        <Button type="button" variant="ghost" onClick={onDone} disabled={connecting}>
          {t("account.add.cancel")}
        </Button>
        <Button type="submit" disabled={busy || blocked || !email || !password}>
          {busy && <Icon name="spinner" className="animate-spin" />}
          {connecting
            ? t("account.add.connecting")
            : detecting
              ? t("account.add.detecting")
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

function securityLabel(security: Security) {
  return security === "tls" ? "TLS" : "STARTTLS";
}

/** Where the provider's app passwords are made, opened in the system browser. */
const providerLinks: Partial<Record<NonNullable<SuggestedSettings["provider"]>, string>> = {
  gmail: "https://myaccount.google.com/apppasswords",
  icloud: "https://account.apple.com/",
  yahoo: "https://login.yahoo.com/account/security",
};

function Notice({
  tone,
  text,
  detail,
  link,
}: {
  tone: "info" | "error";
  text: string;
  detail?: string | null;
  link?: string;
}) {
  return (
    <div
      role={tone === "error" ? "alert" : "status"}
      className={cn(
        "flex gap-2 rounded-lg px-3 py-2 text-sm",
        tone === "error" ? "bg-destructive/10 text-destructive" : "bg-muted text-muted-foreground",
      )}
    >
      <Icon name={tone === "error" ? "alert" : "info"} className="mt-0.5 size-4 shrink-0" />
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
