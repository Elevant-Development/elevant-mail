import type { MessageKey } from "./i18n";

/**
 * Colours that tell accounts apart. New accounts get the first six in turn (`ACCOUNT_COLORS` in
 * crates/core/src/accounts.rs); the settings offer all of them.
 */
export const accountColors: { value: string; name: MessageKey }[] = [
  { value: "oklch(0.72 0.13 250)", name: "account.color.blue" },
  { value: "oklch(0.74 0.14 60)", name: "account.color.orange" },
  { value: "oklch(0.72 0.14 150)", name: "account.color.green" },
  { value: "oklch(0.7 0.15 330)", name: "account.color.pink" },
  { value: "oklch(0.75 0.11 200)", name: "account.color.cyan" },
  { value: "oklch(0.72 0.14 20)", name: "account.color.red" },
  { value: "oklch(0.8 0.13 95)", name: "account.color.yellow" },
  { value: "oklch(0.68 0.14 295)", name: "account.color.violet" },
];
