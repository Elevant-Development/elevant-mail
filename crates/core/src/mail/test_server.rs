//! A tiny IMAP server for tests: just enough protocol for greeting, STARTTLS, LOGIN, LIST and
//! LOGOUT, with a throwaway certificate for "localhost".

use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ClientConfig, RootCertStore, ServerConfig};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{TlsAcceptor, TlsConnector};

pub const PASSWORD: &str = "right-password";

#[derive(Debug, Clone, Copy)]
pub enum Mode {
    /// TLS from the first byte, like port 993.
    Tls,
    /// Plain greeting, then STARTTLS, like port 143.
    StartTls,
    /// Plain greeting, STARTTLS refused.
    NoStartTls,
}

pub struct TestServer {
    pub port: u16,
    /// Trusts exactly the server's throwaway certificate.
    pub connector: TlsConnector,
}

pub async fn start(mode: Mode) -> TestServer {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let generated = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
    let cert = CertificateDer::from(generated.cert.der().to_vec());
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
        generated.signing_key.serialize_der(),
    ));

    let server_config = ServerConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![cert.clone()], key)
        .unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(server_config));

    let mut roots = RootCertStore::empty();
    roots.add(cert).unwrap();
    let client_config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let _ = serve(stream, acceptor, mode).await;
            });
        }
    });

    TestServer {
        port,
        connector: TlsConnector::from(Arc::new(client_config)),
    }
}

async fn serve(mut stream: TcpStream, acceptor: TlsAcceptor, mode: Mode) -> std::io::Result<()> {
    match mode {
        Mode::Tls => {
            let mut tls = acceptor.accept(stream).await?;
            tls.write_all(b"* OK IMAP4rev1 test server ready\r\n")
                .await?;
            session(tls).await
        }
        Mode::StartTls | Mode::NoStartTls => {
            stream
                .write_all(b"* OK IMAP4rev1 test server ready\r\n")
                .await?;
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).await?;
            let tag = line.split(' ').next().unwrap_or("*").to_owned();
            let mut stream = reader.into_inner();
            if matches!(mode, Mode::NoStartTls) {
                stream
                    .write_all(format!("{tag} BAD STARTTLS not supported\r\n").as_bytes())
                    .await?;
                return Ok(());
            }
            stream
                .write_all(format!("{tag} OK begin TLS now\r\n").as_bytes())
                .await?;
            session(acceptor.accept(stream).await?).await
        }
    }
}

async fn session<S: AsyncRead + AsyncWrite + Unpin>(stream: S) -> std::io::Result<()> {
    let mut stream = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        if stream.read_line(&mut line).await? == 0 {
            return Ok(());
        }
        let mut parts = line.trim_end().splitn(3, ' ');
        let tag = parts.next().unwrap_or("*").to_owned();
        let command = parts.next().unwrap_or_default().to_ascii_uppercase();
        let arguments = parts.next().unwrap_or_default();
        let reply = match command.as_str() {
            "CAPABILITY" => format!("* CAPABILITY IMAP4rev1 SPECIAL-USE\r\n{tag} OK done\r\n"),
            "LOGIN" if arguments.contains(&format!("\"{PASSWORD}\"")) => {
                format!("{tag} OK logged in\r\n")
            }
            "LOGIN" => format!("{tag} NO [AUTHENTICATIONFAILED] Invalid credentials\r\n"),
            "LIST" => format!(
                "* LIST (\\HasNoChildren) \"/\" INBOX\r\n\
                 * LIST (\\HasNoChildren \\Sent) \"/\" Sent\r\n\
                 * LIST (\\HasNoChildren) \"/\" \"Entw&APw-rfe\"\r\n\
                 * LIST (\\Noselect \\HasChildren) \"/\" Shared\r\n\
                 {tag} OK done\r\n"
            ),
            "LOGOUT" => {
                stream
                    .write_all(format!("* BYE\r\n{tag} OK bye\r\n").as_bytes())
                    .await?;
                return Ok(());
            }
            _ => format!("{tag} BAD unknown command\r\n"),
        };
        stream.write_all(reply.as_bytes()).await?;
    }
}
