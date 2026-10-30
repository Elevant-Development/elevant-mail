import { mergeAttributes, Node } from "@tiptap/react";

/**
 * The signature as a block of its own in the composer (`<div data-signature>`), so switching the
 * sending account can find and swap it. Written like any other text.
 */
export const Signature = Node.create({
  name: "signature",
  group: "block",
  content: "block+",
  defining: true,
  parseHTML: () => [{ tag: "div[data-signature]:not([data-signature-html])" }],
  renderHTML: ({ HTMLAttributes }) => [
    "div",
    mergeAttributes(HTMLAttributes, { "data-signature": "", class: "signature" }),
    0,
  ],
});

/** Plain text of pasted HTML, for the mail's text version: lines at blocks, cells apart. */
function htmlText(html: string): string {
  const document = new DOMParser().parseFromString(
    html
      .replace(/<br\s*\/?>/gi, "\n")
      .replace(/<\/(p|div|tr|h\d|li)>/gi, "$&\n")
      .replace(/<\/td>/gi, " $&"),
    "text/html",
  );
  return (document.body.textContent ?? "")
    .replace(/[ \t]+\n/g, "\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

/**
 * A signature pasted as HTML (logo, table, colours): kept whole as one block the editor does not
 * open, since it would drop what it does not know. The backend cleaned it when it was saved.
 */
export const HtmlSignature = Node.create({
  name: "htmlSignature",
  group: "block",
  atom: true,
  selectable: true,
  addAttributes: () => ({
    html: { default: "" },
    text: { default: "" },
  }),
  parseHTML: () => [
    {
      tag: "div[data-signature-html]",
      getAttrs: (element) => ({ html: element.innerHTML, text: htmlText(element.innerHTML) }),
    },
  ],
  renderHTML: ({ node }) => {
    const block = document.createElement("div");
    block.setAttribute("data-signature", "");
    block.setAttribute("data-signature-html", "");
    block.className = "signature";
    block.innerHTML = node.attrs.html as string;
    return block;
  },
});
