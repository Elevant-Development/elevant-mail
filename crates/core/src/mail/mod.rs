//! Mail protocols: IMAP login and sync, sending over SMTP, message parsing and server settings.

pub mod download;
pub mod drafts;
pub mod folders;
pub mod idle;
pub mod imap;
pub mod parse;
pub mod presets;
pub mod push;
pub mod remote;
pub mod sanitize;
pub mod search;
pub mod send;
pub mod settings;
pub mod smtp;
#[cfg(test)]
pub(crate) mod smtp_test_server;
pub mod sync;
#[cfg(test)]
pub(crate) mod test_server;
