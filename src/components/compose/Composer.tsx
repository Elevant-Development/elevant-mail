import { EditorContent, useEditor } from "@tiptap/react";
import { Placeholder } from "@tiptap/extensions";
import StarterKit from "@tiptap/starter-kit";
import { type KeyboardEvent, useEffect, useState } from "react";

import { Icon } from "@/components/common/Icon";
import { IconButton } from "@/components/common/IconButton";
import { Notice } from "@/components/common/Notice";
import { Button } from "@/components/ui/button";
import { documentText } from "@/lib/compose";
import { describeError, type UserError } from "@/lib/errors";
import { t } from "@/lib/i18n";
import { api } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useAccounts } from "@/queries/accounts";
import { type Draft, useUiStore } from "@/stores/ui";
import { useUndoStore } from "@/stores/undo";

import { AddressField } from "./AddressField";
import { EditorToolbar } from "./EditorToolbar";

interface ComposerProps {
  className?: string;
  /** Narrow windows: back to the list; the draft stays. */
  onBack?: () => void;
}

/** The draft the reader column shows; nothing while none is open. */
export function Composer({ className, onBack }: ComposerProps) {
  const draft = useUiStore((s) => (s.composeKey ? s.drafts[s.composeKey] : undefined));
  if (!draft) return null;
  // Keyed: another draft gets its own editor.
  return <DraftEditor key={draft.key} draft={draft} className={className} onBack={onBack} />;
}

