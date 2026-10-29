/**
 * Appearance tokens. Four independent choices produce every colour in the app:
 *
 * - mode: dark or light
 * - style: "neutral" keeps surfaces black, grey and white and uses colour only as an accent;
 *   "tinted" tints the surfaces and fills selections with the accent
 * - contrast: how close dark surfaces get to black (or light ones to crisp white)
 * - accent: the one colour, with a matching surface tint for the tinted style
 *
 * The layout is the same everywhere; only these tokens change.
 */

export type ThemeId = "coral" | "teal" | "saffron" | "sky" | "raspberry" | "mono";
export type ResolvedMode = "dark" | "light";
export type Style = "neutral" | "tinted";

/** OKLCH lightness (0–1), chroma, hue. */
type Lch = readonly [number, number, number];

export interface Theme {
  id: ThemeId;
  /** Surface tint, used by the tinted style only. */
  tint: { hue: number; chroma: number };
  accent: Record<ResolvedMode, Lch>;
}

export const themes: Theme[] = [
  {
    id: "coral",
    tint: { hue: 50, chroma: 0.007 },
    accent: { dark: [0.76, 0.12, 42], light: [0.63, 0.15, 38] },
  },
  {
    // Elevant teal from elevant.dev: #2da89a on dark, #1a7a6d on light, forest #0e1f1c as tint.
    id: "teal",
    tint: { hue: 181.5, chroma: 0.024 },
    accent: { dark: [0.663, 0.106, 183.7], light: [0.523, 0.087, 181.4] },
  },
  {
    id: "saffron",
    tint: { hue: 258, chroma: 0.03 },
    accent: { dark: [0.83, 0.13, 80], light: [0.6, 0.13, 62] },
  },
  {
    id: "sky",
    tint: { hue: 235, chroma: 0.02 },
    accent: { dark: [0.78, 0.1, 235], light: [0.52, 0.12, 245] },
  },
  {
    id: "raspberry",
    tint: { hue: 20, chroma: 0.01 },
    accent: { dark: [0.7, 0.17, 10], light: [0.56, 0.19, 12] },
  },
  {
    // Pure black and white; the "accent" is the text colour.
    id: "mono",
    tint: { hue: 0, chroma: 0 },
    accent: { dark: [0.95, 0, 0], light: [0.22, 0, 0] },
  },
];

export const styles: Style[] = ["neutral", "tinted"];

export const defaultThemeId: ThemeId = "teal";
export const defaultStyle: Style = "neutral";

export function findTheme(id: ThemeId): Theme {
  return themes.find((theme) => theme.id === id) ?? themes[0]!;
}

function oklch([l, c, h]: Lch, alphaPercent?: number): string {
  const value = `${l.toFixed(3)} ${c.toFixed(3)} ${h.toFixed(1)}`;
  return alphaPercent === undefined ? `oklch(${value})` : `oklch(${value} / ${alphaPercent}%)`;
}

/**
 * Surface lightness per contrast level. "soft" is layered, "hard" sits close to black (or crisp
 * white) with clearer edges. `tint` scales the surface tint of the tinted style.
 */
