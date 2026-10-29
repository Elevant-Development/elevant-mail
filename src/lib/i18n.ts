/**
 * UI strings go through keys from the start, so English can follow without touching components.
 * German is the only language for now.
 */
import { de } from "@/locales/de";

export type MessageKey = keyof typeof de;

const messages: Record<MessageKey, string> = de;

export const locale = "de-DE";

/** Looks up a UI string and fills `{name}` placeholders from `vars`. */
export function t(key: MessageKey, vars?: Record<string, string | number>): string {
  const template = messages[key];
  if (!vars) return template;
  return template.replace(/\{(\w+)\}/g, (placeholder, name: string) =>
    name in vars ? String(vars[name]) : placeholder,
  );
}
