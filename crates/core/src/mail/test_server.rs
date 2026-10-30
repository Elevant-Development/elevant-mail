//! A tiny IMAP server for tests: greeting, STARTTLS, LOGIN, LIST, CREATE, EXAMINE, UID SEARCH,
//! UID FETCH, UID STORE, UID MOVE and COPY, EXPUNGE, IDLE, APPEND, NOOP
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
    /// INTERNALDATE, when the message arrived. Long ago by default, so the sync's period of
    /// recent mail does not cover it and only the newest batch counts.
    pub arrived: String,
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
            arrived: "28-Oct-2020 10:00:00 +0000".to_owned(),
            raw: raw.into_bytes(),
        }
    }

    /// Arrived this many days ago.
    pub fn arrived_days_ago(mut self, days: i64) -> Self {
        let time = chrono::Utc::now() - chrono::TimeDelta::days(days);
        self.arrived = time.format("%d-%b-%Y %H:%M:%S +0000").to_string();
        self
    }

    /// From, subject and body as given.
    pub fn with_content(mut self, from: &str, subject: &str, body: &str) -> Self {
        let uid = self.uid;
        self.raw = format!(
            "From: {from}\r\nTo: alex@example.org\r\nSubject: {subject}\r\n\
             Date: Wed, 28 Oct 2026 10:00:00 +0000\r\nMessage-ID: <{uid}@example.org>\r\n\
             Content-Type: text/plain; charset=utf-8\r\n\r\n{body}\r\n"
        )
        .into_bytes();
        self
    }

    /// Whether it meets all criteria of a `UID SEARCH`, as far as this server knows them:
    /// ALL, UNSEEN, FLAGGED, SINCE, TEXT, FROM and HEADER, after an optional CHARSET.
    fn matches(&self, criteria: &[String]) -> bool {
        let text = String::from_utf8_lossy(&self.raw).to_lowercase();
        let from = text
            .lines()
            .find(|line| line.starts_with("from:"))
            .unwrap_or_default()
            .to_owned();
        let mut criteria = criteria.iter();
        while let Some(key) = criteria.next() {
            let mut value = || criteria.next().cloned().unwrap_or_default().to_lowercase();
            let met = match key.to_ascii_uppercase().as_str() {
                "ALL" => true,
                "CHARSET" => {
                    value();
                    true
                }
                "UNSEEN" => !self.seen,
                "FLAGGED" => self.flagged,
                "SINCE" => day(&self.arrived) >= day(&value()),
                "TEXT" => text.contains(&value()),
                "FROM" => from.contains(&value()),
                "HEADER" => {
                    let name = format!("{}:", value());
                    let wanted = value();
                    text.lines()
                        .any(|line| line.starts_with(&name) && line.contains(&wanted))
                }
                _ => false,
            };
            if !met {
                return false;
            }
        }
        true
    }
}

/// "28-Oct-2026 …" → (2026, 10, 28), for comparing days.
fn day(date: &str) -> (u32, usize, u32) {
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let mut parts = date.split(' ').next().unwrap_or_default().split('-');
    let day = parts.next().and_then(|d| d.parse().ok()).unwrap_or(0);
    let month = parts.next().unwrap_or_default().to_lowercase();
    let month = MONTHS.iter().position(|m| *m == month).unwrap_or(0) + 1;
    let year = parts.next().and_then(|y| y.parse().ok()).unwrap_or(0);
    (year, month, day)
}

/// A command's arguments as words; quoted strings are one word without the quotes.
fn words(arguments: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for c in arguments.chars() {
        match c {
            '"' => quoted = !quoted,
            ' ' if !quoted => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
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
    /// An older server without IDLE.
    pub without_idle: bool,
    /// Wakes a session in IDLE, which then reports the number of messages (see `announce`).
    pub idle_wake: Arc<tokio::sync::Notify>,
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
            without_idle: false,
            idle_wake: Arc::default(),
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

impl TestServer {
    /// New mail, as the server tells a client waiting in IDLE.
    pub fn deliver(&self, message: TestMessage) {
        let mut mailbox = self.mailbox.lock().unwrap();
        mailbox.messages.push(message);
        mailbox.idle_wake.notify_one();
    }

    pub fn commands(&self) -> Vec<String> {
        self.mailbox.lock().unwrap().commands.clone()
    }
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
        // Literals ("{6+}" and the bytes on the next line) become quoted strings; APPEND takes
        // the bytes of the last one as they are.
        let mut last_literal = Vec::new();
        while let Some(start) = line
            .trim_end()
            .ends_with('}')
            .then(|| line.rfind('{'))
            .flatten()
        {
            let marker = line[start + 1..]
                .trim_end()
                .trim_end_matches('}')
                .to_owned();
            let length: usize = marker.trim_end_matches('+').parse().unwrap_or(0);
            if !marker.ends_with('+') {
                stream.get_mut().write_all(b"+ go on\r\n").await?;
            }
            let mut literal = vec![0; length];
            tokio::io::AsyncReadExt::read_exact(&mut stream, &mut literal).await?;
            line.truncate(start);
            line.push('"');
            line.push_str(&String::from_utf8_lossy(&literal));
            line.push('"');
            last_literal = literal;
            let mut rest = String::new();
            stream.read_line(&mut rest).await?;
            line.push_str(&rest);
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
                let mailbox = mailbox.lock().unwrap();
                let mut extensions = String::new();
                if !mailbox.without_move {
                    extensions.push_str(" MOVE UIDPLUS");
                }
                if !mailbox.without_idle {
                    extensions.push_str(" IDLE");
                }
                format!(
                    "* CAPABILITY IMAP4rev1 SPECIAL-USE LITERAL+{extensions}\r\n{tag} OK done\r\n"
                )
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
            // Until DONE; each wake-up reports the messages, as a server does for new mail.
            "IDLE" => {
                stream.write_all(b"+ idling\r\n").await?;
                let wake = mailbox.lock().unwrap().idle_wake.clone();
                loop {
                    let mut done = String::new();
                    let woken = tokio::select! {
                        read = stream.read_line(&mut done) => {
                            read?;
                            false
                        }
                        () = wake.notified() => true,
                    };
                    if !woken {
                        break;
                    }
                    let exists = mailbox.lock().unwrap().messages.len();
                    stream
                        .write_all(format!("* {exists} EXISTS\r\n").as_bytes())
                        .await?;
                }
                format!("{tag} OK IDLE done\r\n").into_bytes()
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
            "NOOP" => format!("{tag} OK done\r\n").into_bytes(),
            // Into the one mailbox, whichever folder is named.
            "APPEND" => {
                let mut mailbox = mailbox.lock().unwrap();
                let uid = mailbox.messages.iter().map(|m| m.uid).max().unwrap_or(0) + 1;
                let mut message = TestMessage::new(uid);
                message.seen = arguments.contains("\\Seen");
                message.raw = last_literal;
                mailbox.messages.push(message);
                format!("{tag} OK APPEND completed\r\n").into_bytes()
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
            let rest = [
                parts.next().unwrap_or_default(),
                parts.next().unwrap_or_default(),
            ];
            let criteria = words(&rest.join(" "));
            let uids: Vec<String> = mailbox
                .messages
                .iter()
                .filter(|m| m.matches(&criteria))
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
                            "* {seq} FETCH (UID {} FLAGS ({flags}) INTERNALDATE \"{}\" RFC822.SIZE {})\r\n",
                            message.uid,
                            message.arrived,
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