const surfaces = {
  dark: {
    soft: {
      canvas: 0.15,
      background: 0.19,
      card: 0.2,
      row: 0.225,
      rowHover: 0.25,
      popover: 0.23,
      muted: 0.25,
      accent: 0.26,
      foreground: 0.94,
      mutedForeground: 0.67,
      border: 7,
      input: 11,
      tint: 1,
    },
    clear: {
      canvas: 0.115,
      background: 0.145,
      card: 0.155,
      row: 0.18,
      rowHover: 0.205,
      popover: 0.19,
      muted: 0.21,
      accent: 0.22,
      foreground: 0.95,
      mutedForeground: 0.66,
      border: 8,
      input: 12,
      tint: 0.6,
    },
    hard: {
      canvas: 0.075,
      background: 0.1,
      card: 0.105,
      row: 0.135,
      rowHover: 0.16,
      popover: 0.15,
      muted: 0.17,
      accent: 0.18,
      foreground: 0.96,
      mutedForeground: 0.64,
      border: 9,
      input: 13,
      tint: 0.3,
    },
  },
  light: {
    soft: {
      canvas: 0.945,
      background: 0.985,
      card: 1,
      row: 0.978,
      rowHover: 0.958,
      popover: 1,
      muted: 0.955,
      accent: 0.95,
      foreground: 0.23,
      mutedForeground: 0.52,
      border: 0.91,
      input: 0.9,
      tint: 1,
    },
    clear: {
      canvas: 0.93,
      background: 0.985,
      card: 1,
      row: 0.974,
      rowHover: 0.952,
      popover: 1,
      muted: 0.945,
      accent: 0.94,
      foreground: 0.19,
      mutedForeground: 0.48,
      border: 0.89,
      input: 0.88,
      tint: 0.6,
    },
    hard: {
      canvas: 0.91,
      background: 0.99,
      card: 1,
      row: 0.97,
      rowHover: 0.945,
      popover: 1,
      muted: 0.935,
      accent: 0.93,
      foreground: 0.15,
      mutedForeground: 0.44,
      border: 0.86,
      input: 0.85,
      tint: 0.3,
    },
  },
} as const;

export type Contrast = keyof (typeof surfaces)["dark"];
export const contrasts: Contrast[] = ["soft", "clear", "hard"];
export const defaultContrast: Contrast = "soft";

export interface Appearance {
  mode: ResolvedMode;
  style?: Style;
  contrast?: Contrast;
}

