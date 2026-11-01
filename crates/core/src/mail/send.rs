//! Sending what the user wrote: a MIME message (HTML and plain text) with the headers that keep
//! it in its conversation, handed to the account's SMTP server, then filed in Sent, unless the
//! server did that by itself (Gmail, Exchange).

use std::time::{Duration, SystemTime};

use lettre::Message;
use lettre::address::Envelope;
use lettre::message::header::ContentType;
use lettre::message::{Attachment, Mailbox, MultiPart};
use lettre::transport::smtp::client::TlsParameters;
use rusqlite::{Connection, OptionalExtension};
use serde::Deserialize;
use tokio_rustls::TlsConnector;
use ts_rs::TS;

use crate::accounts::{self, SignInMethod};
use crate::db::Database;
use crate::ids::new_id;
use crate::mail::imap::{self, COMMAND_TIMEOUT, Credential, protocol_error, with_timeout};
use crate::mail::smtp;
use crate::model::Address;
use crate::secrets::{SecretKind, SecretStore};
use crate::{Error, Result};

/// A mail as the composer hands it over.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OutgoingMail {
    pub account_id: String,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub bcc: Vec<Address>,
    pub subject: String,
    /// The text from the editor, as HTML and as plain text for mail apps without HTML.
    pub html: String,
    pub text: String,
    /// The cached message this answers; its Message-ID goes into In-Reply-To and References.
    pub in_reply_to: Option<String>,
}

/// A file that goes along: from a draft (`drafts::files`).
#[derive(Debug, Clone)]
pub struct OutgoingFile {
    pub file_name: String,
    pub mime_type: String,
    pub data: Vec<u8>,
}

/// A mail the server took: its Message-ID and the copy for Sent, Bcc included.
#[derive(Debug, Clone)]
pub struct SentMail {
    pub account_id: String,
    pub message_id: String,
    pub copy: Vec<u8>,
}

/// How long to give the server to put the mail into Sent by itself before filing it there.
const WAIT_FOR_SENT: [Duration; 2] = [Duration::from_secs(2), Duration::from_secs(4)];

/// Sends the mail with the files attached to `draft`, the draft it was written in.
pub async fn send(
    db: &Database,
    secrets: &dyn SecretStore,
    mail: &OutgoingMail,
    draft: Option<&str>,
) -> Result<SentMail> {
    let host = accounts::settings(&db.conn(), &mail.account_id)?.smtp.host;
    send_with(db, secrets, mail, draft, smtp::platform_tls(&host)?).await
}

pub(crate) async fn send_with(
    db: &Database,
    secrets: &dyn SecretStore,
    mail: &OutgoingMail,
    draft: Option<&str>,
    tls: TlsParameters,
) -> Result<SentMail> {
    let settings = accounts::settings(&db.conn(), &mail.account_id)?;
    let files = match draft {
        Some(key) => crate::mail::drafts::files(&db.conn(), key)?,
        None => Vec::new(),
    };
    let (message, sent) = prepare(&db.conn(), mail, &files)?;
    let credential = credential(secrets, &mail.account_id, settings.sign_in).await?;
    smtp::send_with(&settings.smtp, &credential, tls, message).await?;
    tracing::info!(account = %mail.account_id, "mail sent");
    Ok(sent)
}

/// The account's sign-in for SMTP: the access token, or the SMTP password where one was set
/// apart from the IMAP one.
pub(crate) async fn credential(
    secrets: &dyn SecretStore,
    account_id: &str,
    sign_in: SignInMethod,
) -> Result<Credential> {
    Ok(match sign_in {
        SignInMethod::Microsoft => {
            Credential::AccessToken(crate::oauth::access_token(secrets, account_id).await?)
        }
        SignInMethod::Password => {
            Credential::Password(match secrets.get(account_id, SecretKind::SmtpPassword) {
                Err(Error::SecretNotFound) => secrets.get(account_id, SecretKind::ImapPassword)?,
                other => other?,
            })
        }
    })
}

/// The message to send (without Bcc) and the copy for Sent (with it).
fn prepare(
    conn: &Connection,
    mail: &OutgoingMail,
    files: &[OutgoingFile],
) -> Result<(Message, SentMail)> {
    if mail.to.is_empty() && mail.cc.is_empty() && mail.bcc.is_empty() {
        return Err(Error::NoRecipients);
    }
    let message_id = new_message_id(&account_email(conn, &mail.account_id)?);
    let (message, copy) = compose(conn, mail, &message_id, false, files)?;
    Ok((
        message,
        SentMail {
            account_id: mail.account_id.clone(),
            message_id,
            copy,
        },
    ))
}

