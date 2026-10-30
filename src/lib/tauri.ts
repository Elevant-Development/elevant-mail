/**
 * The only place the frontend talks to the backend. Every Tauri command gets a typed wrapper
 * here. The data types are generated from Rust (src/lib/bindings, written by `cargo test`).
 *
 * Inside Tauri the real backend answers; in a plain browser (`pnpm dev`) sample data does, so the
 * interface can be worked on without a mail server.
 */
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";

import type { Account } from "./bindings/Account";
import type { AccountChanges } from "./bindings/AccountChanges";
import type { AccountSettings } from "./bindings/AccountSettings";
import type { Address } from "./bindings/Address";
import type { Conversation } from "./bindings/Conversation";
import type { Folder } from "./bindings/Folder";
import type { Draft } from "./bindings/Draft";
import type { DraftAttachment } from "./bindings/DraftAttachment";
import type { ImapSettings } from "./bindings/ImapSettings";
import type { ListCursor } from "./bindings/ListCursor";
import type { ListFilter } from "./bindings/ListFilter";
import type { ListPage } from "./bindings/ListPage";
import type { Message } from "./bindings/Message";
import type { MessageSummary } from "./bindings/MessageSummary";
import type { MoveOutcome } from "./bindings/MoveOutcome";
import type { NewAccount } from "./bindings/NewAccount";
import type { OutgoingMail } from "./bindings/OutgoingMail";
import type { SmtpSettings } from "./bindings/SmtpSettings";
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
export type { ComposeMode } from "./bindings/ComposeMode";
export type { Draft } from "./bindings/Draft";
export type { DraftAttachment } from "./bindings/DraftAttachment";
export type { ImapSettings } from "./bindings/ImapSettings";
export type { ListCursor } from "./bindings/ListCursor";
export type { ListFilter } from "./bindings/ListFilter";
export type { ListPage } from "./bindings/ListPage";
export type { Message } from "./bindings/Message";
export type { MessageSummary } from "./bindings/MessageSummary";
export type { MoveOutcome } from "./bindings/MoveOutcome";
export type { NewAccount } from "./bindings/NewAccount";
export type { OutgoingMail } from "./bindings/OutgoingMail";
export type { SmtpSettings } from "./bindings/SmtpSettings";
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
  /** Takes the account's inbox into the unified inbox or out of it. */
  setInUnifiedInbox(accountId: string, included: boolean): Promise<void>;
  /** Name and colour; they change without a login. */
  updateAccount(accountId: string, changes: AccountChanges): Promise<Account>;
  /**
   * New server settings or a new password (`null` keeps the stored one). Logs in with them first
   * and changes nothing if that fails.
   */
  changeLogin(accountId: string, imap: ImapSettings, password: string | null): Promise<void>;
  /**
   * New outgoing server settings, and a password where it differs from the IMAP one; signs in
   * with them first and keeps the old ones if that fails.
   */
  changeSmtp(accountId: string, smtp: SmtpSettings, password: string | null): Promise<void>;
  /** The name mail goes out under; empty takes the one from Sent again. */
  setSenderName(accountId: string, name: string): Promise<void>;
  /**
   * The signature: HTML from the editor, or `pasted` HTML, which the backend cleans of scripts and
   * the like. Empty removes it.
   */
  setSignature(accountId: string, html: string, pasted: boolean): Promise<void>;
  /**
   * Hands the mail to the account's outgoing server; resolves once the server took it. The
   * draft it was written in (`draftKey`) goes. Filing in Sent follows
   * ({@link Backend.onCacheChanged}).
   */
  sendMessage(mail: OutgoingMail, draftKey: string | null): Promise<void>;
  /** Keeps the draft as it is now, on this device. */
  saveDraft(draft: Draft): Promise<void>;
  /** The drafts kept on this device, the latest change first. */
  listDrafts(): Promise<Draft[]>;
  /** Puts the draft into its account's drafts folder, in the background, if it changed. */
  uploadDraft(key: string): Promise<void>;
  /** Throws the draft away, here and on the server. */
  discardDraft(key: string): Promise<void>;
  /** A draft from the drafts folder (a cached message), to write on. */
  openDraft(messageId: string): Promise<Draft>;
  /**
   * Asks for files with the system's dialog and attaches them to the draft, which has to be
   * saved already; empty when cancelled.
   */
  pickAttachments(draftKey: string): Promise<DraftAttachment[]>;
  /** Attaches the files just dropped onto the window ({@link Backend.onFileDrag}). */
  attachDropped(draftKey: string): Promise<DraftAttachment[]>;
  detachAttachment(attachmentId: string): Promise<void>;
  listAttachments(draftKey: string): Promise<DraftAttachment[]>;
  /** The files of a forwarded mail go into the draft, fetched from the server. */
  forwardAttachments(draftKey: string, messageId: string): Promise<DraftAttachment[]>;
  /**
   * Files dragged over the window ("over"), away ("leave") or dropped ("drop"); returns a
   * function that stops listening.
   */
  onFileDrag(handler: (state: "over" | "leave" | "drop") => void): Promise<() => void>;
  /** Addresses matching what was typed into a recipient field, the ones written with most first. */
  suggestAddresses(query: string): Promise<Address[]>;
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
  /**
   * `remoteImages`: the user allowed the external images of this mail. They also show where the
   * user always allows them for the sender (`remoteImagesLoaded`).
   */
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
  /** Senders whose mail always shows its external images, alphabetically. */
  listImageSenders(): Promise<string[]>;
  allowImagesFrom(address: string): Promise<void>;
  forbidImagesFrom(address: string): Promise<void>;
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
  /**
   * The cache changed in the background (a sent mail filed, a draft uploaded or removed), with
   * the account id.
   */
  onCacheChanged(handler: (accountId: string) => void): Promise<() => void>;
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
  setInUnifiedInbox: (accountId, included) =>
    invoke("set_in_unified_inbox", { accountId, included }),
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
  listImageSenders: () => invoke("list_image_senders"),
  allowImagesFrom: (address) => invoke("allow_images_from", { address }),
  forbidImagesFrom: (address) => invoke("forbid_images_from", { address }),
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
  onCacheChanged: (handler) => listen<string>("cache-changed", (event) => handler(event.payload)),
  changeSmtp: (accountId, smtp, password) => invoke("change_smtp", { accountId, smtp, password }),
  setSenderName: (accountId, name) => invoke("set_sender_name", { accountId, name }),
  setSignature: (accountId, html, pasted) => invoke("set_signature", { accountId, html, pasted }),
  sendMessage: (mail, draftKey) => invoke("send_message", { mail, draftKey }),
  saveDraft: (draft) => invoke("save_draft", { draft }),
  listDrafts: () => invoke("list_drafts"),
  uploadDraft: (key) => invoke("upload_draft", { key }),
  discardDraft: (key) => invoke("discard_draft", { key }),
  openDraft: (messageId) => invoke("open_draft", { messageId }),
  pickAttachments: (draftKey) => invoke("pick_attachments", { draftKey }),
  attachDropped: (draftKey) => invoke("attach_dropped", { draftKey }),
  detachAttachment: (attachmentId) => invoke("detach_attachment", { attachmentId }),
  listAttachments: (draftKey) => invoke("list_attachments", { draftKey }),
  forwardAttachments: (draftKey, messageId) =>
    invoke("forward_attachments", { draftKey, messageId }),
  onFileDrag: (handler) =>
    getCurrentWebview().onDragDropEvent((event) => {
      const type = event.payload.type;
      handler(type === "drop" ? "drop" : type === "leave" ? "leave" : "over");
    }),
  suggestAddresses: (query) => invoke("suggest_addresses", { query }),
};

export const usesMockData = !isTauri();

export const api: Backend = usesMockData ? mockBackend : tauriBackend;
