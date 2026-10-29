/** Checks on links in mail, before they are handed to the system browser. */

const DOMAIN_LIKE =
  /^(?:[a-z][a-z0-9+.-]*:\/\/)?(?:www\.)?((?:[\p{L}\p{N}-]+\.)+\p{L}{2,})(?:[/:?#]\S*)?$/iu;

function hostOf(url: string): string | null {
  try {
    const host = new URL(url).hostname.toLowerCase();
    return host.startsWith("www.") ? host.slice(4) : host;
  } catch {
    return null;
  }
}

/**
 * True when the visible text of a link looks like an address on a different site than the
 * one the link leads to, the classic phishing trick ("paypal.com" leading elsewhere).
 */
export function isDeceptiveLink(href: string, text: string | null): boolean {
  const target = hostOf(href);
  const shown = text?.trim().toLowerCase().match(DOMAIN_LIKE)?.[1];
  if (!target || !shown) return false;
  const shownHost = shown.startsWith("www.") ? shown.slice(4) : shown;
  // A subdomain of the shown site is fine ("example.com" leading to "news.example.com").
  return target !== shownHost && !target.endsWith(`.${shownHost}`);
}

/** Splits a URL for display, so the part that matters (the host) can stand out. */
export function splitUrl(url: string): { before: string; host: string; after: string } {
  try {
    const parsed = new URL(url);
    if (parsed.protocol === "mailto:") return { before: "", host: url, after: "" };
    const before = `${parsed.protocol}//`;
    const after = url.slice(before.length + parsed.host.length);
    return { before, host: parsed.host, after };
  } catch {
    return { before: "", host: url, after: "" };
  }
}
