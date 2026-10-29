/**
 * Fonts. Each one is used throughout (interface, subjects and message body), so the app reads as
 * one piece. All are bundled (see the imports in index.css); nothing loads from the network.
 */

export type FontId = "wix" | "inter" | "onest" | "figtree" | "mona" | "geist";

export interface Font {
  id: FontId;
  /** Font name, shown as is in the picker. */
  label: string;
  family: string;
  /** Body size in the reader. */
  readingSize: string;
}

const stack = (name: string) => `"${name}", ui-sans-serif, system-ui, sans-serif`;

export const fonts: Font[] = [
  {
    id: "wix",
    label: "Wix Madefor",
    family: stack("Wix Madefor Text Variable"),
    readingSize: "15px",
  },
  { id: "inter", label: "Inter", family: stack("Inter Variable"), readingSize: "15px" },
  { id: "onest", label: "Onest", family: stack("Onest Variable"), readingSize: "15px" },
  { id: "figtree", label: "Figtree", family: stack("Figtree Variable"), readingSize: "15px" },
  { id: "mona", label: "Mona Sans", family: stack("Mona Sans Variable"), readingSize: "15px" },
  { id: "geist", label: "Geist", family: stack("Geist Variable"), readingSize: "15px" },
];

export const defaultFontId: FontId = "wix";

export function findFont(id: FontId): Font {
  return fonts.find((font) => font.id === id) ?? fonts[0]!;
}

export function fontTokens(font: Font): Record<string, string> {
  return {
    "--font-ui": font.family,
    "--font-heading-family": font.family,
    "--font-reading-family": font.family,
    "--reading-size": font.readingSize,
  };
}
