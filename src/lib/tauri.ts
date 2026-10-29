/**
 * The only place the frontend talks to the backend. Every Tauri command gets a typed wrapper
 * here. The data types are generated from Rust (src/lib/bindings, written by `cargo test`).
 *
 * Inside Tauri the real backend answers; in a plain browser (`pnpm dev`) sample data does, so the
 * interface can be worked on without a mail server.
 */
import { invoke, isTauri } from "@tauri-apps/api/core";

import type { Account } from "./bindings/Account";
import type { Folder } from "./bindings/Folder";
import type { Message } from "./bindings/Message";
import type { MessageSummary } from "./bindings/MessageSummary";
import type { NewAccount } from "./bindings/NewAccount";
import type { SuggestedSettings } from "./bindings/SuggestedSettings";
import { mockBackend } from "./mock";

export type { Account } from "./bindings/Account";
export type { Address } from "./bindings/Address";
export type { ErrorKind } from "./bindings/ErrorKind";
export type { Folder } from "./bindings/Folder";
export type { FolderRole } from "./bindings/FolderRole";
export type { Hint } from "./bindings/Hint";
export type { ImapSettings } from "./bindings/ImapSettings";
export type { Message } from "./bindings/Message";
export type { MessageSummary } from "./bindings/MessageSummary";
export type { NewAccount } from "./bindings/NewAccount";
export type { Provider } from "./bindings/Provider";
export type { Security } from "./bindings/Security";
export type { SuggestedSettings } from "./bindings/SuggestedSettings";
export type { SuggestionSource } from "./bindings/SuggestionSource";

/** Mailbox id that shows the inboxes of all accounts together (same value in Rust). */
export const UNIFIED_INBOX = "unified-inbox";

export interface Backend {
  listAccounts(): Promise<Account[]>;
  detectSettings(email: string): Promise<SuggestedSettings>;
  /** Logs in once to check the settings, then stores the account and its password. */
  addAccount(account: NewAccount, password: string): Promise<Account>;
  removeAccount(accountId: string): Promise<void>;
  listFolders(): Promise<Folder[]>;
  /** `mailboxId` is a folder id or {@link UNIFIED_INBOX}. Newest first. */
  listMessages(mailboxId: string): Promise<MessageSummary[]>;
  getMessage(messageId: string): Promise<Message | null>;
}

const tauriBackend: Backend = {
  listAccounts: () => invoke("list_accounts"),
  detectSettings: (email) => invoke("detect_settings", { email }),
  addAccount: (account, password) => invoke("add_account", { account, password }),
  removeAccount: (accountId) => invoke("remove_account", { accountId }),
  listFolders: () => invoke("list_folders"),
  listMessages: (mailboxId) => invoke("list_messages", { mailboxId }),
  getMessage: (messageId) => invoke("get_message", { messageId }),
};

export const usesMockData = !isTauri();

export const api: Backend = usesMockData ? mockBackend : tauriBackend;
