//! Mail protocols: IMAP login and sync, message parsing and server settings.

pub mod download;
pub mod folders;
pub mod idle;
pub mod imap;
pub mod parse;
pub mod presets;
pub mod push;
pub mod remote;
pub mod sanitize;
pub mod search;
pub mod settings;
pub mod sync;
#[cfg(test)]
pub(crate) mod test_server;
