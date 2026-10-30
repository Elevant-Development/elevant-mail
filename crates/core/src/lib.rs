//! Core of Elevant Mail: local cache, credentials and the mail protocols. Deliberately free of
//! Tauri, so the desktop app, the MCP server and tests can all use it.

pub mod accounts;
pub mod actions;
pub mod attachments;
pub mod db;
mod error;
pub mod ids;
pub mod image_senders;
pub mod mail;
pub mod mailbox;
pub mod model;
pub mod moves;
pub mod oauth;
pub mod query;
pub mod redact;
pub mod secrets;
mod threads;

pub use db::Database;
pub use error::{Error, ErrorKind, Hint, Result};
