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
import type { AccountChanges } from "./bindings/AccountChanges";
import type { AccountSettings } from "./bindings/AccountSettings";
import type { Conversation } from "./bindings/Conversation";
import type { Folder } from "./bindings/Folder";
import type { ImapSettings } from "./bindings/ImapSettings";
import type { ListCursor } from "./bindings/ListCursor";
import type { ListFilter } from "./bindings/ListFilter";
import type { ListPage } from "./bindings/ListPage";
import type { Message } from "./bindings/Message";
import type { MessageSummary } from "./bindings/MessageSummary";
import type { MoveOutcome } from "./bindings/MoveOutcome";
import type { NewAccount } from "./bindings/NewAccount";
import type { SuggestedSettings } from "./bindings/SuggestedSettings";
import type { SyncProgress } from "./bindings/SyncProgress";
import type { SyncReport } from "./bindings/SyncReport";
import { mockBackend } from "./mock";

export type { Account } from "./bindings/Account";
export type { AccountChanges } from "./bindings/AccountChanges";
export type { AccountSettings } from "./bindings/AccountSettings";
export type { Attachment } from "./bindings/Attachment";
export type { Conversation } from "./bindings/Conversation";
export type { Address } from "./bindings/Address";
export type { ErrorKind } from "./bindings/ErrorKind";
export type { Folder } from "./bindings/Folder";
export type { FolderRole } from "./bindings/FolderRole";
export type { Hint } from "./bindings/Hint";
export type { ImapSettings } from "./bindings/ImapSettings";
export type { ListCursor } from "./bindings/ListCursor";
export type { ListFilter } from "./bindings/ListFilter";
export type { ListPage } from "./bindings/ListPage";
export type { Message } from "./bindings/Message";
export type { MessageSummary } from "./bindings/MessageSummary";
export type { MoveOutcome } from "./bindings/MoveOutcome";
export type { NewAccount } from "./bindings/NewAccount";
export type { NewMail } from "./bindings/NewMail";
export type { Provider } from "./bindings/Provider";
export type { Security } from "./bindings/Security";
export type { SignInMethod } from "./bindings/SignInMethod";
export type { SuggestedSettings } from "./bindings/SuggestedSettings";
export type { SuggestionSource } from "./bindings/SuggestionSource";
export type { SyncProgress } from "./bindings/SyncProgress";
export type { SyncReport } from "./bindings/SyncReport";
export type { SyncStage } from "./bindings/SyncStage";

/** Mailbox id that shows the inboxes of all accounts together (same value in Rust). */
export const UNIFIED_INBOX = "unified-inbox";

/** How long a move can be undone before it goes to the server (`UNDO_WINDOW_MS` in Rust). */
export const UNDO_WINDOW_MS = 10_000;

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
  getAccountSettings(accountId: string): Promise<AccountSettings>;
  /**
   * How many days back the account keeps mail (`null`: everything). Every folder keeps at least
   * its newest 100; the next sync fetches what a longer period adds.
   */
  setKeepDays(accountId: string, days: number | null): Promise<void>;
  /** Name and colour; they change without a login. */
  updateAccount(accountId: string, changes: AccountChanges): Promise<Account>;
  /**
   * New server settings or a new password (`null` keeps the stored one). Logs in with them first
   * and changes nothing if that fails.
   */
  changeLogin(accountId: string, imap: ImapSettings, password: string | null): Promise<void>;
  /** Microsoft accounts: the browser sign-in again, for the account's own address. */
  signInAgain(accountId: string): Promise<void>;
  listFolders(): Promise<Folder[]>;
  /**
   * A page of the list. `mailboxId` is a folder id or {@link UNIFIED_INBOX}. Newest first; with
   * `threads` one row per conversation, otherwise per message. `filters` narrow it (a
   * conversation matches while one of its messages does), except for the rows to `keep`.
   * `after`: the previous page's `next`.
   */
  listMessages(
    mailboxId: string,
    threads: boolean,
    filters: ListFilter[],
    keep: string[],
    after: ListCursor | null,
  ): Promise<ListPage>;
  /** `remoteImages`: the user allowed the external images of this mail. */
  getMessage(messageId: string, remoteImages: boolean): Promise<Message | null>;
  /**
   * What the reader shows for a message, newest first: with `threads` its conversation in its
   * folder plus the user's own replies from "Sent", otherwise the message alone.
   */
  getConversation(messageId: string, threads: boolean): Promise<MessageSummary[]>;
  /** Fetches the full body of a large message that was cached with headers only. */
  loadMessageBody(messageId: string): Promise<void>;
  /** New, changed and deleted mail in the account's inbox. */
  syncAccount(accountId: string): Promise<SyncReport>;
  syncFolder(folderId: string): Promise<SyncReport>;
  /** The next 100 messages older than the oldest cached one. */
  loadOlder(folderId: string): Promise<SyncReport>;
  /**
   * Full-text search over all cached mail, newest first, with filters like `from:lena` or
   * `is:unread`; rows as in {@link Backend.listMessages}. `also`: messages
   * {@link Backend.searchServer} found, listed too.
   */
  searchMessages(query: string, threads: boolean, also: string[]): Promise<Conversation[]>;
  /**
   * Searches the account's folders on the server for what the cache does not have, fetches it
   * and returns the matching messages.
   */
  searchServer(accountId: string, query: string): Promise<string[]>;
  /**
   * Set (not toggle) read or flagged state. The cache changes at once; returns the accounts
   * whose changes still have to go to the server ({@link Backend.pushActions}).
   */
  setRead(messageIds: string[], read: boolean): Promise<string[]>;
  setFlagged(messageIds: string[], flagged: boolean): Promise<string[]>;
  /** Sends the account's queued changes to the server; returns how many arrived. */
  pushActions(accountId: string): Promise<number>;
  /**
   * Hidden from the lists at once; the server follows after {@link UNDO_WINDOW_MS}, until then
   * {@link Backend.undoMoves} takes it back. Messages of other accounts are left alone.
   */
  moveMessages(messageIds: string[], folderId: string): Promise<MoveOutcome>;
  /** Like a move; an account without an archive gets a folder named `folderName` first. */
  archiveMessages(messageIds: string[], folderName: string): Promise<MoveOutcome>;
  /** Into the trash; what is in the trash already (or has no trash) is deleted for good. */
  deleteMessages(messageIds: string[]): Promise<MoveOutcome>;
  /** Returns the messages that are back. */
  undoMoves(messageIds: string[]): Promise<string[]>;
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
  /**
   * The server reported a change in an account's inbox (IMAP IDLE), with the account id;
   * returns a function that stops listening.
   */
  onInboxChanged(handler: (accountId: string) => void): Promise<() => void>;
}

