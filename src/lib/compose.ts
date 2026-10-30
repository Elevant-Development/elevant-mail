/**
 * Starting a reply or a forward from the mail it answers, and turning the editor's document into
 * the plain-text version that goes out next to the HTML.
 */
import type { JSONContent } from "@tiptap/react";

import { displayName, formatFullDate } from "./formatting";
import { locale, t } from "./i18n";
import type { Address, Message } from "./tauri";

export type ComposeMode = "new" | "reply" | "replyAll" | "forward";

const REPLY_PREFIX = /^\s*(re|aw|antw|sv|vs)\s*:/i;
const FORWARD_PREFIX = /^\s*(fwd?|wg|tr)\s*:/i;

/** "Re: …", without stacking "Re: Re:" or adding one to "AW: …". */
export function replySubject(subject: string | null): string {
  const text = subject?.trim() ?? "";
  return REPLY_PREFIX.test(text) ? text : `Re: ${text}`.trim();
}

export function forwardSubject(subject: string | null): string {
  const text = subject?.trim() ?? "";
  return FORWARD_PREFIX.test(text) ? text : `Fwd: ${text}`.trim();
}

/** Each address once, the user's own left out. */
function others(addresses: Address[], own: Set<string>, taken = new Set<string>()): Address[] {
  return addresses.filter((address) => {
    const key = address.address.toLowerCase();
    if (own.has(key) || taken.has(key)) return false;
    taken.add(key);
    return true;
  });
}

/**
 * Who a reply goes to. To the sender, or to where the sender asked replies to go (Reply-To);
 * a reply to the user's own mail goes to its recipients again. Replying to all adds everyone
 * else the mail went to, never the user's own addresses.
 */
export function replyRecipients(
  message: Pick<Message, "from" | "to" | "cc" | "replyTo">,
  all: boolean,
  ownAddresses: Set<string>,
): { to: Address[]; cc: Address[] } {
  const fromSelf = ownAddresses.has(message.from.address.toLowerCase());
  const answered = fromSelf
    ? message.to
    : message.replyTo.length > 0
      ? message.replyTo
      : [message.from];
  const taken = new Set<string>();
  const to = others(answered, ownAddresses, taken);
  if (!all) return { to, cc: [] };
  return {
    to: [...to, ...others(fromSelf ? [] : message.to, ownAddresses, taken)],
    cc: others(message.cc, ownAddresses, taken),
  };
}

function escapeHtml(text: string): string {
  return text
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

/** Paragraphs from plain text: blank lines part them, single line breaks stay. */
function paragraphs(text: string): string {
  return text
    .replace(/\r\n/g, "\n")
    .trim()
    .split(/\n{2,}/)
    .map((block) => `<p>${block.split("\n").map(escapeHtml).join("<br>")}</p>`)
    .join("");
}

function addressList(addresses: Address[]): string {
  return addresses.map((a) => (a.name ? `${a.name} <${a.address}>` : a.address)).join(", ");
}

/**
 * The answered mail below the line to write on and an empty one: "On … Lena wrote:" and its
 * text, quoted.
 */
export function replyQuote(message: Pick<Message, "from" | "dateSent" | "bodyText">): string {
  const said = t("compose.quoteHeader", {
    date: formatFullDate(message.dateSent, locale),
    name: displayName(message.from),
  });
  return `<p></p><p></p><p>${escapeHtml(said)}</p><blockquote>${paragraphs(message.bodyText ?? "")}</blockquote>`;
}

/** The forwarded mail below the line to write on and an empty one: its headers, then its text. */
export function forwardQuote(
  message: Pick<Message, "from" | "to" | "cc" | "dateSent" | "subject" | "bodyText">,
): string {
  const lines = [
    t("compose.forwarded"),
    `${t("compose.field.from")}: ${addressList([message.from])}`,
    `${t("compose.field.date")}: ${formatFullDate(message.dateSent, locale)}`,
    `${t("compose.field.subject")}: ${message.subject ?? ""}`,
    `${t("compose.field.to")}: ${addressList(message.to)}`,
    ...(message.cc.length > 0 ? [`${t("compose.field.cc")}: ${addressList(message.cc)}`] : []),
  ];
  return `<p></p><p></p><p>${lines.map(escapeHtml).join("<br>")}</p>${paragraphs(message.bodyText ?? "")}`;
}

/**
 * The editor's document as plain text: paragraphs apart by a blank line, quotes with "> ", list
 * items with "- " or their number, links with their address after the text.
 */
export function documentText(document: JSONContent): string {
  const inline = (nodes: JSONContent[] = []): string =>
    nodes
      .map((node) => {
        if (node.type === "hardBreak") return "\n";
        const text = node.text ?? "";
        const link = node.marks?.find((mark) => mark.type === "link")?.attrs?.href as
          string | undefined;
        return link && link !== text ? `${text} (${link})` : text;
      })
      .join("");
  const blocks = (nodes: JSONContent[] = []): string[] =>
    nodes.flatMap((node): string[] => {
      switch (node.type) {
        case "paragraph":
        case "heading":
          return [inline(node.content)];
        case "blockquote":
          return [
            blocks(node.content)
              .join("\n\n")
              .split("\n")
              .map((line) => (line ? `> ${line}` : ">"))
              .join("\n"),
          ];
        case "bulletList":
        case "orderedList":
          return [
            (node.content ?? [])
              .map((item, index) => {
                const marker = node.type === "orderedList" ? `${index + 1}.` : "-";
                const [first = "", ...rest] = blocks(item.content).join("\n").split("\n");
                return [`${marker} ${first}`, ...rest.map((line) => `  ${line}`)].join("\n");
              })
              .join("\n"),
          ];
        case "horizontalRule":
          return ["---"];
        case "codeBlock":
          return [inline(node.content)];
        default:
          return blocks(node.content);
      }
    });
  return `${blocks(document.content).join("\n\n").trim()}\n`;
}

/** "Lena <lena@example.org>, jonas@example.org" → addresses; what is not one stays as typed. */
export function parseAddresses(input: string): Address[] {
  return input
    .split(/[,;\n]/)
    .map((part) => part.trim())
    .filter(Boolean)
    .map((part) => {
      const match = /^(.*?)\s*<([^<>\s]+)>$/.exec(part);
      if (match) {
        const name = match[1]?.trim().replace(/^"|"$/g, "") ?? "";
        return { name: name || null, address: match[2] ?? "" };
      }
      return { name: null, address: part };
    });
}

/** Loosely: something@something.something, no spaces. The server decides the rest. */
export function looksLikeAddress(address: string): boolean {
  return /^[^\s@<>]+@[^\s@<>]+\.[^\s@<>]+$/.test(address);
}
