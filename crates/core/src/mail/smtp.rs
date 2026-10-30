//! Sending over SMTP: TLS from the start (465) or STARTTLS (587), checked against the operating
//! system's certificates like IMAP, signed in with the password or, for Microsoft, an OAuth access
//! token (XOAUTH2). The client greets as `[127.0.0.1]`, so the computer's name stays private.

use std::time::Duration;

use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::mail::imap::Credential;
use crate::mail::settings::{Provider, Security, SmtpSettings};
use crate::{Error, Result};

/// Connecting, signing in and handing over a large mail on a slow line.
const TIMEOUT: Duration = Duration::from_secs(60);

type Transport = AsyncSmtpTransport<Tokio1Executor>;

/// TLS with the platform's certificate checks for `host`.
pub(crate) fn platform_tls(host: &str) -> Result<TlsParameters> {
    TlsParameters::builder(host.trim().to_owned())
        .build_rustls()
        .map_err(|error| Error::TlsFailed(std::io::Error::other(error)))
}

fn transport(settings: &SmtpSettings, credential: &Credential, tls: TlsParameters) -> Transport {
    let (secret, mechanisms) = match credential {
        Credential::Password(password) => (
            password.expose().to_owned(),
            vec![Mechanism::Plain, Mechanism::Login],
        ),
        Credential::AccessToken(token) => (token.expose().to_owned(), vec![Mechanism::Xoauth2]),
    };
    let tls = match settings.security {
        Security::Tls => Tls::Wrapper(tls),
        Security::StartTls => Tls::Required(tls),
    };
    // "Dangerous" only means that lettre leaves TLS to us; it is set right here.
    Transport::builder_dangerous(settings.host.trim())
        .port(settings.port)
        .tls(tls)
        .credentials(Credentials::new(
            settings.username.trim().to_owned(),
            secret,
        ))
        .authentication(mechanisms)
        .timeout(Some(TIMEOUT))
        .build()
}

/// Connects, signs in and leaves again: whether the settings work, without sending anything.
pub async fn check_login(settings: &SmtpSettings, credential: &Credential) -> Result<()> {
    check_login_with(settings, credential, platform_tls(&settings.host)?).await
}

pub(crate) async fn check_login_with(
    settings: &SmtpSettings,
    credential: &Credential,
    tls: TlsParameters,
) -> Result<()> {
    transport(settings, credential, tls)
        .test_connection()
        .await
        .map_err(|error| send_error(&error, settings))?;
    tracing::info!(host = %settings.host, "SMTP login check passed");
    Ok(())
}

pub(crate) async fn send_with(
    settings: &SmtpSettings,
    credential: &Credential,
    tls: TlsParameters,
    message: Message,
) -> Result<()> {
    transport(settings, credential, tls)
        .send(message)
        .await
        .map_err(|error| send_error(&error, settings))?;
    Ok(())
}

/// What the user can act on. The server's own words go to the log with the error.
fn send_error(error: &lettre::transport::smtp::Error, settings: &SmtpSettings) -> Error {
    let text = error.to_string();
    if error.is_timeout() {
        return Error::Timeout;
    }
    if error.is_tls() {
        let io = std::io::Error::other(text.clone());
        return if text.contains("certificate") {
            Error::CertificateInvalid(io)
        } else {
            Error::TlsFailed(io)
        };
    }
    // The SMTP hosts of the providers we know share their IMAP hosts' domains.
    let provider = Provider::from_imap_host(&settings.host);
    match error.status().map(|code| code.to_string()) {
        Some(code) if matches!(code.as_str(), "530" | "534" | "535") => {
            Error::SmtpAuthenticationFailed { provider }
        }
        // 5.1.x: something wrong with an address.
        Some(code) if code.starts_with('5') && (code == "553" || text.contains(" 5.1.")) => {
            Error::RecipientRejected(text)
        }
        Some(_) => Error::MessageRejected(text),
        None if text.contains("authentication") || text.contains("mechanism") => {
            Error::SmtpAuthenticationFailed { provider }
        }
        None if text.contains("lookup") || text.contains("resolve") => Error::HostNotFound,
        None => Error::ConnectionFailed(std::io::Error::other(text)),
    }
}
