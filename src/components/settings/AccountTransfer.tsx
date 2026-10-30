import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";

import { Field } from "@/components/accounts/ServerFields";
import { Icon } from "@/components/common/Icon";
import { Notice } from "@/components/common/Notice";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { describeError, isCancelledSignIn } from "@/lib/errors";
import { t } from "@/lib/i18n";
import { api, type ImportedAccount, type ImportPreview } from "@/lib/tauri";

/** Shorter passphrases are too easy to guess for a file that may get lost with the stick. */
const MIN_PASSPHRASE = 8;

function errorNotice(error: unknown) {
  const { message, action } = describeError(error);
  return { text: message, detail: action };
}

/** All accounts into one file, for moving them to another computer. */
function ExportAccounts() {
  const [withPasswords, setWithPasswords] = useState(false);
  const [passphrase, setPassphrase] = useState("");
  const [repeat, setRepeat] = useState("");
  const exportAccounts = useMutation({
    mutationFn: () => api.exportAccounts(withPasswords ? passphrase : null),
  });

  const tooShort = withPasswords && passphrase.length < MIN_PASSPHRASE;
  const mismatch = withPasswords && repeat !== "" && repeat !== passphrase;
  const ready = !withPasswords || (!tooShort && repeat === passphrase);

  return (
    <section className="grid gap-4">
      <div className="grid gap-1">
        <h3 className="text-sm font-medium">{t("settings.transfer.export")}</h3>
        <p className="text-sm text-muted-foreground">{t("settings.transfer.exportHint")}</p>
      </div>
      <div className="flex items-start justify-between gap-4">
        <div className="grid gap-0.5">
          <label htmlFor="transfer-passwords" className="text-sm font-medium">
            {t("settings.transfer.withPasswords")}
          </label>
          <p id="transfer-passwords-hint" className="text-sm text-muted-foreground">
            {t("settings.transfer.withPasswordsHint")}
          </p>
        </div>
        <Switch
          id="transfer-passwords"
          aria-describedby="transfer-passwords-hint"
          checked={withPasswords}
          onCheckedChange={(on) => {
            setWithPasswords(on);
            exportAccounts.reset();
          }}
          className="mt-0.5"
        />
      </div>
      {withPasswords && (
        <div className="grid gap-3">
          <Field id="transfer-passphrase" label={t("settings.transfer.passphrase")}>
            <Input
              id="transfer-passphrase"
              type="password"
              autoComplete="new-password"
              value={passphrase}
              onChange={(event) => setPassphrase(event.target.value)}
            />
          </Field>
          <Field id="transfer-passphrase-repeat" label={t("settings.transfer.passphraseRepeat")}>
            <Input
              id="transfer-passphrase-repeat"
              type="password"
              autoComplete="new-password"
              value={repeat}
              aria-invalid={mismatch || undefined}
              onChange={(event) => setRepeat(event.target.value)}
            />
          </Field>
          <p className="text-xs text-muted-foreground">
            {mismatch
              ? t("settings.transfer.passphraseMismatch")
              : t("settings.transfer.passphraseHint", { count: MIN_PASSPHRASE })}
          </p>
        </div>
      )}
      {exportAccounts.error ? (
        <Notice tone="error" {...errorNotice(exportAccounts.error)} />
      ) : (
        exportAccounts.data && <Notice tone="success" text={t("settings.transfer.exported")} />
      )}
      <Button
        variant="outline"
        className="justify-self-start"
        disabled={!ready || exportAccounts.isPending}
        onClick={() => exportAccounts.mutate()}
      >
        <Icon name="download" />
        {t("settings.transfer.exportButton")}
      </Button>
    </section>
  );
}

type RowState = { status: "idle" | "busy" | "added" } & { error?: unknown };

interface RowProps {
  account: ImportedAccount;
  state: RowState;
  password: string;
  onPassword: (password: string) => void;
  onAdd: () => void;
}

function ImportRow({ account, state, password, onPassword, onAdd }: RowProps) {
  const needsPassword = account.signIn === "password" && !account.hasAccess;
  const id = `transfer-password-${account.emailAddress}`;
  const done = account.exists || state.status === "added";

  return (
    <li className="grid gap-2 py-3">
      <div className="flex items-center gap-3">
        <span
          aria-hidden
          className="size-2.5 shrink-0 rounded-full"
          style={{ backgroundColor: account.color }}
        />
        <div className="grid min-w-0 flex-1">
          <span className="truncate text-sm font-medium">{account.displayName}</span>
          <span className="truncate text-xs text-muted-foreground">{account.emailAddress}</span>
        </div>
        {done ? (
          <span className="flex items-center gap-1 text-xs text-muted-foreground">
            <Icon name="check" className="size-3.5" />
            {state.status === "added"
              ? t("settings.transfer.added")
              : t("settings.transfer.exists")}
          </span>
        ) : (
          <Button
            size="sm"
            variant="outline"
            disabled={state.status === "busy" || (needsPassword && password === "")}
            onClick={onAdd}
          >
            {state.status === "busy" && <Icon name="spinner" className="animate-spin" />}
            {account.signIn === "microsoft" && !account.hasAccess
              ? t("settings.transfer.signIn")
              : t("settings.transfer.add")}
          </Button>
        )}
      </div>
      {!done && needsPassword && (
        <Input
          id={id}
          type="password"
          autoComplete="off"
          aria-label={t("settings.transfer.passwordFor", { email: account.emailAddress })}
          placeholder={t("settings.transfer.password")}
          value={password}
          readOnly={state.status === "busy"}
          onChange={(event) => onPassword(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && password !== "") onAdd();
          }}
        />
      )}
      {!done && account.signIn === "microsoft" && !account.hasAccess && (
        <p className="text-xs text-muted-foreground">{t("settings.transfer.microsoftHint")}</p>
      )}
      {state.error !== undefined && !isCancelledSignIn(state.error) && (
        <Notice tone="error" {...errorNotice(state.error)} />
      )}
    </li>
  );
}

