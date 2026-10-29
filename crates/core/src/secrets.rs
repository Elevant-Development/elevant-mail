//! Credentials live in the operating system's keyring only: Secret Service (GNOME Keyring,
//! KWallet) on Linux, Credential Manager on Windows. Never in SQLite, files or logs.

use std::collections::HashMap;
use std::sync::Mutex;

use zeroize::Zeroizing;

use crate::{Error, Result};

/// A password or token. Debug output is masked and the memory is wiped on drop.
#[derive(Clone)]
pub struct Secret(Zeroizing<String>);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(Zeroizing::new(value.into()))
    }

    /// The plain value, only for handing it to the mail server.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(***)")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecretKind {
    ImapPassword,
    SmtpPassword,
    OAuthRefreshToken,
}

impl SecretKind {
    fn key(self) -> &'static str {
        match self {
            SecretKind::ImapPassword => "imap-password",
            SecretKind::SmtpPassword => "smtp-password",
            SecretKind::OAuthRefreshToken => "oauth-refresh-token",
        }
    }
}

/// Name of the keyring entry: one per account and kind of secret.
fn entry_name(account_id: &str, kind: SecretKind) -> String {
    format!("{account_id}:{}", kind.key())
}

pub trait SecretStore: Send + Sync {
    fn set(&self, account_id: &str, kind: SecretKind, secret: &Secret) -> Result<()>;
    fn get(&self, account_id: &str, kind: SecretKind) -> Result<Secret>;
    /// Removing something that is not there is not an error.
    fn delete(&self, account_id: &str, kind: SecretKind) -> Result<()>;
}

/// The system keyring. `service` groups the app's entries, e.g. "dev.elevant.mail".
pub struct KeyringStore {
    service: String,
}

impl KeyringStore {
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    /// Whether a keyring backend could be reached at all, checked once per process.
    pub fn status() -> Result<()> {
        keyring::Entry::store_status()
            .as_ref()
            .map_err(|_| Error::KeyringUnavailable(keyring::Error::NoDefaultStore))?;
        Ok(())
    }

    fn entry(&self, account_id: &str, kind: SecretKind) -> Result<keyring::Entry> {
        keyring::Entry::new(&self.service, &entry_name(account_id, kind)).map_err(map_keyring_error)
    }
}

fn map_keyring_error(error: keyring::Error) -> Error {
    match error {
        keyring::Error::NoEntry => Error::SecretNotFound,
        keyring::Error::NoDefaultStore
        | keyring::Error::NoStorageAccess(_)
        | keyring::Error::PlatformFailure(_) => Error::KeyringUnavailable(error),
        other => Error::Keyring(other),
    }
}

impl SecretStore for KeyringStore {
    fn set(&self, account_id: &str, kind: SecretKind, secret: &Secret) -> Result<()> {
        self.entry(account_id, kind)?
            .set_password(secret.expose())
            .map_err(map_keyring_error)
    }

    fn get(&self, account_id: &str, kind: SecretKind) -> Result<Secret> {
        let password = self
            .entry(account_id, kind)?
            .get_password()
            .map_err(map_keyring_error)?;
        Ok(Secret::new(password))
    }

    fn delete(&self, account_id: &str, kind: SecretKind) -> Result<()> {
        match self
            .entry(account_id, kind)?
            .delete_credential()
            .map_err(map_keyring_error)
        {
            Err(Error::SecretNotFound) => Ok(()),
            other => other,
        }
    }
}

/// In-memory store for tests and for running without a keyring in development.
#[derive(Default)]
pub struct MemoryStore {
    entries: Mutex<HashMap<String, Secret>>,
}

impl SecretStore for MemoryStore {
    fn set(&self, account_id: &str, kind: SecretKind, secret: &Secret) -> Result<()> {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(entry_name(account_id, kind), secret.clone());
        Ok(())
    }

    fn get(&self, account_id: &str, kind: SecretKind) -> Result<Secret> {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&entry_name(account_id, kind))
            .cloned()
            .ok_or(Error::SecretNotFound)
    }

    fn delete(&self, account_id: &str, kind: SecretKind) -> Result<()> {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&entry_name(account_id, kind));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_never_shows_the_secret() {
        let secret = Secret::new("hunter2");
        assert_eq!(format!("{secret:?}"), "Secret(***)");
        assert_eq!(secret.expose(), "hunter2");
    }

    #[test]
    fn memory_store_round_trip() {
        let store = MemoryStore::default();
        store
            .set("acc-1", SecretKind::ImapPassword, &Secret::new("one"))
            .unwrap();
        store
            .set("acc-1", SecretKind::SmtpPassword, &Secret::new("two"))
            .unwrap();

        assert_eq!(
            store
                .get("acc-1", SecretKind::ImapPassword)
                .unwrap()
                .expose(),
            "one"
        );
        assert_eq!(
            store
                .get("acc-1", SecretKind::SmtpPassword)
                .unwrap()
                .expose(),
            "two"
        );
        assert!(matches!(
            store.get("acc-2", SecretKind::ImapPassword),
            Err(Error::SecretNotFound)
        ));

        store.delete("acc-1", SecretKind::ImapPassword).unwrap();
        store.delete("acc-1", SecretKind::ImapPassword).unwrap();
        assert!(matches!(
            store.get("acc-1", SecretKind::ImapPassword),
            Err(Error::SecretNotFound)
        ));
    }

    #[test]
    fn keyring_errors_map_to_actionable_kinds() {
        assert!(matches!(
            map_keyring_error(keyring::Error::NoEntry),
            Error::SecretNotFound
        ));
        assert!(matches!(
            map_keyring_error(keyring::Error::NoDefaultStore),
            Error::KeyringUnavailable(_)
        ));
    }

    /// Talks to the real system keyring, so it is opt-in:
    /// `cargo test -p elevant-mail-core -- --ignored keyring_round_trip`
    #[test]
    #[ignore = "needs an unlocked system keyring"]
    fn keyring_round_trip() {
        let store = KeyringStore::new("dev.elevant.mail.test");
        let account = crate::ids::new_id();
        store
            .set(
                &account,
                SecretKind::ImapPassword,
                &Secret::new("test-secret"),
            )
            .unwrap();
        assert_eq!(
            store
                .get(&account, SecretKind::ImapPassword)
                .unwrap()
                .expose(),
            "test-secret"
        );
        store.delete(&account, SecretKind::ImapPassword).unwrap();
        assert!(matches!(
            store.get(&account, SecretKind::ImapPassword),
            Err(Error::SecretNotFound)
        ));
    }
}
