//! A tiny IMAP server for tests: greeting, STARTTLS, LOGIN, LIST, CREATE, EXAMINE, UID SEARCH,
//! UID FETCH, UID STORE, UID MOVE and COPY, EXPUNGE
//! and LOGOUT on one INBOX, with a throwaway certificate for "localhost". Tests change the
//! mailbox between syncs and can read the commands the client sent.

use std::sync::{Arc, Mutex};

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ClientConfig, RootCertStore, ServerConfig};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{TlsAcceptor, TlsConnector};

pub const PASSWORD: &str = "right-password";
/// The only access token the server takes over XOAUTH2.
pub const ACCESS_TOKEN: &str = "test-access-token";

#[derive(Debug, Clone, Copy)]
pub enum Mode {
    /// TLS from the first byte, like port 993.
    Tls,
    /// Plain greeting, then STARTTLS, like port 143.
    StartTls,
    /// Plain greeting, STARTTLS refused.
    NoStartTls,
}

#[derive(Debug, Clone)]
pub struct TestMessage {
    pub uid: u32,
    pub seen: bool,
    pub flagged: bool,
    pub deleted: bool,
    pub raw: Vec<u8>,
}

impl TestMessage {
    /// A small plain-text message whose subject and body carry the UID.
    pub fn new(uid: u32) -> Self {
        let raw = format!(
            "From: Lena Fischer <lena@example.org>\r\nTo: alex@example.org\r\n\
             Subject: Message {uid}\r\nDate: Wed, 28 Oct 2026 10:00:00 +0000\r\n\
             Message-ID: <{uid}@example.org>\r\n\r\nBody of message {uid}\r\n"
        );
        Self {
            uid,
            seen: false,
            flagged: false,
            deleted: false,
            raw: raw.into_bytes(),
        }
    }
}

#[derive(Debug)]
pub struct Mailbox {
    pub uid_validity: u32,
    pub messages: Vec<TestMessage>,
    /// Every command after login, for assertions (LOGIN itself is not recorded).
    pub commands: Vec<String>,
    /// Answer STORE with NO, like a server that does not allow the change.
    pub refuse_store: bool,
    /// Answer MOVE and COPY with NO.
    pub refuse_move: bool,
    /// An older server: neither MOVE nor UIDPLUS.
    pub without_move: bool,
    /// Folders made with CREATE; LIST shows them. Messages moved there are simply gone.
    pub created: Vec<String>,
}

impl Mailbox {
    pub fn with_messages(count: u32) -> Self {
        Self {
            uid_validity: 1,
            messages: (1..=count).map(TestMessage::new).collect(),
            commands: Vec::new(),
            refuse_store: false,
            refuse_move: false,
            without_move: false,
            created: Vec::new(),
        }
    }
}

pub type SharedMailbox = Arc<Mutex<Mailbox>>;

pub struct TestServer {
    pub port: u16,
    /// Trusts exactly the server's throwaway certificate.
    pub connector: TlsConnector,
    pub mailbox: SharedMailbox,
}

pub async fn start(mode: Mode) -> TestServer {
    start_with(mode, Mailbox::with_messages(0)).await
}

pub async fn start_with(mode: Mode, mailbox: Mailbox) -> TestServer {
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

    let mailbox: SharedMailbox = Arc::new(Mutex::new(mailbox));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let shared = mailbox.clone();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let acceptor = acceptor.clone();
            let mailbox = shared.clone();
            tokio::spawn(async move {
                let _ = serve(stream, acceptor, mode, mailbox).await;
            });
        }
    });

    TestServer {
        port,
        connector: TlsConnector::from(Arc::new(client_config)),
        mailbox,
    }
}

async fn serve(
    mut stream: TcpStream,
    acceptor: TlsAcceptor,
    mode: Mode,
    mailbox: SharedMailbox,
) -> std::io::Result<()> {
    match mode {
        Mode::Tls => {
            let mut tls = acceptor.accept(stream).await?;
            tls.write_all(b"* OK IMAP4rev1 test server ready\r\n")
                .await?;
            session(tls, mailbox).await
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
            session(acceptor.accept(stream).await?, mailbox).await
        }
    }
}

