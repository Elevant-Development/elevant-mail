import { type Editor, useEditorState } from "@tiptap/react";
import { type FormEvent, useState } from "react";

import { Icon, type IconName } from "@/components/common/Icon";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { t } from "@/lib/i18n";
import { cn } from "@/lib/utils";

function ToolButton({
  label,
  icon,
  active = false,
  onClick,
}: {
  label: string;
  icon: IconName;
  active?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      aria-pressed={active}
      // Keeps the editor's selection, which a focus change would lose.
      onMouseDown={(event) => event.preventDefault()}
      onClick={onClick}
      className={cn(
        "flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground [&_svg]:size-4",
        active && "bg-accent text-foreground",
      )}
    >
      <Icon name={icon} />
    </button>
  );
}

/** Sets or removes the link on the selection (Ctrl+K opens it too, see Composer). */
function LinkButton({ editor, active }: { editor: Editor; active: boolean }) {
  const [open, setOpen] = useState(false);
  const [href, setHref] = useState("");

  const apply = (event: FormEvent) => {
    event.preventDefault();
    const target = href.trim();
    const chain = editor.chain().focus().extendMarkRange("link");
    if (!target) chain.unsetLink().run();
    else
      chain
        .setLink({ href: /^(https?|mailto):/i.test(target) ? target : `https://${target}` })
        .run();
    setOpen(false);
  };

  return (
    <Popover
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        if (next) setHref((editor.getAttributes("link").href as string | undefined) ?? "");
      }}
    >
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={t("compose.format.link")}
          title={t("compose.format.link")}
          aria-pressed={active}
          data-link-button=""
          className={cn(
            "flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground [&_svg]:size-4",
            active && "bg-accent text-foreground",
          )}
        >
          <Icon name="link" />
        </button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-80 p-2">
        <form onSubmit={apply} className="flex gap-2">
          <Input
            value={href}
            onChange={(event) => setHref(event.target.value)}
            placeholder={t("compose.link.placeholder")}
            aria-label={t("compose.format.link")}
          />
          <Button type="submit" size="sm">
            {href.trim() ? t("compose.link.apply") : t("compose.link.remove")}
          </Button>
        </form>
      </PopoverContent>
    </Popover>
  );
}

/** Bold, italic, underline, lists, quote, link and plain text again. */
export function EditorToolbar({ editor }: { editor: Editor }) {
  const state = useEditorState({
    editor,
    selector: ({ editor }) => ({
      bold: editor.isActive("bold"),
      italic: editor.isActive("italic"),
      underline: editor.isActive("underline"),
      bulletList: editor.isActive("bulletList"),
      orderedList: editor.isActive("orderedList"),
      blockquote: editor.isActive("blockquote"),
      link: editor.isActive("link"),
    }),
  });
  const run = (command: (chain: ReturnType<Editor["chain"]>) => ReturnType<Editor["chain"]>) =>
    command(editor.chain().focus()).run();

  return (
    <div
      role="toolbar"
      aria-label={t("compose.format")}
      className="flex shrink-0 items-center gap-0.5 border-b px-3 py-1"
    >
      <ToolButton
        label={t("compose.format.bold")}
        icon="bold"
        active={state.bold}
        onClick={() => run((c) => c.toggleBold())}
      />
      <ToolButton
        label={t("compose.format.italic")}
        icon="italic"
        active={state.italic}
        onClick={() => run((c) => c.toggleItalic())}
      />
      <ToolButton
        label={t("compose.format.underline")}
        icon="underline"
        active={state.underline}
        onClick={() => run((c) => c.toggleUnderline())}
      />
      <span aria-hidden className="mx-1 h-4 w-px bg-border" />
      <ToolButton
        label={t("compose.format.bulletList")}
        icon="list"
        active={state.bulletList}
        onClick={() => run((c) => c.toggleBulletList())}
      />
      <ToolButton
        label={t("compose.format.orderedList")}
        icon="listOrdered"
        active={state.orderedList}
        onClick={() => run((c) => c.toggleOrderedList())}
      />
      <ToolButton
        label={t("compose.format.quote")}
        icon="quote"
        active={state.blockquote}
        onClick={() => run((c) => c.toggleBlockquote())}
      />
      <LinkButton editor={editor} active={state.link} />
      <span aria-hidden className="mx-1 h-4 w-px bg-border" />
      <ToolButton
        label={t("compose.format.removeFormat")}
        icon="removeFormat"
        onClick={() => run((c) => c.unsetAllMarks().clearNodes())}
      />
    </div>
  );
}
