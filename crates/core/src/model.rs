//! Data the frontend sees. TypeScript types are generated from these (`cargo test` writes them
//! to src/lib/bindings), so both sides always agree on names and shapes.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Role of a folder, from SPECIAL-USE attributes or well-known names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum FolderRole {
    Inbox,
    Sent,
    Drafts,
    Trash,
    Archive,
    Junk,
    All,
}

impl FolderRole {
    pub fn as_str(self) -> &'static str {
        match self {
            FolderRole::Inbox => "inbox",
            FolderRole::Sent => "sent",
            FolderRole::Drafts => "drafts",
            FolderRole::Trash => "trash",
            FolderRole::Archive => "archive",
            FolderRole::Junk => "junk",
            FolderRole::All => "all",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "inbox" => FolderRole::Inbox,
            "sent" => FolderRole::Sent,
            "drafts" => FolderRole::Drafts,
            "trash" => FolderRole::Trash,
            "archive" => FolderRole::Archive,
            "junk" => FolderRole::Junk,
            "all" => FolderRole::All,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Account {
    pub id: String,
    pub display_name: String,
    pub email_address: String,
    /// CSS colour used to tell accounts apart.
    pub color: String,
    /// Its inbox is part of the unified inbox.
    pub in_unified_inbox: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Folder {
    pub id: String,
    pub account_id: String,
    pub name: String,
    /// Server path, for telling folders apart.
    pub path: String,
    /// The path split at the server's delimiter and decoded, for showing folders as a tree.
    pub path_names: Vec<String>,
    pub role: Option<FolderRole>,
    pub unread_count: u32,
    pub total_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Address {
    pub name: Option<String>,
    pub address: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MessageSummary {
    pub id: String,
    pub account_id: String,
    pub folder_id: String,
    pub subject: Option<String>,
    pub from: Address,
    /// Unix time in milliseconds.
    #[ts(type = "number")]
    pub date_sent: i64,
    pub snippet: String,
    pub is_read: bool,
    pub is_flagged: bool,
    pub has_attachments: bool,
}

/// A row of the message list: a conversation within one folder, shown with its newest message.
/// Where conversations are off, every message is one of its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Conversation {
    /// The newest message. Its read, flagged and attachment state stand for the conversation:
    /// unread while one of its messages is, flagged while one is.
    #[serde(flatten)]
    #[ts(flatten)]
    pub latest: MessageSummary,
    /// The conversation's messages in this folder, newest first: what marking, archiving,
    /// deleting and moving act on.
    pub message_ids: Vec<String>,
    /// All messages the conversation shows, the user's own replies from "Sent" included.
    pub count: u32,
    /// Who wrote, in the order of their first message, each once.
    pub senders: Vec<Address>,
}

/// What the filter bar narrows the list to. A conversation matches while one of its messages
/// in the folder does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum ListFilter {
    Unread,
    Flagged,
    Attachment,
}

/// Where the list goes on: after the row whose newest message this is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ListCursor {
    #[ts(type = "number")]
    pub date: i64,
    pub uid: u32,
    pub id: String,
}

/// One page of the message list, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ListPage {
    pub rows: Vec<Conversation>,
    /// Where the next page starts; `None` on the last.
    pub next: Option<ListCursor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Message {
    #[serde(flatten)]
    #[ts(flatten)]
    pub summary: MessageSummary,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub body_text: Option<String>,
    /// False for large mail fetched with headers only; the body is loaded when opened.
    pub body_complete: bool,
    /// Sanitized HTML for the sandboxed frame; `None` for plain-text mail.
    pub body_html: Option<String>,
    /// The HTML sets its own colours, so dark mode shows it on a light page.
    pub html_styled: bool,
    /// External images in the HTML. They are left out unless the request allowed them.
    pub remote_image_count: u32,
    /// The HTML shows them: allowed for this mail, or always for its sender.
    pub remote_images_loaded: bool,
    /// Known once the full body is cached.
    pub attachments: Vec<Attachment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Attachment {
    pub id: String,
    /// Safe to use as a file name; a fallback when the mail gives none.
    pub file_name: String,
    pub mime_type: String,
    /// Decoded size in bytes.
    #[ts(type = "number")]
    pub size: u64,
    /// False for files that could run code: those are only saved, never opened.
    pub can_open: bool,
}