async fn session<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    mailbox: SharedMailbox,
) -> std::io::Result<()> {
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
        let arguments = parts.next().unwrap_or_default().to_owned();
        if command != "LOGIN" {
            mailbox
                .lock()
                .unwrap()
                .commands
                .push(format!("{command} {arguments}"));
        }
        let reply: Vec<u8> = match command.as_str() {
            "CAPABILITY" => {
                let extensions = if mailbox.lock().unwrap().without_move {
                    ""
                } else {
                    " MOVE UIDPLUS"
                };
                format!("* CAPABILITY IMAP4rev1 SPECIAL-USE{extensions}\r\n{tag} OK done\r\n")
                    .into_bytes()
            }
            "LOGIN" if arguments.contains(&format!("\"{PASSWORD}\"")) => {
                format!("{tag} OK logged in\r\n").into_bytes()
            }
            "LOGIN" => {
                format!("{tag} NO [AUTHENTICATIONFAILED] Invalid credentials\r\n").into_bytes()
            }
            // SASL XOAUTH2 like Microsoft: the token in one line, an error challenge on refusal.
            "AUTHENTICATE" if arguments.eq_ignore_ascii_case("XOAUTH2") => {
                stream.write_all(b"+ \r\n").await?;
                let mut answer = String::new();
                stream.read_line(&mut answer).await?;
                use base64::Engine;
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(answer.trim())
                    .unwrap_or_default();
                if String::from_utf8_lossy(&decoded)
                    .contains(&format!("auth=Bearer {ACCESS_TOKEN}\x01"))
                {
                    format!("{tag} OK authenticated\r\n").into_bytes()
                } else {
                    stream.write_all(b"+ eyJzdGF0dXMiOiI0MDEifQ==\r\n").await?;
                    answer.clear();
                    stream.read_line(&mut answer).await?;
                    format!("{tag} NO AUTHENTICATE failed\r\n").into_bytes()
                }
            }
            "LIST" => {
                let created: String = mailbox
                    .lock()
                    .unwrap()
                    .created
                    .iter()
                    .map(|name| format!("* LIST (\\HasNoChildren) \"/\" \"{name}\"\r\n"))
                    .collect();
                format!(
                    "* LIST (\\HasNoChildren) \"/\" INBOX\r\n\
                     * LIST (\\HasNoChildren \\Sent) \"/\" Sent\r\n\
                     * LIST (\\HasNoChildren) \"/\" \"Entw&APw-rfe\"\r\n\
                     * LIST (\\Noselect \\HasChildren) \"/\" Shared\r\n\
                     {created}{tag} OK done\r\n"
                )
                .into_bytes()
            }
            "CREATE" => {
                let name = arguments.trim_matches('"').to_owned();
                mailbox.lock().unwrap().created.push(name);
                format!("{tag} OK created\r\n").into_bytes()
            }
            "EXPUNGE" => {
                mailbox.lock().unwrap().messages.retain(|m| !m.deleted);
                format!("{tag} OK expunged\r\n").into_bytes()
            }
            "EXAMINE" | "SELECT" => {
                let access = if command == "SELECT" {
                    "READ-WRITE"
                } else {
                    "READ-ONLY"
                };
                let mailbox = mailbox.lock().unwrap();
                let next = mailbox.messages.iter().map(|m| m.uid).max().unwrap_or(0) + 1;
                format!(
                    "* {} EXISTS\r\n* 0 RECENT\r\n* OK [UIDVALIDITY {}] ok\r\n* OK [UIDNEXT {next}] ok\r\n\
                     * FLAGS (\\Seen \\Flagged)\r\n{tag} OK [{access}] done\r\n",
                    mailbox.messages.len(),
                    mailbox.uid_validity
                )
                .into_bytes()
            }
            "UID" => uid_command(&tag, &arguments, &mut mailbox.lock().unwrap()),
            // Every folder reports the one mailbox's numbers.
            "STATUS" => {
                let mailbox = mailbox.lock().unwrap();
                let name = arguments
                    .split(" (")
                    .next()
                    .unwrap_or_default()
                    .trim_matches('"');
                let unseen = mailbox.messages.iter().filter(|m| !m.seen).count();
                format!(
                    "* STATUS \"{name}\" (MESSAGES {} UNSEEN {unseen})\r\n{tag} OK done\r\n",
                    mailbox.messages.len()
                )
                .into_bytes()
            }
            "LOGOUT" => {
                stream
                    .write_all(format!("* BYE\r\n{tag} OK bye\r\n").as_bytes())
                    .await?;
                return Ok(());
            }
            _ => format!("{tag} BAD unknown command\r\n").into_bytes(),
        };
        stream.write_all(&reply).await?;
    }
}

