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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Folder {
    pub id: String,
    pub account_id: String,
    pub name: String,
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
}
