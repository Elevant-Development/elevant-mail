//! Errors of the core. Every error has a kind the UI can show and, where it helps, a hint what
//! the user can do about it. Messages and sources are for logs only and never contain secrets.

use serde::Serialize;
use ts_rs::TS;

use crate::mail::settings::Provider;

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

    #[error("not a valid email address")]
    InvalidEmail,

    #[error("not a valid server address")]
    InvalidHost,

    #[error("the server name could not be resolved")]
    HostNotFound,

    #[error("could not connect to the server")]
    ConnectionFailed(#[source] std::io::Error),

    #[error("the server did not answer in time")]
    Timeout,

    #[error("the server's certificate is not valid")]
    CertificateInvalid(#[source] std::io::Error),

    #[error("TLS handshake failed")]
    TlsFailed(#[source] std::io::Error),

    #[error("TLS could not be set up")]
    TlsSetup(#[source] rustls::Error),

    #[error("the server does not offer STARTTLS")]
    StartTlsUnsupported,

    #[error("the server rejected the login")]
    AuthenticationFailed { provider: Option<Provider> },

    #[error("this provider needs a sign-in via OAuth")]
    OAuthRequired { provider: Provider },

    #[error("the sign-in was cancelled")]
    SignInCancelled,

    #[error("the sign-in did not work: {0}")]
    SignInFailed(String),

    #[error("the organisation has to allow the app first")]
    ConsentRequired,

    #[error("the sign-in has expired")]
    SignInExpired,

    #[error("signed in with a different account")]
    DifferentAccount,

    #[error("the account does not sign in this way")]
    WrongSignInMethod,

    #[error("not a colour")]
    InvalidColor,

    #[error("unexpected reply from the mail server: {0}")]
    Protocol(String),

    #[error("this account has already been added")]
    AccountExists,

    #[error("account not found")]
    AccountNotFound,

    #[error("folder not found")]
    FolderNotFound,

    #[error("message not found")]
    MessageNotFound,

    #[error("attachment not found")]
    AttachmentNotFound,

    #[error("could not write the file: {0}")]
    FileWrite(std::io::Error),

    #[error("not a web or mail link")]
    InvalidLink,

    #[error("the server refused a change of read or flagged state")]
    ActionRefused,

    #[error("files that can run code are only saved, not opened")]
    NotOpenable,

    #[error("could not hand it to the system: {0}")]
    OpenExternal(std::io::Error),

    #[error("the outgoing server rejected the login")]
    SmtpAuthenticationFailed { provider: Option<Provider> },

    #[error("the mail has no recipient")]
    NoRecipients,

    #[error("not a valid recipient address: {0}")]
    InvalidRecipient(String),

    #[error("the outgoing server refused a recipient: {0}")]
    RecipientRejected(String),

    #[error("the outgoing server refused the mail: {0}")]
    MessageRejected(String),

    #[error("the attachments are too large for one mail")]
    AttachmentsTooLarge,
}

/// What went wrong, in categories the UI can translate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ErrorKind {
    Storage,
    StorageTooNew,
    KeyringUnavailable,
    CredentialMissing,
    InvalidInput,
    ServerNotFound,
    ConnectionFailed,
    Timeout,
    CertificateInvalid,
    AuthenticationFailed,
    OAuthRequired,
    ServerError,
    AccountExists,
    NotFound,
    SignInCancelled,
    SignInFailed,
    /// The organisation (tenant) lets only admins allow new apps.
    ConsentRequired,
    /// The refresh token no longer works: sign in again.
    SignInExpired,
    /// Signing in again ended with another address than the account's.
    DifferentAccount,
    /// The server did not take a change; it was undone in the app.
    ActionRefused,
    /// A file chosen by the user could not be written.
    FileWrite,
    /// The outgoing server did not take a recipient; the mail was not sent.
    RecipientRejected,
    /// The outgoing server did not take the mail (too large, taken for spam, …).
    MessageRejected,
    Internal,
}

/// A concrete next step for the user, translated in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Hint {
    /// The app data folder is not writable or the disk is full.
    CheckDiskSpace,
    /// The data was written by a newer version of the app.
    UpdateApp,
    /// On Linux a Secret Service (GNOME Keyring, KWallet) must be running and unlocked.
    UnlockKeyring,
    /// The password is gone from the keyring; entering it again fixes it.
    EnterPasswordAgain,
    CheckEmailAddress,
    /// Typo in the host, or a certificate issued for a different name.
    CheckServerAddress,
    /// E.g. 993 with TLS, 143 with STARTTLS.
    CheckPortAndSecurity,
    CheckNetwork,
    CheckCredentials,
    /// Gmail, iCloud and Yahoo need an app password instead of the account password.
    UseAppPassword,
    /// web.de and GMX switch IMAP off until it is enabled in their web mail settings.
    EnableImapAccess,
    /// Microsoft accounts sign in with Microsoft (OAuth) instead of a password.
    SignInWithMicrosoft,
    /// An admin of the organisation allows Elevant Mail once (Entra admin center).
    AskAdminForConsent,
    SignInAgain,
    /// Signing in again needs the account's own address.
    SignInAsAccount,
    /// Microsoft 365 switches IMAP off per mailbox (admin center → user → mail apps).
    EnableImapMicrosoft,
    /// Microsoft 365 switches sending from mail apps (SMTP AUTH) off per mailbox or tenant.
    EnableSmtpMicrosoft,
    /// A typo in an address, or a recipient the server does not know.
    CheckRecipients,
    AddRecipient,
    /// Together at most about 18 MB; larger files go better by a link.
    AttachmentLimit,
}

impl Error {
    pub fn kind(&self) -> ErrorKind {
        match self {
            Error::Database(_) | Error::Migration { .. } | Error::Io(_) => ErrorKind::Storage,
            Error::SchemaTooNew { .. } => ErrorKind::StorageTooNew,
            Error::KeyringUnavailable(_) => ErrorKind::KeyringUnavailable,
            Error::SecretNotFound => ErrorKind::CredentialMissing,
            Error::InvalidEmail
            | Error::InvalidHost
            | Error::InvalidLink
            | Error::InvalidColor
            | Error::NotOpenable
            | Error::WrongSignInMethod
            | Error::NoRecipients
            | Error::InvalidRecipient(_)
            | Error::AttachmentsTooLarge => ErrorKind::InvalidInput,
            Error::HostNotFound => ErrorKind::ServerNotFound,
            Error::ConnectionFailed(_) | Error::TlsFailed(_) | Error::StartTlsUnsupported => {
                ErrorKind::ConnectionFailed
            }
            Error::Timeout => ErrorKind::Timeout,
            Error::CertificateInvalid(_) => ErrorKind::CertificateInvalid,
            Error::AuthenticationFailed { .. } | Error::SmtpAuthenticationFailed { .. } => {
                ErrorKind::AuthenticationFailed
            }
            Error::RecipientRejected(_) => ErrorKind::RecipientRejected,
            Error::MessageRejected(_) => ErrorKind::MessageRejected,
            Error::OAuthRequired { .. } => ErrorKind::OAuthRequired,
            Error::SignInCancelled => ErrorKind::SignInCancelled,
            Error::SignInFailed(_) => ErrorKind::SignInFailed,
            Error::ConsentRequired => ErrorKind::ConsentRequired,
            Error::SignInExpired => ErrorKind::SignInExpired,
            Error::DifferentAccount => ErrorKind::DifferentAccount,
            Error::Protocol(_) => ErrorKind::ServerError,
            Error::AccountExists => ErrorKind::AccountExists,
            Error::ActionRefused => ErrorKind::ActionRefused,
            Error::AccountNotFound
            | Error::FolderNotFound
            | Error::MessageNotFound
            | Error::AttachmentNotFound => ErrorKind::NotFound,
            Error::FileWrite(_) => ErrorKind::FileWrite,
            Error::Keyring(_) | Error::TlsSetup(_) | Error::OpenExternal(_) => ErrorKind::Internal,
        }
    }

    pub fn hint(&self) -> Option<Hint> {
        match self {
            Error::Io(_) => Some(Hint::CheckDiskSpace),
            Error::SchemaTooNew { .. } => Some(Hint::UpdateApp),
            Error::KeyringUnavailable(_) => Some(Hint::UnlockKeyring),
            Error::SecretNotFound => Some(Hint::EnterPasswordAgain),
            Error::InvalidEmail => Some(Hint::CheckEmailAddress),
            Error::InvalidHost | Error::HostNotFound | Error::CertificateInvalid(_) => {
                Some(Hint::CheckServerAddress)
            }
            Error::ConnectionFailed(_) | Error::TlsFailed(_) | Error::StartTlsUnsupported => {
                Some(Hint::CheckPortAndSecurity)
            }
            Error::Timeout => Some(Hint::CheckNetwork),
            Error::AuthenticationFailed { provider } => Some(match provider {
                Some(Provider::Gmail | Provider::Icloud | Provider::Yahoo) => Hint::UseAppPassword,
                Some(Provider::WebDe | Provider::Gmx) => Hint::EnableImapAccess,
                Some(Provider::Microsoft) => Hint::EnableImapMicrosoft,
                None => Hint::CheckCredentials,
            }),
            Error::SmtpAuthenticationFailed { provider } => Some(match provider {
                Some(Provider::Gmail | Provider::Icloud | Provider::Yahoo) => Hint::UseAppPassword,
                Some(Provider::WebDe | Provider::Gmx) => Hint::EnableImapAccess,
                Some(Provider::Microsoft) => Hint::EnableSmtpMicrosoft,
                None => Hint::CheckCredentials,
            }),
            Error::NoRecipients => Some(Hint::AddRecipient),
            Error::AttachmentsTooLarge => Some(Hint::AttachmentLimit),
            Error::InvalidRecipient(_) | Error::RecipientRejected(_) => Some(Hint::CheckRecipients),
            Error::OAuthRequired { .. } => Some(Hint::SignInWithMicrosoft),
            Error::ConsentRequired => Some(Hint::AskAdminForConsent),
            Error::SignInExpired => Some(Hint::SignInAgain),
            Error::DifferentAccount => Some(Hint::SignInAsAccount),
            Error::SignInCancelled | Error::SignInFailed(_) => None,
            Error::Database(_)
            | Error::Migration { .. }
            | Error::Keyring(_)
            | Error::TlsSetup(_)
            | Error::Protocol(_)
            | Error::AccountExists
            | Error::AccountNotFound
            | Error::FolderNotFound
            | Error::MessageNotFound
            | Error::AttachmentNotFound
            | Error::FileWrite(_)
            | Error::InvalidLink
            | Error::InvalidColor
            | Error::NotOpenable
            | Error::WrongSignInMethod
            | Error::ActionRefused
            | Error::MessageRejected(_)
            | Error::OpenExternal(_) => None,
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

    #[test]
    fn failed_logins_get_provider_specific_hints() {
        let hint = |provider| Error::AuthenticationFailed { provider }.hint();
        assert_eq!(hint(Some(Provider::Gmail)), Some(Hint::UseAppPassword));
        assert_eq!(hint(Some(Provider::WebDe)), Some(Hint::EnableImapAccess));
        assert_eq!(hint(None), Some(Hint::CheckCredentials));
    }
}
