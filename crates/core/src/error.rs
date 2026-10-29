//! Errors of the core. Every error has a kind the UI can show and, where it helps, a hint what
//! the user can do about it. Messages and sources are for logs only and never contain secrets.

use serde::Serialize;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error")]
    Database(#[from] rusqlite::Error),

    #[error("database migration to version {version} failed")]
    Migration {
        version: u32,
        #[source]
        source: rusqlite::Error,
    },

    #[error("database schema version {found} is newer than this app supports ({supported})")]
    SchemaTooNew { found: u32, supported: u32 },

    #[error("file system error")]
    Io(#[from] std::io::Error),

    #[error("the system keyring is not available")]
    KeyringUnavailable(#[source] keyring::Error),

    #[error("no stored credential for this account")]
    SecretNotFound,

    #[error("keyring error")]
    Keyring(#[source] keyring::Error),
}

/// What went wrong, in categories the UI can translate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    Storage,
    StorageTooNew,
    KeyringUnavailable,
    CredentialMissing,
    Internal,
}

/// A concrete next step for the user, translated in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Hint {
    /// The app data folder is not writable or the disk is full.
    CheckDiskSpace,
    /// The data was written by a newer version of the app.
    UpdateApp,
    /// On Linux a Secret Service (GNOME Keyring, KWallet) must be running and unlocked.
    UnlockKeyring,
    /// The password is gone from the keyring; entering it again fixes it.
    EnterPasswordAgain,
}

impl Error {
    pub fn kind(&self) -> ErrorKind {
        match self {
            Error::Database(_) | Error::Migration { .. } | Error::Io(_) => ErrorKind::Storage,
            Error::SchemaTooNew { .. } => ErrorKind::StorageTooNew,
            Error::KeyringUnavailable(_) => ErrorKind::KeyringUnavailable,
            Error::SecretNotFound => ErrorKind::CredentialMissing,
            Error::Keyring(_) => ErrorKind::Internal,
        }
    }

    pub fn hint(&self) -> Option<Hint> {
        match self {
            Error::Io(_) => Some(Hint::CheckDiskSpace),
            Error::SchemaTooNew { .. } => Some(Hint::UpdateApp),
            Error::KeyringUnavailable(_) => Some(Hint::UnlockKeyring),
            Error::SecretNotFound => Some(Hint::EnterPasswordAgain),
            Error::Database(_) | Error::Migration { .. } | Error::Keyring(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_and_hints_serialize_in_camel_case() {
        assert_eq!(
            serde_json::to_string(&ErrorKind::KeyringUnavailable).unwrap(),
            "\"keyringUnavailable\""
        );
        assert_eq!(
            serde_json::to_string(&Hint::EnterPasswordAgain).unwrap(),
            "\"enterPasswordAgain\""
        );
    }

    #[test]
    fn missing_secret_suggests_entering_it_again() {
        let error = Error::SecretNotFound;
        assert_eq!(error.kind(), ErrorKind::CredentialMissing);
        assert_eq!(error.hint(), Some(Hint::EnterPasswordAgain));
    }
}