/** Accounts from a file made on another computer, added one by one with their settings. */
function ImportAccounts() {
  const queryClient = useQueryClient();
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [unlocked, setUnlocked] = useState(false);
  const [passphrase, setPassphrase] = useState("");
  const [passwords, setPasswords] = useState<Record<string, string>>({});
  const [rows, setRows] = useState<Record<string, RowState>>({});

  // The file's sign-ins leave the backend's memory with this page.
  useEffect(() => () => void api.closeAccountFile(), []);

  const open = useMutation({
    mutationFn: api.openAccountFile,
    onSuccess: (opened) => {
      if (!opened) return;
      setPreview(opened);
      setUnlocked(false);
      setPassphrase("");
      setPasswords({});
      setRows({});
    },
  });
  const unlock = useMutation({
    mutationFn: () => api.unlockAccountFile(passphrase),
    onSuccess: (opened) => {
      setPreview(opened);
      setUnlocked(true);
      setPassphrase("");
    },
  });

  const add = async (account: ImportedAccount) => {
    const key = account.emailAddress;
    setRows((current) => ({ ...current, [key]: { status: "busy" } }));
    try {
      await api.importAccount(key, passwords[key] || null);
      setRows((current) => ({ ...current, [key]: { status: "added" } }));
      await Promise.all(
        ["accounts", "folders", "messages"].map((query) =>
          queryClient.invalidateQueries({ queryKey: [query] }),
        ),
      );
    } catch (error) {
      setRows((current) => ({ ...current, [key]: { status: "idle", error } }));
    }
  };

  const canAdd = (account: ImportedAccount) =>
    !account.exists && rows[account.emailAddress]?.status !== "added";
  // "All" takes those that need nothing more: their sign-in came along.
  const ready = preview?.accounts.filter((a) => canAdd(a) && a.hasAccess) ?? [];
  const addAll = async () => {
    for (const account of ready) await add(account);
  };

  return (
    <section className="grid gap-4">
      <div className="grid gap-1">
        <h3 className="text-sm font-medium">{t("settings.transfer.import")}</h3>
        <p className="text-sm text-muted-foreground">{t("settings.transfer.importHint")}</p>
      </div>
      <Button
        variant="outline"
        className="justify-self-start"
        disabled={open.isPending}
        onClick={() => open.mutate()}
      >
        <Icon name="upload" />
        {t("settings.transfer.openFile")}
      </Button>
      {open.error && <Notice tone="error" {...errorNotice(open.error)} />}

      {preview && preview.sealed && !unlocked && (
        <form
          className="grid gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            if (passphrase !== "") unlock.mutate();
          }}
        >
          <Field id="transfer-unlock" label={t("settings.transfer.passphrase")}>
            <div className="flex gap-2">
              <Input
                id="transfer-unlock"
                type="password"
                autoComplete="off"
                value={passphrase}
                readOnly={unlock.isPending}
                onChange={(event) => setPassphrase(event.target.value)}
              />
              <Button type="submit" disabled={passphrase === "" || unlock.isPending}>
                {t("settings.transfer.unlock")}
              </Button>
            </div>
          </Field>
          <p className="text-xs text-muted-foreground">{t("settings.transfer.unlockHint")}</p>
          {unlock.error && <Notice tone="error" {...errorNotice(unlock.error)} />}
        </form>
      )}

      {preview && (
        <div className="grid gap-2">
          <ul className="divide-y rounded-lg border px-3">
            {preview.accounts.map((account) => (
              <ImportRow
                key={account.emailAddress}
                account={account}
                state={rows[account.emailAddress] ?? { status: "idle" }}
                password={passwords[account.emailAddress] ?? ""}
                onPassword={(password) =>
                  setPasswords((current) => ({ ...current, [account.emailAddress]: password }))
                }
                onAdd={() => void add(account)}
              />
            ))}
          </ul>
          {ready.length > 1 && (
            <Button className="justify-self-end" onClick={() => void addAll()}>
              {t("settings.transfer.addAll", { count: ready.length })}
            </Button>
          )}
        </div>
      )}
    </section>
  );
}

/** Moving accounts between computers: export here, import there. */
export function AccountTransfer() {
  return (
    <div className="grid gap-8">
      <ExportAccounts />
      <ImportAccounts />
    </div>
  );
}
