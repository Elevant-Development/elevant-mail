import { useEffect, useEffectEvent, useMemo, useRef, useState } from "react";

import { useResolvedMode } from "@/hooks/use-resolved-mode";
import { t } from "@/lib/i18n";
import { api, ASSET_SOURCES } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/stores/ui";

/**
 * - `adapted`: the app's colours and font, for mail without its own design.
 * - `paper`: a light page with the mail's own colours, how designed mail is meant to look.
 * - `inverted`: the light page with colours inverted (images kept), a dark view of designed mail.
 */
export type MailLook = "adapted" | "paper" | "inverted";

interface SafeHtmlMailProps {
  /** Sanitized by the backend; this component never puts it into the app's own DOM. */
  html: string;
  look: MailLook;
  /** A link was clicked: its address and the text it shows. */
  onLink: (url: string, text: string | null) => void;
  /** The address under the mouse pointer, or null. */
  onHoverLink: (url: string | null) => void;
  className?: string;
}

/**
 * The frame: no scripts, forms or navigation of the app. `allow-same-origin` lets the app read
 * the frame (its height, the hovered link); without `allow-scripts` the mail can run nothing.
 * `allow-popups` plus `<base target="_blank">` turns every link click into a new-window request,
 * which the backend refuses and passes on as a `mail-link` event (WebKitGTK does not run the
 * app's own listeners inside a frame without scripts).
 */
const SANDBOX = "allow-same-origin allow-popups";

/** The frame's own policy, on top of the app's: nothing loads except images the app serves. */
const FRAME_CSP = [
  "default-src 'none'",
  "style-src 'unsafe-inline'",
  `img-src ${ASSET_SOURCES} data:`,
  // The app's bundled fonts, for the reading font.
  "font-src 'self'",
].join("; ");

/** Wider mail than this would get unreadably small; it scrolls sideways instead. */
const MIN_ZOOM = 0.6;

const PAPER = { foreground: "#1f1f1f", muted: "#5f6368", border: "#dadce0", link: "#0b57d0" };

function cssVar(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

/** @font-face rules of the app's fonts, so the frame can use the reading font too. */
function fontFaces(): string {
  const rules: string[] = [];
  for (const sheet of Array.from(document.styleSheets)) {
    let cssRules: CSSRuleList;
    try {
      cssRules = sheet.cssRules;
    } catch {
      continue;
    }
    const base = sheet.href ?? document.baseURI;
    for (const rule of Array.from(cssRules)) {
      if (!(rule instanceof CSSFontFaceRule)) continue;
      // Relative font URLs would otherwise resolve against the frame document.
      rules.push(
        rule.cssText.replace(/url\("?([^")]+)"?\)/g, (_, url: string) => {
          return `url("${new URL(url, base).href}")`;
        }),
      );
    }
  }
  return rules.join("\n");
}

/** Undoes the inversion for pictures, so photos and logos keep their colours. */
const INVERT = "invert(1) hue-rotate(180deg)";

function frameDocument(html: string, look: MailLook, mode: "dark" | "light"): string {
  const paper = look !== "adapted";
  const reader = document.querySelector("[data-reader-body]") ?? document.body;
  const font = getComputedStyle(reader).getPropertyValue("--font-reading") || "sans-serif";
  const size = cssVar("--reading-size") || "15px";
  const colors = paper
    ? PAPER
    : {
        foreground: cssVar("--foreground"),
        muted: cssVar("--muted-foreground"),
        border: cssVar("--border"),
        link: cssVar("--primary"),
      };
  // Ours comes first, so a mail's own style sheet wins where it sets something.
  const style = `
${fontFaces()}
:root { color-scheme: ${paper ? "light" : mode}; }
html, body { height: auto !important; min-height: 0 !important; }
html { overflow-x: auto; overflow-y: hidden; }
body {
  margin: 0 !important;
  padding: ${paper ? "24px" : "0"};
  font-family: ${font};
  font-size: ${size};
  line-height: 1.6;
  color: ${colors.foreground};
  /* Inverted, this light grey becomes the dark grey of the reader. */
  background: ${look === "inverted" ? "#e8e8e8" : paper ? "#ffffff" : "transparent"};
  overflow-wrap: break-word;
}
${look === "inverted" ? `html { filter: ${INVERT}; } img, video, picture { filter: ${INVERT}; }` : ""}
a { color: ${colors.link}; }
img { max-width: 100%; height: auto; }
/* Blocked external images: their alt text in a quiet box instead of a broken-image icon. */
img:not([src]) {
  display: inline-block;
  min-width: 1em;
  min-height: 1em;
  font-size: 12px;
  color: ${colors.muted};
  background: color-mix(in srgb, ${colors.muted} 12%, transparent);
}
blockquote {
  margin: 0.5em 0 0.5em 0.25em;
  padding-left: 0.9em;
  border-left: 2px solid ${colors.border};
  color: ${colors.muted};
}
pre { white-space: pre-wrap; }
`;
  return `<!doctype html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="${FRAME_CSP}">
<base target="_blank">
<style>${style}</style></head><body>${html}</body></html>`;
}