pub(crate) fn account_email(conn: &Connection, account_id: &str) -> Result<String> {
    conn.query_row(
        "SELECT email_address FROM accounts WHERE id = ?1",
        [account_id],
        |row| row.get(0),
    )
    .optional()?
    .ok_or(Error::AccountNotFound)
}

/// A new Message-ID in the domain of the sender's address.
pub(crate) fn new_message_id(email: &str) -> String {
    let domain = email
        .rsplit_once('@')
        .map_or("localhost", |(_, domain)| domain);
    format!("<{}@{domain}>", new_id())
}

/// The mail as MIME: the message to send (without Bcc) and the copy to keep (with it), sharing
/// `message_id` and date. A `draft` may still hold addresses being typed: those are left out
/// instead of refused. Without recipients the envelope names the sender; only SMTP reads it.
pub(crate) fn compose(
    conn: &Connection,
    mail: &OutgoingMail,
    message_id: &str,
    draft: bool,
    files: &[OutgoingFile],
) -> Result<(Message, Vec<u8>)> {
    let email = account_email(conn, &mail.account_id)?;
    let sender_name = accounts::settings(conn, &mail.account_id)?.sender_name;
    let from = mailbox(&sender_name, &email).map_err(|()| Error::InvalidEmail)?;
    let thread = match &mail.in_reply_to {
        Some(id) => thread_headers(conn, id)?,
        None => None,
    };
    let recipients = |list: &[Address]| -> Result<Vec<Mailbox>> {
        list.iter()
            .filter_map(|address| {
                match mailbox(
                    address.name.as_deref().unwrap_or_default(),
                    &address.address,
                ) {
                    Ok(mailbox) => Some(Ok(mailbox)),
                    Err(()) if draft => None,
                    Err(()) => Some(Err(Error::InvalidRecipient(address.address.clone()))),
                }
            })
            .collect()
    };
    let (to, cc, bcc) = (
        recipients(&mail.to)?,
        recipients(&mail.cc)?,
        recipients(&mail.bcc)?,
    );
    let nobody = to.is_empty() && cc.is_empty() && bcc.is_empty();
    let date = SystemTime::now();

    let build = |keep_bcc: bool| -> Result<Message> {
        let mut builder = Message::builder()
            .from(from.clone())
            .subject(mail.subject.trim())
            .date(date)
            .message_id(Some(message_id.to_owned()));
        for mailbox in &to {
            builder = builder.to(mailbox.clone());
        }
        for mailbox in &cc {
            builder = builder.cc(mailbox.clone());
        }
        for mailbox in &bcc {
            builder = builder.bcc(mailbox.clone());
        }
        if keep_bcc {
            builder = builder.keep_bcc();
        }
        if nobody {
            let envelope = Envelope::new(Some(from.email.clone()), vec![from.email.clone()])
                .map_err(|error| Error::MessageRejected(error.to_string()))?;
            builder = builder.envelope(envelope);
        }
        if let Some((parent, references)) = &thread {
            builder = builder
                .in_reply_to(parent.clone())
                .references(references.clone());
        }
        let text = MultiPart::alternative_plain_html(mail.text.clone(), html_document(&mail.html));
        let body = if files.is_empty() {
            text
        } else {
            files
                .iter()
                .fold(MultiPart::mixed().multipart(text), |body, file| {
                    let kind = ContentType::parse(&file.mime_type).unwrap_or_else(|_| {
                        ContentType::parse("application/octet-stream").expect("a media type")
                    });
                    body.singlepart(
                        Attachment::new(file.file_name.clone()).body(file.data.clone(), kind),
                    )
                })
        };
        builder
            .multipart(body)
            .map_err(|error| Error::MessageRejected(error.to_string()))
    };
    Ok((build(false)?, build(true)?.formatted()))
}

fn mailbox(name: &str, address: &str) -> std::result::Result<Mailbox, ()> {
    let address = address.trim().parse().map_err(|_| ())?;
    let name = name.trim();
    Ok(Mailbox::new(
        (!name.is_empty()).then(|| name.to_owned()),
        address,
    ))
}