const tauriBackend: Backend = {
  listAccounts: () => invoke("list_accounts"),
  detectSettings: (email) => invoke("detect_settings", { email }),
  addAccount: (account, password) => invoke("add_account", { account, password }),
  addMicrosoftAccount: (account) => invoke("add_microsoft_account", { account }),
  cancelSignIn: () => invoke("cancel_sign_in"),
  removeAccount: (accountId) => invoke("remove_account", { accountId }),
  getAccountSettings: (accountId) => invoke("get_account_settings", { accountId }),
  setKeepDays: (accountId, days) => invoke("set_keep_days", { accountId, days }),
  updateAccount: (accountId, changes) => invoke("update_account", { accountId, changes }),
  changeLogin: (accountId, imap, password) => invoke("change_login", { accountId, imap, password }),
  signInAgain: (accountId) => invoke("sign_in_again", { accountId }),
  listFolders: () => invoke("list_folders"),
  listMessages: (mailboxId, threads, filters, keep, after) =>
    invoke("list_messages", { mailboxId, threads, filters, keep, after }),
  getMessage: (messageId, remoteImages) => invoke("get_message", { messageId, remoteImages }),
  getConversation: (messageId, threads) => invoke("get_conversation", { messageId, threads }),
  loadMessageBody: (messageId) => invoke("load_message_body", { messageId }),
  syncAccount: (accountId) => invoke("sync_account", { accountId }),
  syncFolder: (folderId) => invoke("sync_folder", { folderId }),
  loadOlder: (folderId) => invoke("load_older", { folderId }),
  searchMessages: (query, threads, also) => invoke("search_messages", { query, threads, also }),
  searchServer: (accountId, query) => invoke("search_server", { accountId, query }),
  setRead: (messageIds, read) => invoke("set_read", { messageIds, read }),
  setFlagged: (messageIds, flagged) => invoke("set_flagged", { messageIds, flagged }),
  pushActions: (accountId) => invoke("push_actions", { accountId }),
  moveMessages: (messageIds, folderId) => invoke("move_messages", { messageIds, folderId }),
  archiveMessages: (messageIds, folderName) =>
    invoke("archive_messages", { messageIds, folderName }),
  deleteMessages: (messageIds) => invoke("delete_messages", { messageIds }),
  undoMoves: (messageIds) => invoke("undo_moves", { messageIds }),
  saveAttachment: (attachmentId) => invoke("save_attachment", { attachmentId }),
  openAttachment: (attachmentId) => invoke("open_attachment", { attachmentId }),
  openLink: (url) => invoke("open_link", { url }),
  onMailLink: (handler) => listen<string>("mail-link", (event) => handler(event.payload)),
  onSyncProgress: (handler) =>
    listen<SyncProgress>("sync-progress", (event) => handler(event.payload)),
  onInboxChanged: (handler) => listen<string>("inbox-changed", (event) => handler(event.payload)),
};

export const usesMockData = !isTauri();

export const api: Backend = usesMockData ? mockBackend : tauriBackend;
