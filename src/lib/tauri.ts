/**
 * The only place the frontend talks to the backend. Every Tauri command gets a typed wrapper
 * here, and all command types live in this file.
 *
 * Until the Rust commands exist, every call is served from mock data, in the browser and in the
 * Tauri window alike. `usesMockData` lets the UI say so.
 */
import { mockBackend } from "./mock";

export type FolderRole = "inbox" | "sent" | "drafts" | "archive" | "trash" | "junk";

export interface Account {
  id: string;
  displayName: string;
  emailAddress: string;
  /** CSS colour used to tell accounts apart. */
  color: string;
}

export interface Folder {
  id: string;
  accountId: string;
  name: string;
  role: FolderRole | null;
  unreadCount: number;
  totalCount: number;
}

export interface Address {
  name: string | null;
  address: string;
}

export interface MessageSummary {
  id: string;
  accountId: string;
  folderId: string;
  subject: string | null;
  from: Address;
  /** Unix time in milliseconds. */
  dateSent: number;
  snippet: string;
  isRead: boolean;
  isFlagged: boolean;
  hasAttachments: boolean;
}

export interface Message extends MessageSummary {
  to: Address[];
  cc: Address[];
  bodyText: string | null;
}

/** Mailbox id that shows the inboxes of all accounts together. */
export const UNIFIED_INBOX = "unified-inbox";

export interface Backend {
  listAccounts(): Promise<Account[]>;
  listFolders(): Promise<Folder[]>;
  /** `mailboxId` is a folder id or {@link UNIFIED_INBOX}. Newest first. */
  listMessages(mailboxId: string): Promise<MessageSummary[]>;
  getMessage(messageId: string): Promise<Message | null>;
}

export const usesMockData = true;

export const api: Backend = mockBackend;
