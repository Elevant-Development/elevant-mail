import type { ReactNode } from "react";

import { Icon, type IconName } from "@/components/common/Icon";
import {
  type Appearance,
  contrasts,
  styles,
  themes,
  themeTokens,
  type Theme,
} from "@/design/themes";
import { fonts } from "@/design/fonts";
import { useResolvedMode } from "@/hooks/use-resolved-mode";
import { type LanguageSetting, languages, t } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { type Density, type Mode, useUiStore } from "@/stores/ui";

const modes: { value: Mode; icon: IconName }[] = [
  { value: "dark", icon: "moon" },
  { value: "light", icon: "sun" },
  { value: "system", icon: "monitor" },
];

const densities: Density[] = ["comfortable", "compact"];

interface SegmentedProps<T extends string> {
  label: string;
  value: T;
  options: { value: T; label: string; icon?: ReactNode }[];
  onChange: (value: T) => void;
}

function Segmented<T extends string>({ label, value, options, onChange }: SegmentedProps<T>) {
  return (
    <div>
      <h3 className="mb-2 text-xs font-medium text-muted-foreground">{label}</h3>
      <div
        role="radiogroup"
        aria-label={label}
        className="grid auto-cols-fr grid-flow-col gap-1 rounded-lg bg-muted p-1"
      >
        {options.map((option) => (
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={value === option.value}
            onClick={() => onChange(option.value)}
            className={cn(
              "flex h-7 items-center justify-center gap-1.5 rounded-md text-xs font-medium transition-colors [&_svg]:size-3.5",
              value === option.value
                ? "bg-card text-foreground shadow-sm"
                : "text-muted-foreground hover:text-foreground",
            )}
          >
            {option.icon}
            {option.label}
          </button>
        ))}
      </div>
    </div>
  );
}

/** Miniature of the window: canvas, sidebar with accent, list with selection, reader. */
function ThemePreview({ theme, appearance }: { theme: Theme; appearance: Appearance }) {
  const tokens = themeTokens(theme, appearance);
  return (
    <span
      aria-hidden
      className="flex h-10 w-full gap-1 rounded-lg p-1"
      style={{ backgroundColor: tokens["--canvas"] }}
    >
      <span className="flex w-1/4 flex-col gap-1 pt-1">
        <span className="h-1 w-3/4 rounded-full" style={{ backgroundColor: tokens["--primary"] }} />
        <span className="h-1 w-1/2 rounded-full" style={{ backgroundColor: tokens["--border"] }} />
      </span>
      <span
        className="flex w-1/3 flex-col gap-0.5 rounded-md p-0.5"
        style={{ backgroundColor: tokens["--card"] }}
      >
        <span className="h-1.5 rounded-sm" style={{ backgroundColor: tokens["--selection"] }} />
        <span className="h-1.5 rounded-sm" style={{ backgroundColor: tokens["--muted"] }} />
      </span>
      <span className="flex-1 rounded-md" style={{ backgroundColor: tokens["--card"] }} />
    </span>
  );
}

/** Mode, style, contrast, accent colour, font, density and language; all apply at once. */
export function AppearanceSettings() {
  const mode = useUiStore((s) => s.mode);
  const style = useUiStore((s) => s.style);
  const contrast = useUiStore((s) => s.contrast);
  const themeId = useUiStore((s) => s.themeId);
  const fontId = useUiStore((s) => s.fontId);
  const setMode = useUiStore((s) => s.setMode);
  const setStyle = useUiStore((s) => s.setStyle);
  const setContrast = useUiStore((s) => s.setContrast);
  const setTheme = useUiStore((s) => s.setTheme);
  const setFont = useUiStore((s) => s.setFont);
  const density = useUiStore((s) => s.density);
  const setDensity = useUiStore((s) => s.setDensity);
  const languageSetting = useUiStore((s) => s.language);
  const setLanguage = useUiStore((s) => s.setLanguage);
  const resolvedMode = useResolvedMode();

  return (
    <div className="flex flex-col gap-4">
      <Segmented
        label={t("appearance.mode")}
        value={mode}
        options={modes.map((m) => ({
          value: m.value,
          label: t(`appearance.mode.${m.value}`),
          icon: <Icon name={m.icon} />,
        }))}
        onChange={setMode}
      />
      <Segmented
        label={t("appearance.style")}
        value={style}
        options={styles.map((s) => ({ value: s, label: t(`appearance.style.${s}`) }))}
        onChange={setStyle}
      />
      <Segmented
        label={t("appearance.contrast")}
        value={contrast}
        options={contrasts.map((c) => ({ value: c, label: t(`appearance.contrast.${c}`) }))}
        onChange={setContrast}
      />

      <div>
        <h3 className="mb-2 text-xs font-medium text-muted-foreground">{t("appearance.theme")}</h3>
        <div
          role="radiogroup"
          aria-label={t("appearance.theme")}
          className="grid grid-cols-3 gap-1.5"
        >
          {themes.map((theme) => {
            const selected = theme.id === themeId;
            return (
              <button
                key={theme.id}
                type="button"
                role="radio"
                aria-checked={selected}
                onClick={() => setTheme(theme.id)}
                className={cn(
                  "flex flex-col gap-1 rounded-xl border p-1 text-left transition-colors hover:bg-accent/60",
                  selected && "border-selection-border bg-selection hover:bg-selection",
                )}
              >
                <ThemePreview theme={theme} appearance={{ mode: resolvedMode, style, contrast }} />
                <span className="flex items-center justify-between px-0.5 text-xs font-medium">
                  {t(`theme.${theme.id}`)}
                  {selected && <Icon name="check" className="size-3.5" />}
                </span>
              </button>
            );
          })}
        </div>
      </div>
      <div>
        <h3 className="mb-2 text-xs font-medium text-muted-foreground">{t("appearance.font")}</h3>
        <div
          role="radiogroup"
          aria-label={t("appearance.font")}
          className="grid grid-cols-3 gap-1.5"
        >
          {fonts.map((font) => {
            const selected = font.id === fontId;
            return (
              <button
                key={font.id}
                type="button"
                role="radio"
                aria-checked={selected}
                onClick={() => setFont(font.id)}
                style={{ fontFamily: font.family }}
                className={cn(
                  "flex flex-col items-start gap-1 rounded-xl border p-2 text-left transition-colors hover:bg-accent/60",
                  selected && "border-selection-border bg-selection hover:bg-selection",
                )}
              >
                <span className="text-xl leading-none font-medium">Ag</span>
                <span className="text-xs leading-snug text-muted-foreground">
                  {t("appearance.fontSample")}
                </span>
                <span className="mt-0.5 flex w-full items-center justify-between text-xs font-medium">
                  {font.label}
                  {selected && <Icon name="check" className="size-3.5" />}
                </span>
              </button>
            );
          })}
        </div>
      </div>
      <Segmented
        label={t("appearance.density")}
        value={density}
        options={densities.map((d) => ({ value: d, label: t(`appearance.density.${d}`) }))}
        onChange={setDensity}
      />
      <Segmented<LanguageSetting>
        label={t("appearance.language")}
        value={languageSetting}
        options={[
          { value: "system", label: t("appearance.language.system") },
          // Each language in its own name, so it can be found whatever is set now.
          ...Object.entries(languages).map(([value, { name }]) => ({
            value: value as LanguageSetting,
            label: name,
          })),
        ]}
        onChange={setLanguage}
      />
    </div>
  );
}
