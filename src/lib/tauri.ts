/**
 * The only place the frontend talks to the backend. Every Tauri command gets a typed wrapper
 * here. The data types are generated from Rust (src/lib/bindings, written by `cargo test`).
 *
 * Inside Tauri the real backend answers; in a plain browser (`pnpm dev`) sample data does, so the
 * interface can be worked on without a mail server.
 */
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type { Account } from "./bindings/Account";
import type { Folder } from "./bindings/Folder";
import type { Message } from "./bindings/Message";
import type { MessageSummary } from "./bindings/MessageSummary";
import type { NewAccount } from "./bindings/NewAccount";
import type { SuggestedSettings } from "./bindings/SuggestedSettings";
import type { SyncProgress } from "./bindings/SyncProgress";
import type { SyncReport } from "./bindings/SyncReport";
import { mockBackend } from "./mock";

export type { Account } from "./bindings/Account";
export type { Attachment } from "./bindings/Attachment";
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
export type { SyncProgress } from "./bindings/SyncProgress";
export type { SyncReport } from "./bindings/SyncReport";
export type { SyncStage } from "./bindings/SyncStage";

/** Mailbox id that shows the inboxes of all accounts together (same value in Rust). */
export const UNIFIED_INBOX = "unified-inbox";

/** Where the backend serves mail images (`mail-asset://`; on Windows `http://mail-asset.localhost`). */
export const ASSET_SOURCES = "mail-asset: http://mail-asset.localhost";

export interface Backend {
  listAccounts(): Promise<Account[]>;
  detectSettings(email: string): Promise<SuggestedSettings>;
  /** Logs in once to check the settings, then stores the account and its password. */
  addAccount(account: NewAccount, password: string): Promise<Account>;
  /**
   * Microsoft 365 or Outlook.com: opens Microsoft's sign-in in the system browser and waits for
   * it, then stores the account like `addAccount`.
   */
  addMicrosoftAccount(account: NewAccount): Promise<Account>;
  /** Stops waiting for the browser sign-in. */
  cancelSignIn(): Promise<void>;
  removeAccount(accountId: string): Promise<void>;
  listFolders(): Promise<Folder[]>;
  /** `mailboxId` is a folder id or {@link UNIFIED_INBOX}. Newest first. */
  listMessages(mailboxId: string): Promise<MessageSummary[]>;
  /** `remoteImages`: the user allowed the external images of this mail. */
  getMessage(messageId: string, remoteImages: boolean): Promise<Message | null>;
  /** Fetches the full body of a large message that was cached with headers only. */
  loadMessageBody(messageId: string): Promise<void>;
  /** New, changed and deleted mail in the account's inbox. */
  syncAccount(accountId: string): Promise<SyncReport>;
  syncFolder(folderId: string): Promise<SyncReport>;
  /** The next 100 messages older than the oldest cached one. */
  loadOlder(folderId: string): Promise<SyncReport>;
  /** Full-text search over all cached mail, newest first. */
  searchMessages(query: string): Promise<MessageSummary[]>;
  /**
   * Set (not toggle) read or flagged state. The cache changes at once; returns the accounts
   * whose changes still have to go to the server ({@link Backend.pushActions}).
   */
  setRead(messageIds: string[], read: boolean): Promise<string[]>;
  setFlagged(messageIds: string[], flagged: boolean): Promise<string[]>;
  /** Sends the account's queued changes to the server; returns how many arrived. */
  pushActions(accountId: string): Promise<number>;
  /** Asks where to save (system dialog) and writes the file; false when cancelled. */
  saveAttachment(attachmentId: string): Promise<boolean>;
  /** Opens with the system's program; refused for files that could run code. */
  openAttachment(attachmentId: string): Promise<void>;
  /** Opens a web or mail link from a mail in the system browser or mail program. */
  openLink(url: string): Promise<void>;
  /** Links clicked in a mail frame (see SafeHtmlMail); returns a function that stops listening. */
  onMailLink(handler: (url: string) => void): Promise<() => void>;
  /** Progress of running syncs; returns a function that stops listening. */
  onSyncProgress(handler: (progress: SyncProgress) => void): Promise<() => void>;
}

const tauriBackend: Backend = {
  listAccounts: () => invoke("list_accounts"),
  detectSettings: (email) => invoke("detect_settings", { email }),
  addAccount: (account, password) => invoke("add_account", { account, password }),
  addMicrosoftAccount: (account) => invoke("add_microsoft_account", { account }),
  cancelSignIn: () => invoke("cancel_sign_in"),
  removeAccount: (accountId) => invoke("remove_account", { accountId }),
  listFolders: () => invoke("list_folders"),
  listMessages: (mailboxId) => invoke("list_messages", { mailboxId }),
  getMessage: (messageId, remoteImages) => invoke("get_message", { messageId, remoteImages }),
  loadMessageBody: (messageId) => invoke("load_message_body", { messageId }),
  syncAccount: (accountId) => invoke("sync_account", { accountId }),
  syncFolder: (folderId) => invoke("sync_folder", { folderId }),
  loadOlder: (folderId) => invoke("load_older", { folderId }),
  searchMessages: (query) => invoke("search_messages", { query }),
  setRead: (messageIds, read) => invoke("set_read", { messageIds, read }),
  setFlagged: (messageIds, flagged) => invoke("set_flagged", { messageIds, flagged }),
  pushActions: (accountId) => invoke("push_actions", { accountId }),
  saveAttachment: (attachmentId) => invoke("save_attachment", { attachmentId }),
  openAttachment: (attachmentId) => invoke("open_attachment", { attachmentId }),
  openLink: (url) => invoke("open_link", { url }),
  onMailLink: (handler) => listen<string>("mail-link", (event) => handler(event.payload)),
  onSyncProgress: (handler) =>
    listen<SyncProgress>("sync-progress", (event) => handler(event.payload)),
};

export const usesMockData = !isTauri();

export const api: Backend = usesMockData ? mockBackend : tauriBackend;
