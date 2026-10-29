//! IMAP connections: TLS (or STARTTLS) with the operating system's certificate checks, login,
//! folder list. Plain-text connections and certificate exceptions do not exist.

use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use async_imap::{Client, Session};
use futures::TryStreamExt;
use rustls::ClientConfig;
use rustls::pki_types::ServerName;
use rustls_platform_verifier::BuilderVerifierExt;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;

use crate::mail::folders::RemoteFolder;
use crate::mail::settings::{ImapSettings, Provider, Security};
use crate::secrets::Secret;
use crate::{Error, Result};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) type ImapSession = Session<TlsStream<TcpStream>>;

/// What a successful login found out.
#[derive(Debug)]
pub struct ConnectionReport {
    pub folders: Vec<RemoteFolder>,
}

/// Connects, logs in, lists the folders and logs out again.
pub async fn check_login(settings: &ImapSettings, password: &Secret) -> Result<ConnectionReport> {
    check_login_with(settings, password, &platform_connector()?).await
}

pub(crate) async fn check_login_with(
    settings: &ImapSettings,
    password: &Secret,
    connector: &TlsConnector,
) -> Result<ConnectionReport> {
    let mut session = open_session(settings, password, connector).await?;
    let folders = with_timeout(COMMAND_TIMEOUT, list_folders(&mut session)).await??;
    // Best effort: the check succeeded even if the goodbye gets lost.
    let _ = with_timeout(COMMAND_TIMEOUT, session.logout()).await;
    tracing::info!(host = %settings.host, folders = folders.len(), "IMAP login check passed");
    Ok(ConnectionReport { folders })
}

/// Connects and logs in; the caller logs out.
pub(crate) async fn open_session(
    settings: &ImapSettings,
    password: &Secret,
    connector: &TlsConnector,
) -> Result<ImapSession> {
    let provider = Provider::from_imap_host(&settings.host);
    if provider.is_some_and(Provider::needs_oauth) {
        return Err(Error::OAuthRequired {
            provider: Provider::Microsoft,
        });
    }
    let client = with_timeout(CONNECT_TIMEOUT, connect(settings, connector)).await??;
    with_timeout(
        COMMAND_TIMEOUT,
        client.login(&settings.username, password.expose()),
    )
    .await?
    .map_err(|(error, _client)| login_error(error, provider))
}

/// TLS with the operating system's trust store and checks (revocation, enterprise CAs).
pub fn platform_connector() -> Result<TlsConnector> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .and_then(BuilderVerifierExt::with_platform_verifier)
        .map_err(Error::TlsSetup)?
        .with_no_client_auth();
    Ok(TlsConnector::from(Arc::new(config)))
}

async fn connect(
    settings: &ImapSettings,
    connector: &TlsConnector,
) -> Result<Client<TlsStream<TcpStream>>> {
    let server_name =
        ServerName::try_from(settings.host.trim().to_owned()).map_err(|_| Error::InvalidHost)?;
    let addresses: Vec<SocketAddr> = tokio::net::lookup_host((settings.host.trim(), settings.port))
        .await
        .map_err(|_| Error::HostNotFound)?
        .collect();
    let tcp = connect_any(&addresses).await?;

    match settings.security {
        Security::Tls => {
            let stream = connector
                .connect(server_name, tcp)
                .await
                .map_err(tls_error)?;
            let mut client = Client::new(stream);
            expect_greeting(&mut client).await?;
            Ok(client)
        }
        Security::StartTls => {
            let mut client = Client::new(tcp);
            expect_greeting(&mut client).await?;
            client
                .run_command_and_check_ok("STARTTLS", None)
                .await
                .map_err(|_| Error::StartTlsUnsupported)?;
            let stream = connector
                .connect(server_name, client.into_inner())
                .await
                .map_err(tls_error)?;
            // No second greeting after STARTTLS.
            Ok(Client::new(stream))
        }
    }
}

async fn connect_any(addresses: &[SocketAddr]) -> Result<TcpStream> {
    let mut last_error = None;
    for address in addresses {
        match TcpStream::connect(address).await {
            Ok(stream) => return Ok(stream),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.map_or(Error::HostNotFound, Error::ConnectionFailed))
}

async fn expect_greeting<T>(client: &mut Client<T>) -> Result<()>
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + std::fmt::Debug + Send,
{
    match client.read_response().await {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err(Error::ConnectionFailed(
            std::io::ErrorKind::UnexpectedEof.into(),
        )),
        Err(error) => Err(Error::ConnectionFailed(error)),
    }
}

pub(crate) async fn list_folders(session: &mut ImapSession) -> Result<Vec<RemoteFolder>> {
    let names: Vec<_> = session
        .list(Some(""), Some("*"))
        .await
        .map_err(protocol_error)?
        .try_collect()
        .await
        .map_err(protocol_error)?;
    Ok(names.iter().map(RemoteFolder::from_name).collect())
}

pub(crate) async fn with_timeout<F: Future>(duration: Duration, future: F) -> Result<F::Output> {
    tokio::time::timeout(duration, future)
        .await
        .map_err(|_| Error::Timeout)
}

/// A refused certificate gets its own error: it usually means a wrong host name, and the app
/// never offers to accept it anyway.
fn tls_error(error: std::io::Error) -> Error {
    let invalid_certificate = error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<rustls::Error>())
        .is_some_and(|inner| matches!(inner, rustls::Error::InvalidCertificate(_)));
    if invalid_certificate {
        Error::CertificateInvalid(error)
    } else {
        Error::TlsFailed(error)
    }
}

