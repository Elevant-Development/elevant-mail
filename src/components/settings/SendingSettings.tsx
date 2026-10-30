import { useQueryClient } from "@tanstack/react-query";
import { EditorContent, useEditor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { type FormEvent, useState } from "react";

import { Field, ServerFields } from "@/components/accounts/ServerFields";
import { Segmented } from "@/components/common/Segmented";
import { EditorToolbar } from "@/components/compose/EditorToolbar";
import { Icon } from "@/components/common/Icon";
import { Notice } from "@/components/common/Notice";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { describeError } from "@/lib/errors";
import { formatServer } from "@/lib/formatting";
import { t } from "@/lib/i18n";
import { type Account, type AccountSettings, api } from "@/lib/tauri";
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

/** The signature, formatted like mail; saved when the editor is left. */
function SignatureEditor({ account, settings }: { account: Account; settings: AccountSettings }) {
  const queryClient = useQueryClient();
  const [saved, setSaved] = useState(false);
  const [mode, setMode] = useState<"editor" | "html">(settings.signatureHtml ? "html" : "editor");
  const [source, setSource] = useState(settings.signatureHtml ? settings.signature : "");

  const store = (html: string, pasted: boolean) =>
    api.setSignature(account.id, html, pasted).then(() => {
      setSaved(true);
      return queryClient.invalidateQueries({ queryKey: ["account-settings", account.id] });
    });

  const editor = useEditor({
    extensions: [
      StarterKit.configure({
        heading: false,
        code: false,
        codeBlock: false,
        horizontalRule: false,
        link: { openOnClick: false, autolink: true, protocols: ["mailto"] },
      }),
    ],
    content: settings.signatureHtml ? "" : settings.signature,
    editorProps: {
      attributes: {
        id: "settings-signature",
        class: "compose-editor min-h-20 px-3 py-2 outline-none",
        "aria-label": t("settings.account.signature"),
        "aria-multiline": "true",
      },
    },
    onUpdate: () => setSaved(false),
    onBlur: ({ editor }) => {
      const html = editor.isEmpty ? "" : editor.getHTML();
      if (html === settings.signature && !settings.signatureHtml) return;
      void store(html, false);
    },
  });

  return (
    <div className="grid gap-1.5">
      <Segmented
        label={t("settings.account.signature")}
        value={mode}
        options={[
          { value: "editor", label: t("settings.account.signatureEditor") },
          { value: "html", label: t("settings.account.signaturePaste") },
        ]}
        onChange={setMode}
      />
      {mode === "editor" ? (
        <div className="overflow-hidden rounded-lg border focus-within:ring-3 focus-within:ring-ring/50">
          {editor && <EditorToolbar editor={editor} />}
          <EditorContent editor={editor} />
        </div>
      ) : (
        <>
          <textarea
            aria-label={t("settings.account.signaturePaste")}
            value={source}
            spellCheck={false}
            placeholder={t("settings.account.signaturePastePlaceholder")}
            onChange={(event) => {
              setSource(event.target.value);
              setSaved(false);
            }}
            onBlur={() => {
              if (source.trim() !== settings.signature || !settings.signatureHtml) {
                void store(source, true);
              }
            }}
            className="min-h-28 rounded-lg border bg-transparent px-3 py-2 font-mono text-xs outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
          />
          {/* What was saved, as the backend cleaned it; never the text being typed. */}
          {settings.signatureHtml && settings.signature && (
            <div className="grid gap-1">
              <p className="text-xs text-muted-foreground">
                {t("settings.account.signaturePreview")}
              </p>
              <div
                className="overflow-x-auto rounded-lg border bg-white p-3 text-sm text-black"
                dangerouslySetInnerHTML={{ __html: settings.signature }}
              />
            </div>
          )}
        </>
      )}
      <p className="text-xs text-muted-foreground">
        {saved
          ? t("settings.account.senderNameSaved")
          : mode === "html"
            ? t("settings.account.signaturePasteHint")
            : t("settings.account.signatureHint")}
      </p>
    </div>
  );
}

/** Settings for writing mail: the sender's name, the signature and the outgoing server. */
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
      <SignatureEditor account={account} settings={settings} />
      <div className="grid gap-1.5">
        <p className="text-sm font-medium">{t("settings.account.smtp")}</p>
        <OutgoingServer account={account} settings={settings} />
      </div>
    </div>
  );
}
