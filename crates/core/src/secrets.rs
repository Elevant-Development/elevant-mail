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
        set_in_parts(self, &entry_name(account_id, kind), secret)
    }

    fn get(&self, account_id: &str, kind: SecretKind) -> Result<Secret> {
        get_in_parts(self, &entry_name(account_id, kind))
    }

    fn delete(&self, account_id: &str, kind: SecretKind) -> Result<()> {
        delete_in_parts(self, &entry_name(account_id, kind))
    }
}

/// Single keyring entries by name, as the platform stores them.
trait Entries {
    fn put(&self, name: &str, value: &str) -> Result<()>;
    fn read(&self, name: &str) -> Result<Zeroizing<String>>;
    /// Removing something that is not there is not an error.
    fn remove(&self, name: &str) -> Result<()>;
}

impl Entries for KeyringStore {
    fn put(&self, name: &str, value: &str) -> Result<()> {
        keyring::Entry::new(&self.service, name)
            .and_then(|entry| entry.set_password(value))
            .map_err(map_keyring_error)
    }

    fn read(&self, name: &str) -> Result<Zeroizing<String>> {
        keyring::Entry::new(&self.service, name)
            .and_then(|entry| entry.get_password())
            .map(Zeroizing::new)
            .map_err(map_keyring_error)
    }

    fn remove(&self, name: &str) -> Result<()> {
        match keyring::Entry::new(&self.service, name)
            .and_then(|entry| entry.delete_credential())
            .map_err(map_keyring_error)
        {
            Err(Error::SecretNotFound) => Ok(()),
            other => other,
        }
    }
}

/// Windows keeps at most 2560 bytes per credential, as UTF-16: 1280 characters. Microsoft's
/// refresh tokens are longer, so longer secrets go into several entries, and the entry itself
/// says how many.
const PART_CHARS: usize = 1000;
const PARTS_PREFIX: &str = "elevant-mail-parts:";

fn part_name(name: &str, index: usize) -> String {
    format!("{name}:part{index}")
}

fn set_in_parts(entries: &dyn Entries, name: &str, secret: &Secret) -> Result<()> {
    let chars: Vec<char> = secret.expose().chars().collect();
    let parts: Vec<Zeroizing<String>> = chars
        .chunks(PART_CHARS)
        .map(|chunk| Zeroizing::new(chunk.iter().collect()))
        .collect();
    let count = if parts.len() > 1 {
        for (index, part) in parts.iter().enumerate() {
            entries.put(&part_name(name, index), part)?;
        }
        entries.put(name, &format!("{PARTS_PREFIX}{}", parts.len()))?;
        parts.len()
    } else {
        entries.put(name, secret.expose())?;
        0
    };
    // Parts a longer secret left behind.
    remove_parts_from(entries, name, count)
}

fn get_in_parts(entries: &dyn Entries, name: &str) -> Result<Secret> {
    let value = entries.read(name)?;
    let Some(count) = value
        .strip_prefix(PARTS_PREFIX)
        .and_then(|count| count.parse::<usize>().ok())
    else {
        return Ok(Secret::new(value.as_str()));
    };
    let mut whole = Zeroizing::new(String::new());
    for index in 0..count {
        whole.push_str(&entries.read(&part_name(name, index))?);
    }
    Ok(Secret::new(whole.as_str()))
}

fn delete_in_parts(entries: &dyn Entries, name: &str) -> Result<()> {
    entries.remove(name)?;
    remove_parts_from(entries, name, 0)
}

fn remove_parts_from(entries: &dyn Entries, name: &str, first: usize) -> Result<()> {
    let mut index = first;
    while entries.read(&part_name(name, index)).is_ok() {
        entries.remove(&part_name(name, index))?;
        index += 1;
    }
    Ok(())
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

    /// Entries as Windows keeps them: at most 1280 characters each.
    #[derive(Default)]
    struct SmallEntries(Mutex<HashMap<String, String>>);

    impl Entries for SmallEntries {
        fn put(&self, name: &str, value: &str) -> Result<()> {
            if value.chars().count() > 1280 {
                return Err(Error::Keyring(keyring::Error::TooLong(name.into(), 2560)));
            }
            self.0.lock().unwrap().insert(name.into(), value.into());
            Ok(())
        }

        fn read(&self, name: &str) -> Result<Zeroizing<String>> {
            let entries = self.0.lock().unwrap();
            let value = entries.get(name).ok_or(Error::SecretNotFound)?;
            Ok(Zeroizing::new(value.clone()))
        }

        fn remove(&self, name: &str) -> Result<()> {
            self.0.lock().unwrap().remove(name);
            Ok(())
        }
    }

    #[test]
    fn long_secrets_fit_into_small_entries() {
        let entries = SmallEntries::default();
        let token = "x".repeat(2600);
        set_in_parts(&entries, "acc:token", &Secret::new(token.as_str())).unwrap();
        assert_eq!(entries.0.lock().unwrap().len(), 4);
        assert_eq!(get_in_parts(&entries, "acc:token").unwrap().expose(), token);

        // A shorter one replaces it, parts and all.
        set_in_parts(&entries, "acc:token", &Secret::new("kurz")).unwrap();
        assert_eq!(entries.0.lock().unwrap().len(), 1);
        assert_eq!(
            get_in_parts(&entries, "acc:token").unwrap().expose(),
            "kurz"
        );

        set_in_parts(&entries, "acc:token", &Secret::new(token.as_str())).unwrap();
        delete_in_parts(&entries, "acc:token").unwrap();
        assert!(entries.0.lock().unwrap().is_empty());
        assert!(matches!(
            get_in_parts(&entries, "acc:token"),
            Err(Error::SecretNotFound)
        ));
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

        // As long as a Microsoft refresh token, too long for one Windows credential.
        let token = "t".repeat(2600);
        let kind = SecretKind::OAuthRefreshToken;
        store
            .set(&account, kind, &Secret::new(token.as_str()))
            .unwrap();
        assert_eq!(store.get(&account, kind).unwrap().expose(), token);
        store.delete(&account, kind).unwrap();
        assert!(matches!(
            store.get(&account, kind),
            Err(Error::SecretNotFound)
        ));
    }
}
