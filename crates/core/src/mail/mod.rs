//! Mail protocols. For now: IMAP login check, folder list and server settings.

pub mod folders;
pub mod imap;
pub mod presets;
pub mod settings;
#[cfg(test)]
pub(crate) mod test_server;