fn uid_command(tag: &str, arguments: &str, mailbox: &mut Mailbox) -> Vec<u8> {
    let mut parts = arguments.splitn(3, ' ');
    let sub = parts.next().unwrap_or_default().to_ascii_uppercase();
    match sub.as_str() {
        "SEARCH" => {
            let criteria = parts.next().unwrap_or_default().to_ascii_uppercase();
            let uids: Vec<String> = mailbox
                .messages
                .iter()
                .filter(|m| criteria != "UNSEEN" || !m.seen)
                .map(|m| m.uid.to_string())
                .collect();
            let space = if uids.is_empty() { "" } else { " " };
            format!("* SEARCH{space}{}\r\n{tag} OK done\r\n", uids.join(" ")).into_bytes()
        }
        "FETCH" => {
            let set = parts.next().unwrap_or_default();
            let items = parts.next().unwrap_or_default().to_ascii_uppercase();
            let mut out = Vec::new();
            for (index, message) in mailbox.messages.iter().enumerate() {
                if !in_set(set, message.uid) {
                    continue;
                }
                let seq = index + 1;
                let flags = [(message.seen, "\\Seen"), (message.flagged, "\\Flagged")]
                    .iter()
                    .filter(|(on, _)| *on)
                    .map(|(_, flag)| *flag)
                    .collect::<Vec<_>>()
                    .join(" ");
                if items.contains("BODY.PEEK[HEADER]") {
                    let end = message
                        .raw
                        .windows(4)
                        .position(|w| w == b"\r\n\r\n")
                        .map_or(message.raw.len(), |p| p + 4);
                    let header = &message.raw[..end];
                    out.extend(
                        format!(
                            "* {seq} FETCH (UID {} BODY[HEADER] {{{}}}\r\n",
                            message.uid,
                            header.len()
                        )
                        .as_bytes(),
                    );
                    out.extend(header);
                    if items.contains("BODY.PEEK[TEXT]<0.") {
                        let text = &message.raw[end..];
                        let text = &text[..text.len().min(64 * 1024)];
                        out.extend(format!(" BODY[TEXT]<0> {{{}}}\r\n", text.len()).as_bytes());
                        out.extend(text);
                    }
                    out.extend(b")\r\n");
                } else if items.contains("BODY.PEEK[]") {
                    out.extend(
                        format!(
                            "* {seq} FETCH (UID {} BODY[] {{{}}}\r\n",
                            message.uid,
                            message.raw.len()
                        )
                        .as_bytes(),
                    );
                    out.extend(&message.raw);
                    out.extend(b")\r\n");
                } else if items.contains("RFC822.SIZE") {
                    out.extend(
                        format!(
                            "* {seq} FETCH (UID {} FLAGS ({flags}) INTERNALDATE \"28-Oct-2026 10:00:00 +0000\" RFC822.SIZE {})\r\n",
                            message.uid,
                            message.raw.len()
                        )
                        .as_bytes(),
                    );
                } else {
                    out.extend(
                        format!("* {seq} FETCH (UID {} FLAGS ({flags}))\r\n", message.uid)
                            .as_bytes(),
                    );
                }
            }
            out.extend(format!("{tag} OK done\r\n").as_bytes());
            out
        }
        "STORE" if mailbox.refuse_store => format!("{tag} NO not allowed\r\n").into_bytes(),
        "STORE" => {
            let set = parts.next().unwrap_or_default().to_owned();
            let change = parts.next().unwrap_or_default().to_ascii_uppercase();
            let add = change.starts_with('+');
            for message in &mut mailbox.messages {
                if !in_set(&set, message.uid) {
                    continue;
                }
                if change.contains("\\SEEN") {
                    message.seen = add;
                }
                if change.contains("\\FLAGGED") {
                    message.flagged = add;
                }
                if change.contains("\\DELETED") {
                    message.deleted = add;
                }
            }
            format!("{tag} OK done\r\n").into_bytes()
        }
        "MOVE" | "COPY" if mailbox.refuse_move => format!("{tag} NO not allowed\r\n").into_bytes(),
        "MOVE" if mailbox.without_move => format!("{tag} BAD unknown UID command\r\n").into_bytes(),
        "MOVE" => {
            let set = parts.next().unwrap_or_default().to_owned();
            mailbox.messages.retain(|m| !in_set(&set, m.uid));
            format!("{tag} OK moved\r\n").into_bytes()
        }
        "COPY" => format!("{tag} OK copied\r\n").into_bytes(),
        "EXPUNGE" if mailbox.without_move => {
            format!("{tag} BAD unknown UID command\r\n").into_bytes()
        }
        "EXPUNGE" => {
            let set = parts.next().unwrap_or_default().to_owned();
            mailbox
                .messages
                .retain(|m| !(m.deleted && in_set(&set, m.uid)));
            format!("{tag} OK expunged\r\n").into_bytes()
        }
        _ => format!("{tag} BAD unknown UID command\r\n").into_bytes(),
    }
}

fn in_set(set: &str, uid: u32) -> bool {
    set.split(',').any(|part| match part.split_once(':') {
        Some((start, end)) => {
            let start: u32 = start.parse().unwrap_or(u32::MAX);
            let end: u32 = if end == "*" {
                u32::MAX
            } else {
                end.parse().unwrap_or(0)
            };
            (start.min(end)..=start.max(end)).contains(&uid)
        }
        None => part.parse() == Ok(uid),
    })
}
