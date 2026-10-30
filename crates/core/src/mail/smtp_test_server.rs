//! A tiny SMTP server for tests: TLS from the first byte with a throwaway certificate for
//! "localhost", EHLO, AUTH PLAIN and XOAUTH2, MAIL, RCPT, DATA, NOOP, RSET and QUIT. It keeps the
//! recipients and the data of every mail it took; recipients at "nobody@" are unknown to it.

use std::sync::{Arc, Mutex};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use lettre::transport::smtp::client::{Certificate, TlsParameters};
use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

use crate::mail::test_server::{ACCESS_TOKEN, PASSWORD};

/// A mail as the server took it.
#[derive(Debug, Clone)]
pub struct Received {
    pub recipients: Vec<String>,
    pub data: String,
}

pub struct SmtpServer {
    pub port: u16,
    certificate: Vec<u8>,
    pub mails: Arc<Mutex<Vec<Received>>>,
}

impl SmtpServer {
    /// TLS that trusts exactly the server's certificate.
    pub fn tls(&self) -> TlsParameters {
        TlsParameters::builder("localhost".to_owned())
            .add_root_certificate(Certificate::from_der(self.certificate.clone()).unwrap())
            .build_rustls()
            .unwrap()
    }

    pub fn mails(&self) -> Vec<Received> {
        self.mails.lock().unwrap().clone()
    }
}

pub async fn start() -> SmtpServer {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let generated = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
    let certificate = generated.cert.der().to_vec();
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
        generated.signing_key.serialize_der(),
    ));
    let config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![CertificateDer::from(certificate.clone())], key)
        .unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let mails: Arc<Mutex<Vec<Received>>> = Arc::default();
    let shared = mails.clone();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let acceptor = acceptor.clone();
            let mails = shared.clone();
            tokio::spawn(async move {
                if let Ok(tls) = acceptor.accept(stream).await {
                    let _ = serve(tls, mails).await;
                }
            });
        }
    });
    SmtpServer {
        port,
        certificate,
        mails,
    }
}

async fn serve(
    stream: tokio_rustls::server::TlsStream<tokio::net::TcpStream>,
    mails: Arc<Mutex<Vec<Received>>>,
) -> std::io::Result<()> {
    let mut stream = BufReader::new(stream);
    stream
        .get_mut()
        .write_all(b"220 localhost ESMTP test\r\n")
        .await?;
    let mut recipients = Vec::new();
    loop {
        let mut line = String::new();
        if stream.read_line(&mut line).await? == 0 {
            return Ok(());
        }
        let line = line.trim_end();
        let upper = line.to_ascii_uppercase();
        let reply = if upper.starts_with("EHLO") {
            "250-localhost\r\n250-AUTH PLAIN XOAUTH2\r\n250 8BITMIME\r\n".to_owned()
        } else if let Some(response) = upper.strip_prefix("AUTH ").map(|_| &line[5..]) {
            let (mechanism, initial) = response.split_once(' ').unwrap_or((response, ""));
            let decoded = STANDARD
                .decode(initial)
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .unwrap_or_default();
            let accepted = match mechanism.to_ascii_uppercase().as_str() {
                "PLAIN" => decoded.split('\0').nth(2) == Some(PASSWORD),
                "XOAUTH2" => decoded.contains(&format!("auth=Bearer {ACCESS_TOKEN}")),
                _ => false,
            };
            if accepted {
                "235 2.7.0 Authentication successful\r\n".to_owned()
            } else {
                "535 5.7.8 Authentication credentials invalid\r\n".to_owned()
            }
        } else if upper.starts_with("MAIL FROM") {
            recipients.clear();
            "250 2.1.0 OK\r\n".to_owned()
        } else if upper.starts_with("RCPT TO") {
            if upper.contains("<NOBODY@") {
                "550 5.1.1 No such user\r\n".to_owned()
            } else {
                recipients.push(line[8..].trim().trim_matches(['<', '>']).to_owned());
                "250 2.1.5 OK\r\n".to_owned()
            }
        } else if upper == "DATA" {
            stream.get_mut().write_all(b"354 Go ahead\r\n").await?;
            let mut data = String::new();
            loop {
                let mut part = String::new();
                if stream.read_line(&mut part).await? == 0 {
                    return Ok(());
                }
                if part == ".\r\n" {
                    break;
                }
                data.push_str(&part);
            }
            mails.lock().unwrap().push(Received {
                recipients: std::mem::take(&mut recipients),
                data,
            });
            "250 2.0.0 Queued\r\n".to_owned()
        } else if upper == "QUIT" {
            stream.get_mut().write_all(b"221 2.0.0 Bye\r\n").await?;
            return Ok(());
        } else if upper == "NOOP" || upper == "RSET" {
            "250 2.0.0 OK\r\n".to_owned()
        } else {
            "502 5.5.2 Unknown command\r\n".to_owned()
        };
        stream.get_mut().write_all(reply.as_bytes()).await?;
    }
}