/// Server replies to LOGIN are dropped on purpose: some servers echo the command.
fn login_error(error: async_imap::error::Error, provider: Option<Provider>) -> Error {
    use async_imap::error::Error as Imap;
    match error {
        Imap::No(_) | Imap::Bad(_) | Imap::Validate(_) => Error::AuthenticationFailed { provider },
        Imap::Io(error) => Error::ConnectionFailed(error),
        Imap::ConnectionLost => {
            Error::ConnectionFailed(std::io::ErrorKind::ConnectionAborted.into())
        }
        _ => Error::Protocol("unexpected reply to LOGIN".into()),
    }
}

pub(crate) fn protocol_error(error: async_imap::error::Error) -> Error {
    Error::Protocol(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorKind;
    use crate::mail::test_server::{self, Mode, PASSWORD};
    use crate::model::FolderRole;

    fn settings(port: u16, security: Security) -> ImapSettings {
        ImapSettings {
            host: "localhost".into(),
            port,
            security,
            username: "alex@example.org".into(),
        }
    }

    #[tokio::test]
    async fn tls_login_lists_folders_with_roles() {
        let server = test_server::start(Mode::Tls).await;
        let report = check_login_with(
            &settings(server.port, Security::Tls),
            &Secret::new(PASSWORD),
            &server.connector,
        )
        .await
        .unwrap();

        let by_path = |path: &str| {
            report
                .folders
                .iter()
                .find(|folder| folder.path == path)
                .unwrap()
        };
        assert_eq!(by_path("INBOX").role, Some(FolderRole::Inbox));
        assert_eq!(by_path("Sent").role, Some(FolderRole::Sent));
        assert_eq!(by_path("Entw&APw-rfe").name, "Entwürfe");
        assert!(!by_path("Shared").selectable);
    }

    #[tokio::test]
    async fn starttls_upgrades_before_login() {
        let server = test_server::start(Mode::StartTls).await;
        let report = check_login_with(
            &settings(server.port, Security::StartTls),
            &Secret::new(PASSWORD),
            &server.connector,
        )
        .await
        .unwrap();
        assert_eq!(report.folders.len(), 4);
    }

    #[tokio::test]
    async fn wrong_password_is_an_authentication_error() {
        let server = test_server::start(Mode::Tls).await;
        let error = check_login_with(
            &settings(server.port, Security::Tls),
            &Secret::new("wrong"),
            &server.connector,
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            Error::AuthenticationFailed { provider: None }
        ));
        assert_eq!(error.hint(), Some(crate::Hint::CheckCredentials));
    }

    #[tokio::test]
    async fn refused_starttls_never_falls_back_to_plain_text() {
        let server = test_server::start(Mode::NoStartTls).await;
        let error = check_login_with(
            &settings(server.port, Security::StartTls),
            &Secret::new(PASSWORD),
            &server.connector,
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::StartTlsUnsupported));
    }

    #[tokio::test]
    async fn untrusted_certificates_are_refused() {
        let server = test_server::start(Mode::Tls).await;
        // The operating system does not know the throwaway certificate.
        let error = check_login_with(
            &settings(server.port, Security::Tls),
            &Secret::new(PASSWORD),
            &platform_connector().unwrap(),
        )
        .await
        .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::CertificateInvalid, "{error:?}");
    }

    #[tokio::test]
    async fn tls_on_a_plain_text_port_points_to_port_and_security() {
        let server = test_server::start(Mode::StartTls).await;
        let error = check_login_with(
            &settings(server.port, Security::Tls),
            &Secret::new(PASSWORD),
            &server.connector,
        )
        .await
        .unwrap_err();
        assert_eq!(
            error.hint(),
            Some(crate::Hint::CheckPortAndSecurity),
            "{error:?}"
        );
    }

    #[tokio::test]
    async fn unknown_hosts_are_reported_as_such() {
        let mut settings = settings(993, Security::Tls);
        settings.host = "mail.does-not-exist.invalid".into();
        let error = check_login_with(
            &settings,
            &Secret::new(PASSWORD),
            &platform_connector().unwrap(),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::HostNotFound), "{error:?}");
    }

    #[tokio::test]
    async fn microsoft_accounts_are_told_about_oauth_without_connecting() {
        let mut settings = settings(993, Security::Tls);
        settings.host = "outlook.office365.com".into();
        let error = check_login(&settings, &Secret::new("any"))
            .await
            .unwrap_err();
        assert!(matches!(error, Error::OAuthRequired { .. }));
    }
}
