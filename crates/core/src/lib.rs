//! Core of Elevant Mail: local cache, credentials and (later) the mail protocols. Deliberately
//! free of Tauri, so the desktop app, the MCP server and tests can all use it.

pub mod db;
mod error;
pub mod ids;
pub mod redact;
pub mod secrets;

pub use db::Database;
pub use error::{Error, ErrorKind, Hint, Result};
