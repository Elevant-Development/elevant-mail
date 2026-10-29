/**
 * UI strings go through keys, one file per language in src/locales. The language follows the
 * system unless the user picks one ("appearance" menu); changing it reloads the interface.
 * Another language is a new file with the same keys plus an entry in `languages`.
 */
import { de } from "@/locales/de";
import { en } from "@/locales/en";

export type MessageKey = keyof typeof de;

export const languages = {
  de: { name: "Deutsch", locale: "de-DE", messages: de },
  en: { name: "English", locale: "en-US", messages: en },
} satisfies Record<string, { name: string; locale: string; messages: Record<MessageKey, string> }>;

export type Language = keyof typeof languages;
export type LanguageSetting = Language | "system";

/** Where the UI store keeps its persisted settings (stores/ui.ts). */
const SETTINGS_KEY = "elevant-mail-ui";

function storedSetting(): LanguageSetting {
  try {
    const stored = JSON.parse(localStorage.getItem(SETTINGS_KEY) ?? "{}") as {
      state?: { language?: string };
    };
    const value = stored.state?.language;
    return value === "system" || (value !== undefined && value in languages)
      ? (value as LanguageSetting)
      : "system";
  } catch {
    return "system";
  }
}

/** The system's language if there is a translation for it, else English. */
export function resolveLanguage(setting: LanguageSetting, systemLocale: string): Language {
  if (setting !== "system") return setting;
  const base = systemLocale.toLowerCase().split("-")[0] ?? "";
  return base in languages ? (base as Language) : "en";
}

const systemLocale = typeof navigator === "undefined" ? "en-US" : navigator.language;
export const language: Language = resolveLanguage(storedSetting(), systemLocale);

/** For dates and numbers: the system's own variant when it speaks the same language (en-GB). */
export const locale: string = systemLocale.toLowerCase().startsWith(language)
  ? systemLocale
  : languages[language].locale;

const messages: Record<MessageKey, string> = languages[language].messages;

if (typeof document !== "undefined") document.documentElement.lang = language;

/** Looks up a UI string and fills `{name}` placeholders from `vars`. */
export function t(key: MessageKey, vars?: Record<string, string | number>): string {
  const template = messages[key];
  if (!vars) return template;
  return template.replace(/\{(\w+)\}/g, (placeholder, name: string) =>
    name in vars ? String(vars[name]) : placeholder,
  );
}

/** Keys that come as `.one` and `.other`, by their common start. */
type CountedKey = MessageKey extends infer Key
  ? Key extends `${infer Base}.one`
    ? Base
    : never
  : never;

/** `key.one` for exactly one, `key.other` otherwise; both get `{count}`. */
export function tCount(
  key: CountedKey,
  count: number,
  vars?: Record<string, string | number>,
): string {
  return t(`${key}.${count === 1 ? "one" : "other"}` as MessageKey, { count, ...vars });
}
