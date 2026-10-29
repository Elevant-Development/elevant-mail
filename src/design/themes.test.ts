import { describe, expect, it } from "vitest";

import { contrasts, findTheme, styles, themes, themeTokens } from "./themes";

const lightness = (color: string) => Number(/oklch\(([\d.]+)/.exec(color)?.[1]);
const chroma = (color: string) => Number(/oklch\([\d.]+ ([\d.]+)/.exec(color)?.[1]);

describe("themeTokens", () => {
  it("produces the same token names for every combination", () => {
    const reference = Object.keys(themeTokens(themes[0]!, { mode: "dark" })).sort();
    for (const theme of themes) {
      for (const mode of ["dark", "light"] as const) {
        for (const style of styles) {
          for (const contrast of contrasts) {
            const tokens = themeTokens(theme, { mode, style, contrast });
            expect(Object.keys(tokens).sort()).toEqual(reference);
          }
        }
      }
    }
  });

  it("gets darker with more contrast in dark mode", () => {
    const theme = findTheme("saffron");
    const canvas = contrasts.map((contrast) =>
      lightness(themeTokens(theme, { mode: "dark", contrast })["--canvas"]!),
    );
    expect(canvas[0]).toBeGreaterThan(canvas[1]!);
    expect(canvas[1]).toBeGreaterThan(canvas[2]!);
  });

  it("keeps surfaces untinted in the neutral style and tints them otherwise", () => {
    const theme = findTheme("saffron");
    const neutral = themeTokens(theme, { mode: "dark", style: "neutral" });
    const tinted = themeTokens(theme, { mode: "dark", style: "tinted" });
    expect(chroma(neutral["--card"]!)).toBe(0);
    expect(chroma(tinted["--card"]!)).toBeGreaterThan(0);
    // The accent stays in both.
    expect(neutral["--unread"]).toBe(tinted["--unread"]);
  });

  it("falls back to the defaults for unknown stored values", () => {
    const theme = findTheme("coral");
    // Values from old stored settings are not type-checked.
    const unknown = "unknown" as never;
    expect(themeTokens(theme, { mode: "dark", contrast: unknown, style: unknown })).toEqual(
      themeTokens(theme, { mode: "dark" }),
    );
  });

  it("puts dark text on light accents", () => {
    const mono = themeTokens(findTheme("mono"), { mode: "dark" });
    expect(lightness(mono["--primary-foreground"]!)).toBeLessThan(0.5);
  });
});
