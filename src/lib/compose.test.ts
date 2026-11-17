import { describe, expect, it } from "vitest";

import {
  documentText,
  forwardSubject,
  looksLikeAddress,
  parseAddresses,
  replyQuote,
  replyRecipients,
  replySubject,
  signatureIndex,
  signsByItself,
  withSignature,
} from "./compose";

const lena = { name: "Lena Fischer", address: "lena@example.org" };
const jonas = { name: "Jonas", address: "jonas@example.org" };
const ben = { name: null, address: "ben@example.org" };
const me = { name: "Alex", address: "alex@example.org" };
const own = new Set(["alex@example.org"]);

describe("subjects", () => {
  it("adds Re: and Fwd: once", () => {
    expect(replySubject("Wanderung")).toBe("Re: Wanderung");
    expect(replySubject("Re: Wanderung")).toBe("Re: Wanderung");
    expect(replySubject("AW: Wanderung")).toBe("AW: Wanderung");
    expect(replySubject(null)).toBe("Re:");
    expect(forwardSubject("Rechnung")).toBe("Fwd: Rechnung");
    expect(forwardSubject("WG: Rechnung")).toBe("WG: Rechnung");
  });
});

describe("recipients of a reply", () => {
  const mail = { from: lena, to: [me, jonas], cc: [ben], replyTo: [] };

  it("answers the sender, or everyone but the user", () => {
    expect(replyRecipients(mail, false, own)).toEqual({ to: [lena], cc: [] });
    expect(replyRecipients(mail, true, own)).toEqual({ to: [lena, jonas], cc: [ben] });
  });

  it("follows Reply-To", () => {
    const list = { name: "Liste", address: "liste@example.org" };
    expect(replyRecipients({ ...mail, replyTo: [list] }, false, own).to).toEqual([list]);
  });

  it("answers the user's own mail to its recipients again", () => {
    const sent = { from: me, to: [lena], cc: [], replyTo: [] };
    expect(replyRecipients(sent, false, own)).toEqual({ to: [lena], cc: [] });
  });

  it("names everyone once", () => {
    const twice = { from: lena, to: [lena, me], cc: [jonas, jonas], replyTo: [] };
    expect(replyRecipients(twice, true, own)).toEqual({ to: [lena], cc: [jonas] });
  });
});

describe("quotes", () => {
  it("quotes the text safely, below the line to write on and an empty one", () => {
    const html = replyQuote({
      from: lena,
      dateSent: Date.UTC(2026, 9, 29, 14, 40),
      bodyText: "Hallo <Alex>,\nwie geht's?\n\nLena",
    });
    expect(html.startsWith("<p></p><p></p><p>Am ")).toBe(true);
    expect(html).toContain("schrieb Lena Fischer:");
    expect(html).toContain(
      "<blockquote><p>Hallo &lt;Alex&gt;,<br>wie geht's?</p><p>Lena</p></blockquote>",
    );
  });
});

describe("plain text of the editor's document", () => {
  it("keeps paragraphs, quotes, lists and links readable", () => {
    const text = documentText({
      type: "doc",
      content: [
        {
          type: "paragraph",
          content: [
            { type: "text", text: "Siehe " },
            {
              type: "text",
              text: "Karte",
              marks: [{ type: "link", attrs: { href: "https://k.example" } }],
            },
          ],
        },
        {
          type: "bulletList",
          content: [
            {
              type: "listItem",
              content: [{ type: "paragraph", content: [{ type: "text", text: "Brot" }] }],
            },
            {
              type: "listItem",
              content: [{ type: "paragraph", content: [{ type: "text", text: "Käse" }] }],
            },
          ],
        },
        {
          type: "blockquote",
          content: [
            {
              type: "paragraph",
              content: [
                { type: "text", text: "Zeile 1" },
                { type: "hardBreak" },
                { type: "text", text: "Zeile 2" },
              ],
            },
            { type: "paragraph", content: [{ type: "text", text: "Absatz" }] },
          ],
        },
      ],
    });
    expect(text).toBe(
      "Siehe Karte (https://k.example)\n\n- Brot\n- Käse\n\n> Zeile 1\n> Zeile 2\n>\n> Absatz\n",
    );
  });
});

describe("typed addresses", () => {
  it("reads names, several at once, and tells addresses from the rest", () => {
    expect(parseAddresses('"Lena Fischer" <lena@example.org>, jonas@example.org; x')).toEqual([
      lena,
      { name: null, address: "jonas@example.org" },
      { name: null, address: "x" },
    ]);
    expect(looksLikeAddress("lena@example.org")).toBe(true);
    expect(looksLikeAddress("lena@example")).toBe(false);
    expect(looksLikeAddress("lena fischer@example.org")).toBe(false);
  });
});

describe("signatures", () => {
  it("go below the line to write on, above a quote", () => {
    expect(withSignature("", "<p>Alex</p>")).toBe(
      '<p></p><div data-signature=""><p>Alex</p></div>',
    );
    expect(withSignature("<p></p><p></p><p>Am …</p>", "<p>Alex</p>")).toBe(
      '<p></p><div data-signature=""><p>Alex</p></div><p></p><p>Am …</p>',
    );
    expect(withSignature("<p></p><p>Zitat</p>", "")).toBe("<p></p><p>Zitat</p>");
  });

  it("go below the quote at the end when set so", () => {
    expect(withSignature("<p></p><p></p><p>Am …</p>", "<p>Alex</p>", false, true)).toBe(
      '<p></p><p></p><p>Am …</p><p></p><div data-signature=""><p>Alex</p></div>',
    );
    // Without a quote it is the same as above.
    expect(withSignature("", "<p>Alex</p>", false, true)).toBe(
      '<p></p><div data-signature=""><p>Alex</p></div>',
    );
  });

  it("come by themselves into the kinds of mail the settings name", () => {
    const use = {
      newMail: true,
      replies: false,
      forwards: true,
      agentDrafts: false,
      belowQuote: false,
    };
    expect(signsByItself(use, "new")).toBe(true);
    expect(signsByItself(use, "reply")).toBe(false);
    expect(signsByItself(use, "replyAll")).toBe(false);
    expect(signsByItself(use, "forward")).toBe(true);
  });

  it("are inserted below the text written above the quote", () => {
    const text = { empty: false, signature: false, quote: false };
    const empty = { empty: true, signature: false, quote: false };
    const quote = { empty: false, signature: false, quote: true };
    const signature = { empty: false, signature: true, quote: false };
    // A draft an agent wrote: below its last line, before trailing empty ones.
    expect(signatureIndex([text, empty, text, empty])).toBe(3);
    // A reply: above the attribution line, below what was written.
    expect(signatureIndex([text, text, empty, empty, quote, quote])).toBe(2);
    expect(signatureIndex([text, text, empty, empty, quote, quote], true)).toBe(6);
    // Nothing written yet: below the first, empty line.
    expect(signatureIndex([empty, empty, quote])).toBe(1);
    expect(signatureIndex([])).toBe(0);
    // A signature there is no text to go below.
    expect(signatureIndex([text, signature, empty])).toBe(1);
  });
});
