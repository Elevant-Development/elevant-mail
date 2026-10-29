/**
 * Errors from the backend arrive as `{ kind, hint }` (see CommandError in Rust). This turns them
 * into a message and, where there is one, a concrete next step for the user.
 */
import { t, type MessageKey } from "./i18n";
import type { ErrorKind, Hint } from "./tauri";

export interface UserError {
  message: string;
  action: string | null;
}

function isCommandError(value: unknown): value is { kind: ErrorKind; hint: Hint | null } {
  return typeof value === "object" && value !== null && "kind" in value;
}

export function describeError(error: unknown): UserError {
  if (!isCommandError(error)) {
    return { message: t("error.kind.internal"), action: null };
  }
  return {
    message: t(`error.kind.${error.kind}` as MessageKey),
    action: error.hint ? t(`error.hint.${error.hint}` as MessageKey) : null,
  };
}
