import { type FormEvent, useState } from "react";

import { Field, ServerFields } from "@/components/accounts/ServerFields";
import { Icon } from "@/components/common/Icon";
import { Notice } from "@/components/common/Notice";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { describeError } from "@/lib/errors";
import { formatServer } from "@/lib/formatting";
import { t } from "@/lib/i18n";
import type { Account, AccountSettings } from "@/lib/tauri";
import { useChangeSmtp, useSetSenderName } from "@/queries/accounts";

function errorText(error: unknown): string {
  const described = describeError(error);
  return [described.message, described.action].filter(Boolean).join(" ");
}

/** The name mail goes out under, saved when the field is left. */
function SenderName({ account, settings }: { account: Account; settings: AccountSettings }) {
  const setSenderName = useSetSenderName();
  const [name, setName] = useState(settings.senderName);

  const save = () => {
    if (name.trim() === settings.senderName) return;
    setSenderName.mutate({ accountId: account.id, name });
  };

  return (
    <Field id="settings-sender-name" label={t("settings.account.senderName")}>
      <Input
        id="settings-sender-name"
        value={name}
        placeholder={account.emailAddress}
        onChange={(event) => setName(event.target.value)}
        onBlur={save}
        onKeyDown={(event) => {
          if (event.key === "Enter") save();
        }}
      />
      <p className="text-xs text-muted-foreground">
        {setSenderName.error
          ? errorText(setSenderName.error)
          : setSenderName.isSuccess
            ? t("settings.account.senderNameSaved")
            : t("settings.account.senderNameHint")}
      </p>
    </Field>
  );
}

/**
 * The outgoing server, suggested from the incoming one, and for password accounts a password of
 * its own where it differs. Only stored after a login with it worked.
 */
function OutgoingServer({ account, settings }: { account: Account; settings: AccountSettings }) {
  const changeSmtp = useChangeSmtp();
  const [server, setServer] = useState(settings.smtp);
  const [password, setPassword] = useState("");
  const [editing, setEditing] = useState(false);
  const [saved, setSaved] = useState(false);
  const busy = changeSmtp.isPending;

  const reset = () => {
    setServer(settings.smtp);
    setPassword("");
    setEditing(false);
    changeSmtp.reset();
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (busy) return;
    setSaved(false);
    changeSmtp.mutate(
      { accountId: account.id, smtp: server, password: password === "" ? null : password },
      {
        onSuccess: () => {
          setPassword("");
          setEditing(false);
          setSaved(true);
        },
      },
    );
  };

  return (
    <form onSubmit={submit} className="grid gap-3">
      <div className="flex items-start justify-between gap-2 rounded-lg border p-3">
        <div className="min-w-0">
          <p className="truncate font-medium">{formatServer(settings.smtp)}</p>
          <p className="truncate text-xs text-muted-foreground">{settings.smtp.username}</p>
        </div>
        {!editing && (
          <Button type="button" variant="ghost" size="sm" onClick={() => setEditing(true)}>
            {t("account.settings.edit")}
          </Button>
        )}
      </div>
      {editing && (
        <>
          <ServerFields
            idPrefix="settings-smtp"
            settings={server}
            readOnly={busy}
            outgoing
            onChange={(next) => {
              setServer(next);
              setSaved(false);
            }}
          />
          {settings.signIn === "password" && (
            <Field id="settings-smtp-password" label={t("settings.account.smtpPassword")}>
              <Input
                id="settings-smtp-password"
                type="password"
                autoComplete="current-password"
                placeholder={t("settings.account.smtpPasswordHint")}
                value={password}
                readOnly={busy}
                onChange={(event) => {
                  setPassword(event.target.value);
                  setSaved(false);
                }}
              />
            </Field>
          )}
        </>
      )}

      {changeSmtp.error ? (
        <Notice tone="error" text={errorText(changeSmtp.error)} />
      ) : (
        saved && <Notice tone="success" text={t("settings.account.smtpSaved")} />
      )}

      {editing && (
        <div className="flex justify-end gap-2">
          {!busy && (
            <Button type="button" variant="ghost" onClick={reset}>
              {t("account.add.cancel")}
            </Button>
          )}
          {/* Also unchanged: checks whether sending works. */}
          <Button type="submit" disabled={busy}>
            {busy && <Icon name="spinner" className="animate-spin" />}
            {busy ? t("settings.account.savingSmtp") : t("settings.account.saveSmtp")}
          </Button>
        </div>
      )}
    </form>
  );
}

/** Settings for writing mail: the sender's name and the outgoing server. */
export function SendingSettings({
  account,
  settings,
}: {
  account: Account;
  settings: AccountSettings;
}) {
  return (
    <div className="grid gap-4">
      <SenderName account={account} settings={settings} />
      <div className="grid gap-1.5">
        <p className="text-sm font-medium">{t("settings.account.smtp")}</p>
        <OutgoingServer account={account} settings={settings} />
      </div>
    </div>
  );
}