/** Surfaces, text and accent for a mode; the neutral style then greys out selection and sidebar. */
function baseTokens(theme: Theme, mode: ResolvedMode, contrast: Contrast, chroma: number) {
  const { hue } = theme.tint;
  const accent = theme.accent[mode];
  const [accentL, accentC, accentH] = accent;
  // Light accents carry dark text, dark accents light text.
  const onAccent =
    accentL >= 0.7 ? oklch([0.2, Math.min(accentC, 0.05), accentH]) : oklch([0.99, 0.005, accentH]);

  const shared = {
    "--primary": oklch(accent),
    "--primary-foreground": onAccent,
    "--ring": oklch(accent),
    "--sidebar-primary": oklch(accent),
    "--sidebar-primary-foreground": onAccent,
    "--sidebar-ring": oklch(accent),
    "--unread": oklch(accent),
    "--unread-text": oklch(accent),
  };

  if (mode === "dark") {
    const s = surfaces.dark[contrast];
    const c = chroma * s.tint;
    const neutral = (l: number, tint = c) => oklch([l, tint, hue]);
    const neutralAlpha = (l: number, alpha: number) => oklch([l, c, hue], alpha);
    return {
      ...shared,
      "--canvas": neutral(s.canvas),
      "--background": neutral(s.background),
      "--foreground": neutral(s.foreground),
      "--card": neutral(s.card),
      "--card-foreground": neutral(s.foreground),
      "--row": neutral(s.row),
      "--row-hover": neutral(s.rowHover),
      "--popover": neutral(s.popover),
      "--popover-foreground": neutral(s.foreground),
      "--secondary": neutral(s.muted),
      "--secondary-foreground": neutral(s.foreground),
      "--muted": neutral(s.muted),
      "--muted-foreground": neutral(s.mutedForeground, c * 1.5),
      "--accent": neutral(s.accent),
      "--accent-foreground": neutral(0.95),
      "--destructive": "oklch(0.704 0.191 22.216)",
      "--border": neutralAlpha(0.95, s.border),
      "--input": neutralAlpha(0.95, s.input),
      "--sidebar": neutral(s.canvas),
      "--sidebar-foreground": neutral(0.86),
      "--sidebar-accent": oklch(accent, 14),
      "--sidebar-accent-foreground": oklch([0.86, accentC * 0.8, accentH]),
      "--sidebar-border": neutralAlpha(0.95, s.border - 1),
      "--selection": oklch(accent, 13),
      "--selection-border": oklch(accent, 25),
      "--scrollbar": neutralAlpha(0.95, 14),
      "--shadow-color": "oklch(0 0 0 / 30%)",
      "--avatar-bg-l": "0.38",
      "--avatar-bg-c": "0.08",
      "--avatar-fg-l": "0.9",
      "--avatar-fg-c": "0.07",
    };
  }

  const s = surfaces.light[contrast];
  const c = chroma * s.tint;
  const surfaceChroma = chroma > 0 ? Math.max(c * 0.8, 0.003) : 0;
  const light = (l: number, tint = surfaceChroma) => oklch([l, tint, hue]);
  const text = (l: number) => oklch([l, c * 1.6, hue]);
  return {
    ...shared,
    "--canvas": light(s.canvas),
    "--background": light(s.background, surfaceChroma * 0.5),
    "--foreground": text(s.foreground),
    "--card": light(s.card, 0),
    "--card-foreground": text(s.foreground),
    "--row": light(s.row),
    "--row-hover": light(s.rowHover),
    "--popover": light(s.popover, 0),
    "--popover-foreground": text(s.foreground),
    "--secondary": light(s.muted),
    "--secondary-foreground": text(s.foreground + 0.03),
    "--muted": light(s.muted),
    "--muted-foreground": oklch([s.mutedForeground, c * 2, hue]),
    "--accent": light(s.accent),
    "--accent-foreground": text(s.foreground),
    "--destructive": "oklch(0.577 0.245 27.325)",
    "--border": light(s.border),
    "--input": light(s.input),
    "--sidebar": light(s.canvas),
    "--sidebar-foreground": text(s.foreground + 0.07),
    "--sidebar-accent": oklch(accent, 12),
    "--sidebar-accent-foreground": oklch([0.45, accentC * 0.9, accentH]),
    "--sidebar-border": light(s.border),
    "--selection": oklch(accent, 11),
    "--selection-border": oklch(accent, 25),
    "--scrollbar": oklch([0.3, c * 1.6, hue], 20),
    "--shadow-color": oklch([0.2, c * 3, hue], 8),
    "--avatar-bg-l": "0.85",
    "--avatar-bg-c": "0.08",
    "--avatar-fg-l": "0.4",
    "--avatar-fg-c": "0.1",
  };
}

/**
 * All CSS custom properties for a theme. Unknown contrast or style values (e.g. from old stored
 * settings) fall back to the defaults.
 */
export function themeTokens(
  theme: Theme,
  { mode, style = defaultStyle, contrast = defaultContrast }: Appearance,
): Record<string, string> {
  const level = contrast in surfaces[mode] ? contrast : defaultContrast;

  if (style === "tinted") return baseTokens(theme, mode, level, theme.tint.chroma);

  // Neutral: untinted surfaces; selection, sidebar and unread text in grey. Colour remains for
  // the unread dot, focus ring, logo and primary actions.
  const base = baseTokens(theme, mode, level, 0);
  const ink = mode === "dark" ? "oklch(1 0 0" : "oklch(0 0 0";
  const wash = (percent: number) => `${ink} / ${percent}%)`;
  return {
    ...base,
    "--row": "transparent",
    "--row-hover": wash(mode === "dark" ? 5 : 4),
    "--selection": wash(mode === "dark" ? 9 : 6),
    "--selection-border": wash(mode === "dark" ? 10 : 8),
    "--sidebar-accent": wash(mode === "dark" ? 9 : 6),
    "--sidebar-accent-foreground": base["--foreground"],
    "--sidebar-primary": base["--foreground"],
    "--sidebar-primary-foreground": base["--background"],
    "--unread-text": base["--foreground"],
    "--avatar-bg-c": "0.03",
    "--avatar-fg-c": "0.03",
  };
}