function DraftEditor({ draft, className, onBack }: ComposerProps & { draft: Draft }) {
  const { data: accounts = [] } = useAccounts();
  const updateDraft = useUiStore((s) => s.updateDraft);
  const closeCompose = useUiStore((s) => s.closeCompose);
  const discardDraft = useUiStore((s) => s.discardDraft);
  const [showCopies, setShowCopies] = useState(draft.cc.length > 0 || draft.bcc.length > 0);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<UserError | null>(null);
  const update = (changes: Partial<Draft>) => updateDraft(draft.key, changes);
  // Kept on this device a moment after each change; leaving it uploads it (useDraftLeaving).
  const current = useUiStore((s) => s.drafts[draft.key]);
  useEffect(() => {
    if (!current) return;
    const timer = window.setTimeout(() => void api.saveDraft(current), 1500);
    return () => window.clearTimeout(timer);
  }, [current]);

  const editor = useEditor({
    extensions: [
      StarterKit.configure({
        heading: false,
        code: false,
        codeBlock: false,
        horizontalRule: false,
        link: { openOnClick: false, autolink: true, protocols: ["mailto"] },
      }),
      Placeholder.configure({ placeholder: t("compose.placeholder") }),
    ],
    content: draft.html,
    // With recipients (a reply) the text starts above the quote; otherwise "To" comes first.
    autofocus: draft.to.length > 0 ? "start" : false,
    editorProps: {
      attributes: {
        class: "compose-editor min-h-full outline-none",
        "aria-label": t("compose.body"),
        "aria-multiline": "true",
      },
    },
    onCreate: ({ editor }) => update({ text: documentText(editor.getJSON()) }),
    onUpdate: ({ editor }) =>
      update({ html: editor.getHTML(), text: documentText(editor.getJSON()) }),
  });

  const send = async () => {
    if (!editor || sending) return;
    // What the store holds now: an address typed but not turned into a chip has just been.
    const latest = useUiStore.getState().drafts[draft.key] ?? draft;
    setSending(true);
    setError(null);
    try {
      await api.sendMessage(
        {
          accountId: latest.accountId,
          to: latest.to,
          cc: latest.cc,
          bcc: latest.bcc,
          subject: latest.subject,
          html: editor.getHTML(),
          text: documentText(editor.getJSON()),
          inReplyTo: latest.mode === "forward" ? null : latest.sourceId,
        },
        draft.key,
      );
      discardDraft(draft.key);
      useUndoStore.getState().show(t("compose.sent"), null);
    } catch (cause) {
      setError(describeError(cause));
    } finally {
      setSending(false);
    }
  };

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      // After the address field took what was typed (it handles the key first).
      queueMicrotask(() => void send());
    } else if (event.key.toLowerCase() === "k" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      document.querySelector<HTMLElement>("[data-link-button]")?.click();
    }
  };

  const account = accounts.find((a) => a.id === draft.accountId);

  return (
    <section
      aria-label={t(`compose.title.${draft.mode}`)}
      onKeyDown={onKeyDown}
      className={cn("relative flex min-h-0 min-w-0 flex-col", className)}
    >
      <header className="flex h-12 shrink-0 items-center gap-1 border-b pr-3 pl-4">
        {onBack && (
          <IconButton label={t("reader.back")} icon={<Icon name="back" />} onClick={onBack} />
        )}
        <h2 className="truncate font-heading font-semibold tracking-tight">
          {t(`compose.title.${draft.mode}`)}
        </h2>
        <div className="ml-auto flex items-center gap-1">
          <IconButton
            label={t("compose.discard")}
            icon={<Icon name="trash" />}
            onClick={() => {
              discardDraft(draft.key);
              void api.discardDraft(draft.key);
            }}
          />
          <IconButton
            label={t("compose.close")}
            icon={<Icon name="close" />}
            onClick={closeCompose}
          />
        </div>
      </header>

      <div className="shrink-0 divide-y border-b">
        {accounts.length > 1 && (
          <div className="flex min-h-10 items-center gap-2 px-4 py-1.5">
            <label htmlFor="compose-from" className="w-14 shrink-0 text-sm text-muted-foreground">
              {t("compose.field.from")}
            </label>
            <select
              id="compose-from"
              value={draft.accountId}
              onChange={(event) => update({ accountId: event.target.value })}
              className="h-7 min-w-0 flex-1 truncate rounded-md bg-transparent text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
            >
              {accounts.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.displayName} · {a.emailAddress}
                </option>
              ))}
            </select>
          </div>
        )}
        <AddressField
          label={t("compose.field.to")}
          addresses={draft.to}
          onChange={(to) => update({ to })}
          autoFocus={draft.to.length === 0}
          extra={
            !showCopies && (
              <button
                type="button"
                onClick={() => setShowCopies(true)}
                className="mt-1 shrink-0 rounded-md px-1.5 py-0.5 text-xs text-muted-foreground hover:bg-accent hover:text-foreground"
              >
                {t("compose.showCopies")}
              </button>
            )
          }
        />
        {showCopies && (
          <>
            <AddressField
              label={t("compose.field.cc")}
              addresses={draft.cc}
              onChange={(cc) => update({ cc })}
            />
            <AddressField
              label={t("compose.field.bcc")}
              addresses={draft.bcc}
              onChange={(bcc) => update({ bcc })}
            />
          </>
        )}
        <div className="flex min-h-10 items-center gap-2 px-4 py-1.5">
          <label htmlFor="compose-subject" className="w-14 shrink-0 text-sm text-muted-foreground">
            {t("compose.field.subject")}
          </label>
          <input
            id="compose-subject"
            value={draft.subject}
            onChange={(event) => update({ subject: event.target.value })}
            className="h-7 min-w-0 flex-1 bg-transparent text-sm font-medium outline-none"
          />
        </div>
      </div>

      {editor && <EditorToolbar editor={editor} />}
      <div
        className="min-h-0 flex-1 cursor-text overflow-y-auto px-5 py-4"
        onMouseDown={(event) => {
          // A click below the text still writes.
          if (event.target === event.currentTarget) {
            event.preventDefault();
            editor?.commands.focus("end");
          }
        }}
      >
        <EditorContent editor={editor} className="h-full" />
      </div>

      <footer className="grid shrink-0 gap-2 border-t px-4 py-3">
        {error && (
          <Notice tone="error" text={[error.message, error.action].filter(Boolean).join(" ")} />
        )}
        <div className="flex items-center gap-3">
          <Button onClick={() => void send()} disabled={sending || !editor}>
            {sending ? <Icon name="spinner" className="animate-spin" /> : <Icon name="sent" />}
            {sending ? t("compose.sending") : t("compose.send")}
          </Button>
          <span className="text-xs text-muted-foreground">{t("compose.sendHint")}</span>
          {account && (
            <span className="ml-auto truncate text-xs text-muted-foreground">
              {account.emailAddress}
            </span>
          )}
        </div>
      </footer>
    </section>
  );
}
