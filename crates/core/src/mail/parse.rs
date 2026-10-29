//! Turns raw messages (or only their headers) into what the cache stores.

use mail_parser::MessageParser;

use crate::model::Address;

const SNIPPET_LENGTH: usize = 200;
/// Inline images up to this size are cached with the body; larger ones are left out.
const INLINE_IMAGE_LIMIT: usize = 1024 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedMessage {
    pub message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Option<String>,
    pub subject: Option<String>,
    pub from: Option<Address>,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    /// From the Date header, Unix milliseconds.
    pub date_sent: Option<i64>,
    pub snippet: String,
    /// Plain text; HTML-only mail is converted. `None` when only headers were parsed.
    pub body_text: Option<String>,
    /// The original HTML of mail that has an HTML version, sanitized only when shown.
    pub body_html: Option<String>,
    /// Images the HTML refers to by `cid:`, such as logos in signatures.
    pub inline_images: Vec<InlineImage>,
    pub has_attachments: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineImage {
    /// MIME part, as mail-parser numbers them.
    pub part_id: String,
    /// Without angle brackets, as `cid:` refers to it.
    pub content_id: String,
    pub mime_type: String,
    pub data: Vec<u8>,
}

/// Parses a complete message.
pub fn parse_message(raw: &[u8]) -> ParsedMessage {
    let Some(message) = MessageParser::new().parse(raw) else {
        return ParsedMessage::default();
    };
    let body_text = message.body_text(0).map(|text| text.trim_end().to_owned());
    let snippet = message
        .body_preview(SNIPPET_LENGTH * 2)
        .map(|preview| collapse_whitespace(&preview))
        .unwrap_or_default();
    let html_parts: Vec<String> = message
        .html_body()
        .filter(|part| part.is_content_type("text", "html"))
        .filter_map(|part| part.text().map(|html| html.into_owned()))
        .collect();
    ParsedMessage {
        snippet: truncate(&snippet, SNIPPET_LENGTH),
        body_text,
        body_html: (!html_parts.is_empty()).then(|| html_parts.join("\n")),
        inline_images: inline_images(&message),
        has_attachments: message.attachments().len() > 0,
        ..parse_headers_of(&message)
    }
}

fn inline_images(message: &mail_parser::Message<'_>) -> Vec<InlineImage> {
    message
        .parts()
        .filter_map(|part| {
            let content_type = part.content_type()?;
            if !content_type.ctype().eq_ignore_ascii_case("image") {
                return None;
            }
            let content_id = part.content_id()?.trim().to_owned();
            if content_id.is_empty() || part.decoded_len() > INLINE_IMAGE_LIMIT {
                return None;
            }
            Some(InlineImage {
                part_id: part.id().to_string(),
                content_id,
                mime_type: format!(
                    "image/{}",
                    content_type
                        .subtype()
                        .unwrap_or("octet-stream")
                        .to_ascii_lowercase()
                ),
                data: part.decoded().into_owned(),
            })
        })
        .collect()
}

/// Parses only the header block, for messages too large to fetch up front.
pub fn parse_headers(raw_headers: &[u8]) -> ParsedMessage {
    let Some(message) = MessageParser::new().parse(raw_headers) else {
        return ParsedMessage::default();
    };
    let content_type = String::from_utf8_lossy(raw_headers).to_ascii_lowercase();
    ParsedMessage {
        // Without the body this is a good guess: such mail is mostly large because of files.
        has_attachments: content_type.contains("multipart/mixed"),
        ..parse_headers_of(&message)
    }
}

/// Headers plus the first part of the body, for large messages: enough for the preview line
/// and the attachment marker, while the full body is fetched when the message is opened.
pub fn parse_preview(raw_headers: &[u8], body_prefix: &[u8]) -> ParsedMessage {
    let headers = parse_headers(raw_headers);
    let mut combined = raw_headers.to_vec();
    combined.extend_from_slice(body_prefix);
    let partial = parse_message(&combined);
    ParsedMessage {
        snippet: partial.snippet,
        has_attachments: headers.has_attachments || partial.has_attachments,
        ..headers
    }
}

fn parse_headers_of(message: &mail_parser::Message<'_>) -> ParsedMessage {
    let mailbox = |mailbox: mail_parser::Mailbox<'_>| {
        mailbox.address().map(|address| Address {
            name: mailbox
                .name()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned),
            address: address.trim().to_owned(),
        })
    };
    let list = |addresses: Option<mail_parser::AddressList<'_>>| -> Vec<Address> {
        addresses
            .map(|addresses| addresses.mailboxes().filter_map(mailbox).collect())
            .unwrap_or_default()
    };
    let text_list = |values: Option<mail_parser::TextList<'_>>| {
        values
            .map(|values| values.iter().collect::<Vec<_>>().join(" "))
            .filter(|joined| !joined.is_empty())
    };

    ParsedMessage {
        message_id: message.message_id().map(str::to_owned),
        in_reply_to: text_list(message.in_reply_to()),
        references: text_list(message.references()),
        subject: message
            .subject()
            .map(str::trim)
            .filter(|subject| !subject.is_empty())
            .map(str::to_owned),
        from: message
            .from()
            .and_then(|from| from.first())
            .and_then(mailbox),
        to: list(message.to()),
        cc: list(message.cc()),
        date_sent: message.date().map(|date| date.to_timestamp() * 1000),
        ..ParsedMessage::default()
    }
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate(text: &str, max_chars: usize) -> String {
    match text.char_indices().nth(max_chars) {
        Some((index, _)) => format!("{}…", text[..index].trim_end()),
        None => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: &[u8] = b"From: \"Lena Fischer\" <lena@example.org>\r\n\
To: Alex <alex@example.org>, team@example.org\r\n\
Cc: =?UTF-8?Q?J=C3=BCrgen?= <juergen@example.org>\r\n\
Subject: =?UTF-8?Q?Wanderung_am_Samstag?=\r\n\
Date: Wed, 28 Oct 2026 10:15:00 +0100\r\n\
Message-ID: <hike@example.org>\r\n\
In-Reply-To: <plan@example.org>\r\n\
References: <start@example.org> <plan@example.org>\r\n\
\r\n\
Hi Alex,\r\n\
\r\n\
are you in for the hike?\r\n";

    #[test]
    fn reads_headers_body_and_snippet() {
        let parsed = parse_message(PLAIN);
        assert_eq!(parsed.subject.as_deref(), Some("Wanderung am Samstag"));
        assert_eq!(
            parsed.from,
            Some(Address {
                name: Some("Lena Fischer".into()),
                address: "lena@example.org".into()
            })
        );
        assert_eq!(parsed.to.len(), 2);
        assert_eq!(parsed.to[1].name, None);
        assert_eq!(parsed.cc[0].name.as_deref(), Some("Jürgen"));
        assert_eq!(parsed.message_id.as_deref(), Some("hike@example.org"));
        assert_eq!(parsed.in_reply_to.as_deref(), Some("plan@example.org"));
        assert_eq!(
            parsed.references.as_deref(),
            Some("start@example.org plan@example.org")
        );
        assert_eq!(parsed.date_sent, Some(1_793_178_900_000));
        assert_eq!(parsed.snippet, "Hi Alex, are you in for the hike?");
        assert!(parsed.body_text.unwrap().ends_with("for the hike?"));
        assert!(!parsed.has_attachments);
    }

    #[test]
    fn html_only_mail_gets_a_text_body() {
        let raw = b"From: news@example.org\r\nSubject: News\r\nContent-Type: text/html\r\n\r\n\
<html><body><h1>Autumn</h1><p>Five recipes &amp; more</p><script>x()</script></body></html>\r\n";
        let parsed = parse_message(raw);
        let body = parsed.body_text.unwrap();
        assert!(body.contains("Autumn"));
        assert!(body.contains("Five recipes & more"));
        assert!(!body.contains("<p>"));
    }

    #[test]
    fn detects_attachments() {
        let raw = b"From: a@example.org\r\nSubject: Files\r\n\
Content-Type: multipart/mixed; boundary=\"b\"\r\n\r\n\
--b\r\nContent-Type: text/plain\r\n\r\nSee attached.\r\n\
--b\r\nContent-Type: application/pdf\r\nContent-Disposition: attachment; filename=\"invoice.pdf\"\r\n\
Content-Transfer-Encoding: base64\r\n\r\nJVBERi0=\r\n--b--\r\n";
        let parsed = parse_message(raw);
        assert!(parsed.has_attachments);
        assert_eq!(parsed.snippet, "See attached.");
    }

    #[test]
    fn headers_only_leave_the_body_empty() {
        let raw = b"From: a@example.org\r\nSubject: Big\r\nContent-Type: multipart/mixed; boundary=x\r\n\r\n";
        let parsed = parse_headers(raw);
        assert_eq!(parsed.subject.as_deref(), Some("Big"));
        assert!(parsed.body_text.is_none());
        assert!(parsed.has_attachments);
    }

    #[test]
    fn long_previews_are_cut_on_a_character_boundary() {
        let text = "ä".repeat(300);
        let cut = truncate(&text, 200);
        assert!(cut.ends_with('…'));
        assert_eq!(cut.chars().count(), 201);
    }
}
