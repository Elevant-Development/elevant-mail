/**
 * Errors from the backend arrive as `{ kind, hint }` (see CommandError in Rust). This turns them
 * into a message and, where there is one, a concrete next step for the user.
 */
import { t, type MessageKey } from "./i18n";
import type { ErrorKind, Hint } from "./tauri";

export interface UserError {
  message: string;
  action: string | null;
  /** What went wrong, so the interface can offer a fitting button; null if not from the backend. */
  kind: ErrorKind | null;
}

/** Errors the account settings fix: a new password, new server settings or a new sign-in. */
const signInKinds: ErrorKind[] = [
  "authenticationFailed",
  "credentialMissing",
  "signInExpired",
  "differentAccount",
];

export function isSignInError(error: UserError | undefined): boolean {
  return error?.kind != null && signInKinds.includes(error.kind);
}

/** The user stopped waiting for the browser sign-in: nothing to report. */
export function isCancelledSignIn(error: unknown): boolean {
  return isCommandError(error) && error.kind === "signInCancelled";
}

function isCommandError(value: unknown): value is { kind: ErrorKind; hint: Hint | null } {
  return typeof value === "object" && value !== null && "kind" in value;
}

export function describeError(error: unknown): UserError {
  if (!isCommandError(error)) {
    return { message: t("error.kind.internal"), action: null, kind: null };
  }
  return {
    message: t(`error.kind.${error.kind}` as MessageKey),
    action: error.hint ? t(`error.hint.${error.hint}` as MessageKey) : null,
    kind: error.kind,
  };
}