/** The link element for an address, to read the text it shows. */
function findLink(doc: Document, url: string): HTMLAnchorElement | null {
  for (const link of Array.from(doc.querySelectorAll<HTMLAnchorElement>("a[href]"))) {
    if (link.href === url) return link;
  }
  return null;
}

export function SafeHtmlMail({ html, look, onLink, onHoverLink, className }: SafeHtmlMailProps) {
  const frameRef = useRef<HTMLIFrameElement>(null);
  const [height, setHeight] = useState(0);
  const mode = useResolvedMode();
  // Any appearance change rebuilds the document with the new colours and font.
  const appearance = useUiStore((s) => [s.themeId, s.contrast, s.style, s.fontId].join("|"));
  const srcDoc = useMemo(
    () => frameDocument(html, look, mode),
    // eslint-disable-next-line react-hooks/exhaustive-deps -- appearance is read from the DOM
    [html, look, mode, appearance],
  );

  const frameDoc = () => frameRef.current?.contentDocument ?? null;
  // The frame's listeners and the hover timer outlive a render; they call the latest props.
  const latest = useRef({ onLink, onHoverLink });
  useEffect(() => {
    latest.current = { onLink, onHoverLink };
  });
  const handleLink = (url: string, text: string | null) => latest.current.onLink(url, text);
  const handleHover = (url: string | null) => latest.current.onHoverLink(url);
  const onBackendLink = useEffectEvent(handleLink);

  // Clicks arrive from the backend (see SANDBOX).
  useEffect(() => {
    const listening = api.onMailLink((url) => {
      const doc = frameDoc();
      onBackendLink(url, doc ? (findLink(doc, url)?.textContent ?? null) : null);
    });
    return () => void listening.then((stop) => stop());
  }, []);

  const resizeObserver = useRef<ResizeObserver | null>(null);
  useEffect(() => () => resizeObserver.current?.disconnect(), []);

  const onLoad = () => {
    const doc = frameDoc();
    if (!doc) return;
    const measure = () => {
      // Newsletters are built for 600 to 700 pixels; a narrower reader shrinks them to fit
      // instead of scrolling sideways.
      const root = doc.documentElement;
      doc.body.style.zoom = "";
      const overflow = root.scrollWidth / Math.max(root.clientWidth, 1);
      if (overflow > 1.01) doc.body.style.zoom = String(Math.max(1 / overflow, MIN_ZOOM));
      setHeight(Math.ceil(root.getBoundingClientRect().height));
    };
    measure();
    resizeObserver.current?.disconnect();
    resizeObserver.current = new ResizeObserver(measure);
    resizeObserver.current.observe(doc.documentElement);
    // Engines that do run the app's listeners in the frame (Chromium) handle clicks here,
    // so no new-window request is made.
    doc.addEventListener("click", (event) => {
      const link = (event.target as Element | null)?.closest?.<HTMLAnchorElement>("a[href]");
      if (!link) return;
      event.preventDefault();
      handleLink(link.href, link.textContent);
    });
  };

  // The hovered link, polled: mouse events inside the frame do not reach the app everywhere.
  const hoverTimer = useRef<number | undefined>(undefined);
  const stopHover = () => {
    window.clearInterval(hoverTimer.current);
    hoverTimer.current = undefined;
    handleHover(null);
  };
  const startHover = () => {
    let last: string | null = null;
    window.clearInterval(hoverTimer.current);
    hoverTimer.current = window.setInterval(() => {
      const link = frameDoc()?.querySelector<HTMLAnchorElement>("a[href]:hover") ?? null;
      const url = link?.href ?? null;
      if (url !== last) {
        last = url;
        handleHover(url);
      }
    }, 120);
  };
  useEffect(() => () => window.clearInterval(hoverTimer.current), []);

  return (
    <iframe
      ref={frameRef}
      title={t("reader.content")}
      sandbox={SANDBOX}
      srcDoc={srcDoc}
      onLoad={onLoad}
      onPointerEnter={startHover}
      onPointerLeave={stopHover}
      referrerPolicy="no-referrer"
      className={cn(
        "block w-full border-0",
        look === "paper" && "rounded-xl bg-white",
        look === "inverted" && "rounded-xl",
        className,
      )}
      style={{ height }}
    />
  );
}