/// In-Reply-To and References for an answer to the cached message `id`: its Message-ID, after
/// the ones it refers to itself. `None` for mail without a Message-ID.
pub(crate) fn thread_headers(conn: &Connection, id: &str) -> Result<Option<(String, String)>> {
    let row: Option<(Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT message_id, references_header FROM messages WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((Some(parent), references)) = row else {
        return Ok(None);
    };
    let bracket = |id: &str| format!("<{}>", id.trim_matches(['<', '>']));
    let mut chain: Vec<String> = references
        .unwrap_or_default()
        .split_whitespace()
        .map(bracket)
        .collect();
    chain.push(bracket(&parent));
    Ok(Some((bracket(&parent), chain.join(" "))))
}

/// The editor's HTML as a document of its own, declared UTF-8.
fn html_document(body: &str) -> String {
    format!(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"></head><body>{body}</body></html>\n"
    )
}

/// After sending: files the copy in the account's Sent folder, unless the server put the mail
/// there by itself within a few seconds. Returns whether it filed it.
pub async fn file_in_sent(
    db: &Database,
    secrets: &dyn SecretStore,
    sent: &SentMail,
) -> Result<bool> {
    file_in_sent_with(
        db,
        secrets,
        sent,
        &imap::platform_connector()?,
        &WAIT_FOR_SENT,
    )
    .await
}

pub(crate) async fn file_in_sent_with(
    db: &Database,
    secrets: &dyn SecretStore,
    sent: &SentMail,
    connector: &TlsConnector,
    waits: &[Duration],
) -> Result<bool> {
    let path: Option<String> = db
        .conn()
        .query_row(
            "SELECT path FROM folders WHERE account_id = ?1 AND role = 'sent'",
            [&sent.account_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(path) = path else {
        tracing::warn!(account = %sent.account_id, "no Sent folder to file the mail in");
        return Ok(false);
    };
    let (settings, credential) =
        crate::mail::sync::credentials(db, secrets, &sent.account_id).await?;
    let mut session = imap::open_session(&settings, &credential, connector).await?;
    with_timeout(COMMAND_TIMEOUT, session.select(&path))
        .await?
        .map_err(protocol_error)?;
    let query = format!("HEADER Message-ID \"{}\"", sent.message_id);
    let mut there = false;
    for wait in waits {
        tokio::time::sleep(*wait).await;
        with_timeout(COMMAND_TIMEOUT, session.noop())
            .await?
            .map_err(protocol_error)?;
        there = !with_timeout(COMMAND_TIMEOUT, session.uid_search(&query))
            .await?
            .map_err(protocol_error)?
            .is_empty();
        if there {
            break;
        }
    }
    if !there {
        with_timeout(
            COMMAND_TIMEOUT,
            session.append(&path, Some("(\\Seen)"), None, &sent.copy),
        )
        .await?
        .map_err(protocol_error)?;
    }
    let _ = with_timeout(COMMAND_TIMEOUT, session.logout()).await;
    tracing::info!(account = %sent.account_id, filed = !there, "sent mail in Sent");
    Ok(!there)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::{Login, NewAccount, change_smtp_with, save, set_sender_name};
    use crate::mail::folders::RemoteFolder;
    use crate::mail::imap::ConnectionReport;
    use crate::mail::settings::{ImapSettings, Security, SmtpSettings};
    use crate::mail::smtp_test_server::{self, SmtpServer};
    use crate::mail::sync::sync_with_connector;
    use crate::mail::test_server::{self, Mailbox, Mode, PASSWORD, TestServer};
    use crate::model::FolderRole;
    use crate::secrets::{MemoryStore, Secret};

    struct Setup {
        db: Database,
        secrets: MemoryStore,
        account_id: String,
        imap: TestServer,
        smtp: SmtpServer,
    }

    /// An account with three messages in its inbox and an outgoing server that takes it.
    async fn setup() -> Setup {
        let imap = test_server::start_with(Mode::Tls, Mailbox::with_messages(3)).await;
        let smtp = smtp_test_server::start().await;
        let db = Database::open_in_memory().unwrap();
        let secrets = MemoryStore::default();
        let account = save(
            &db,
            &secrets,
            &NewAccount {
                display_name: "Personal".into(),
                email_address: "alex@example.org".into(),
                imap: ImapSettings {
                    host: "localhost".into(),
                    port: imap.port,
                    security: Security::Tls,
                    username: "alex@example.org".into(),
                },
            },
            Login::Password(&Secret::new(PASSWORD)),
            &ConnectionReport {
                folders: vec![
                    RemoteFolder::new("INBOX", Some("/"), None, true),
                    RemoteFolder::new("Sent", Some("/"), Some(FolderRole::Sent), true),
                ],
            },
        )
        .unwrap();
        sync_with_connector(&db, &secrets, &account.id, &imap.connector)
            .await
            .unwrap();
        // The IMAP password signs in to SMTP as well.
        let outgoing = SmtpSettings {
            host: "localhost".into(),
            port: smtp.port,
            security: Security::Tls,
            username: "alex@example.org".into(),
        };
        change_smtp_with(&db, &secrets, &account.id, outgoing, None, smtp.tls())
            .await
            .unwrap();
        Setup {
            db,
            secrets,
            account_id: account.id,
            imap,
            smtp,
        }
    }

    fn address(address: &str) -> Address {
        Address {
            name: None,
            address: address.into(),
        }
    }

    impl Setup {
        fn mail(&self, to: &[&str]) -> OutgoingMail {
            OutgoingMail {
                account_id: self.account_id.clone(),
                to: to.iter().map(|to| address(to)).collect(),
                cc: Vec::new(),
                bcc: Vec::new(),
                subject: "Hallo".into(),
                html: "<p>Gern!</p>".into(),
                text: "Gern!".into(),
                in_reply_to: None,
            }
        }

        fn cached_id(&self, uid: u32) -> String {
            self.db
                .conn()
                .query_row(
                    "SELECT id FROM messages WHERE uid = ?1 AND folder_id IN
                       (SELECT id FROM folders WHERE role = 'inbox')",
                    [uid],
                    |row| row.get(0),
                )
                .unwrap()
        }
    }

    #[tokio::test]
    async fn a_reply_goes_out_in_its_thread_and_is_filed_in_sent() {
        let setup = setup().await;
        set_sender_name(&setup.db.conn(), &setup.account_id, "Alex Berger").unwrap();
        let reply = OutgoingMail {
            to: vec![Address {
                name: Some("Lena Fischer".into()),
                address: "lena@example.org".into(),
            }],
            bcc: vec![address("secret@example.org")],
            subject: "Re: Message 2".into(),
            in_reply_to: Some(setup.cached_id(2)),
            ..setup.mail(&[])
        };
        let sent = send_with(&setup.db, &setup.secrets, &reply, None, setup.smtp.tls())
            .await
            .unwrap();

        let mails = setup.smtp.mails();
        assert_eq!(mails.len(), 1);
        assert_eq!(
            mails[0].recipients,
            ["lena@example.org", "secret@example.org"]
        );
        let data = &mails[0].data;
        assert!(
            data.contains(r#"From: "Alex Berger" <alex@example.org>"#),
            "{data}"
        );
        assert!(data.contains(r#"To: "Lena Fischer" <lena@example.org>"#));
        assert!(data.contains("In-Reply-To: <2@example.org>"));
        assert!(data.contains("References: <2@example.org>"));
        assert!(data.contains(&format!("Message-ID: {}", sent.message_id)));
        assert!(sent.message_id.ends_with("@example.org>"));
        assert!(data.contains("multipart/alternative"));
        // Bcc recipients get the mail but are named only in the copy for Sent.
        assert!(!data.contains("Bcc:"));
        assert!(String::from_utf8_lossy(&sent.copy).contains("Bcc: secret@example.org"));

        // Nobody filed it: the app does, read.
        let filed = file_in_sent_with(
            &setup.db,
            &setup.secrets,
            &sent,
            &setup.imap.connector,
            &[Duration::ZERO],
        )
        .await
        .unwrap();
        assert!(filed);
        {
            let mailbox = setup.imap.mailbox.lock().unwrap();
            let copy = mailbox.messages.last().unwrap();
            assert!(copy.seen);
            assert_eq!(copy.raw, sent.copy);
        }
        // Once the server has it (like Gmail), it is not filed again.
        let filed = file_in_sent_with(
            &setup.db,
            &setup.secrets,
            &sent,
            &setup.imap.connector,
            &[Duration::ZERO],
        )
        .await
        .unwrap();
        assert!(!filed);
        assert_eq!(setup.imap.mailbox.lock().unwrap().messages.len(), 4);
    }

    #[tokio::test]
    async fn an_invitation_is_answered_to_its_organizer() {
        use crate::calendar::invitation::{self, Answer, InvitationAnswer, tests::OUTLOOK};
        let setup = setup().await;
        set_sender_name(&setup.db.conn(), &setup.account_id, "Alex Berger").unwrap();
        let id = setup.cached_id(2);
        let invite = |ics: &str| {
            setup
                .db
                .conn()
                .execute(
                    "UPDATE messages SET calendar = ?2 WHERE id = ?1",
                    rusqlite::params![id, ics],
                )
                .unwrap();
        };
        invite(OUTLOOK);
        let shown = invitation::for_message(&setup.db.conn(), &id)
            .unwrap()
            .unwrap();
        assert!(shown.can_answer);
        assert_eq!(shown.answer, None);

        let answer = InvitationAnswer {
            message_id: id.clone(),
            answer: Answer::Accepted,
            subject: "Zugesagt: Planung Herbstfest".into(),
            text: "Ich nehme teil.".into(),
        };
        invitation::answer_with(&setup.db, &setup.secrets, &answer, None, setup.smtp.tls())
            .await
            .unwrap();
        let mails = setup.smtp.mails();
        assert_eq!(mails.len(), 1);
        assert_eq!(mails[0].recipients, ["lena@example.org"]);
        let data = &mails[0].data;
        assert!(
            data.contains(r#"From: "Alex Berger" <alex@example.org>"#),
            "{data}"
        );
        assert!(data.contains(r#"To: "Lena Fischer" <lena@example.org>"#));
        assert!(data.contains("Subject: Zugesagt: Planung Herbstfest"));
        assert!(data.contains("In-Reply-To: <2@example.org>"));
        assert!(data.contains("text/calendar; method=REPLY"), "{data}");
        assert!(data.contains("METHOD:REPLY"), "{data}");
        assert!(data.contains("PARTSTAT=ACCEPTED"), "{data}");
        // Answers are not filed in Sent.
        assert_eq!(setup.imap.mailbox.lock().unwrap().messages.len(), 3);

        let shown = invitation::for_message(&setup.db.conn(), &id)
            .unwrap()
            .unwrap();
        assert_eq!(shown.answer, Some(Answer::Accepted));
        assert!(!shown.changed_since_answer);
        // Accepted: in the calendar (the one on this computer: the account has none), with the
        // answer filled in, and without the server answering again.
        let entry = shown.in_calendar.clone().unwrap();
        assert_eq!(entry.calendar_id, crate::calendar::events::LOCAL_CALENDAR);
        let filed: String = setup
            .db
            .conn()
            .query_row(
                "SELECT ics FROM events WHERE id = ?1",
                [&entry.event_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(filed.contains("PARTSTAT=ACCEPTED"), "{filed}");
        assert!(filed.contains("SCHEDULE-AGENT=CLIENT"), "{filed}");
        assert!(!filed.contains("METHOD:"), "{filed}");
        // Another event at that time shows as a conflict; the invitation's own copy does not.
        crate::calendar::events::save_event(
            &setup.db.conn(),
            &crate::calendar::events::EventInput {
                id: None,
                calendar_id: crate::calendar::events::LOCAL_CALENDAR.into(),
                title: "Zahnarzt".into(),
                location: String::new(),
                description: String::new(),
                start: shown.start + 30 * 60_000,
                end: shown.start + 90 * 60_000,
                all_day: false,
                repeat: None,
                attendees: Vec::new(),
                reminder: None,
                instance: None,
            },
        )
        .unwrap();
        let shown = invitation::for_message(&setup.db.conn(), &id)
            .unwrap()
            .unwrap();
        let conflicts: Vec<_> = shown
            .conflicts
            .iter()
            .filter_map(|c| c.title.as_deref())
            .collect();
        assert_eq!(conflicts, ["Zahnarzt"]);
        // The organizer moved the event: the answer was to the version before.
        invite(&OUTLOOK.replace("SEQUENCE:2", "SEQUENCE:3"));
        let shown = invitation::for_message(&setup.db.conn(), &id)
            .unwrap()
            .unwrap();
        assert_eq!(shown.answer, Some(Answer::Accepted));
        assert!(shown.changed_since_answer);
        // Declined after all: out of the calendar again.
        let decline = InvitationAnswer {
            answer: Answer::Declined,
            ..answer
        };
        invitation::answer_with(&setup.db, &setup.secrets, &decline, None, setup.smtp.tls())
            .await
            .unwrap();
        let shown = invitation::for_message(&setup.db.conn(), &id)
            .unwrap()
            .unwrap();
        assert_eq!(shown.in_calendar, None);
    }

    #[tokio::test]
    async fn microsoft_accounts_answer_over_graph() {
        use crate::calendar::graph::Graph;
        use crate::calendar::graph_test_server::{self, TOKEN};
        use crate::calendar::invitation::{self, Answer, InvitationAnswer, tests::OUTLOOK};
        let setup = setup().await;
        let id = setup.cached_id(2);
        setup
            .db
            .conn()
            .execute(
                "UPDATE messages SET calendar = ?2 WHERE id = ?1",
                rusqlite::params![id, OUTLOOK],
            )
            .unwrap();
        let server = graph_test_server::start().await;
        let event = server.store().put(
            "cal-1",
            serde_json::json!({ "iCalUId": "040000008200E00074C5B7101A82E008", "subject": "Planung" }),
        );
        let graph = Graph::new(&server.base(), Secret::new(TOKEN)).unwrap();
        let answer = InvitationAnswer {
            message_id: id.clone(),
            answer: Answer::Tentative,
            subject: "Vielleicht: Planung".into(),
            text: "Vielleicht.".into(),
        };
        invitation::answer_with(
            &setup.db,
            &setup.secrets,
            &answer,
            Some(&graph),
            setup.smtp.tls(),
        )
        .await
        .unwrap();
        // Exchange tells the organizer; no mail goes out.
        assert_eq!(
            server.store().answers,
            [(event, "tentativelyAccept".to_owned())]
        );
        assert!(setup.smtp.mails().is_empty());
        let shown = invitation::for_message(&setup.db.conn(), &id)
            .unwrap()
            .unwrap();
        assert_eq!(shown.answer, Some(Answer::Tentative));
    }

    #[tokio::test]
    async fn files_of_the_draft_go_along() {
        let setup = setup().await;
        let mail = setup.mail(&["lena@example.org"]);
        let draft = crate::mail::drafts::Draft {
            key: "new:1".into(),
            mode: crate::mail::drafts::ComposeMode::New,
            source_id: None,
            account_id: setup.account_id.clone(),
            to: mail.to.clone(),
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: mail.subject.clone(),
            html: mail.html.clone(),
            text: mail.text.clone(),
        };
        crate::mail::drafts::save(&setup.db.conn(), &draft).unwrap();
        crate::mail::drafts::attach(&setup.db.conn(), "new:1", "Fotos.zip", b"PK..".to_vec())
            .unwrap();
        send_with(
            &setup.db,
            &setup.secrets,
            &mail,
            Some("new:1"),
            setup.smtp.tls(),
        )
        .await
        .unwrap();
        let data = &setup.smtp.mails()[0].data;
        assert!(data.contains("multipart/mixed"), "{data}");
        assert!(data.contains("Fotos.zip"));
        assert!(data.contains("application/zip"));
    }

    #[tokio::test]
    async fn refusals_say_what_to_fix() {
        let setup = setup().await;
        let send = |mail: OutgoingMail| {
            let setup = &setup;
            async move { send_with(&setup.db, &setup.secrets, &mail, None, setup.smtp.tls()).await }
        };
        assert!(matches!(
            send(setup.mail(&[])).await,
            Err(Error::NoRecipients)
        ));
        assert!(matches!(
            send(setup.mail(&["lena"])).await,
            Err(Error::InvalidRecipient(address)) if address == "lena"
        ));
        assert!(matches!(
            send(setup.mail(&["nobody@example.org"])).await,
            Err(Error::RecipientRejected(_))
        ));
        assert!(setup.smtp.mails().is_empty());

        // A wrong SMTP password keeps the settings that worked.
        let wrong = change_smtp_with(
            &setup.db,
            &setup.secrets,
            &setup.account_id,
            SmtpSettings {
                host: "localhost".into(),
                port: setup.smtp.port,
                security: Security::Tls,
                username: "alex@example.org".into(),
            },
            Some(Secret::new("wrong")),
            setup.smtp.tls(),
        )
        .await;
        assert!(matches!(
            wrong,
            Err(Error::SmtpAuthenticationFailed { provider: None })
        ));
        assert!(send(setup.mail(&["lena@example.org"])).await.is_ok());
    }
}
