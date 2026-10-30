//! What the user types into the search field: words, and filters like `from:lena`, `is:unread`
//! or `has:attachment`, in German too (`von:`, `ist:ungelesen`, `hat:anhang`). The same query
//! searches the cache (full-text index) and, on request, the servers (IMAP `SEARCH`).

/// A parsed search. Every part has to match.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchQuery {
    /// Words to find in subject, sender or text, as a word or the start of one.
    pub words: Vec<String>,
    /// Senders, by name or address (or the start of either).
    pub from: Vec<String>,
    /// `is:unread` (true) or `is:read` (false).
    pub unread: Option<bool>,
    pub flagged: bool,
    pub attachment: bool,
}

impl SearchQuery {
    pub fn parse(input: &str) -> Self {
        let mut query = Self::default();
        for token in input.split_whitespace() {
            let token = token.replace('"', "");
            if let Some((key, value)) = token.split_once(':')
                && query.apply(&key.to_lowercase(), value)
            {
                continue;
            }
            if token.chars().any(char::is_alphanumeric) {
                query.words.push(token);
            }
        }
        query
    }

    /// A known filter; false leaves the token to be searched as a word ("Re:", "10:30").
    fn apply(&mut self, key: &str, value: &str) -> bool {
        let value = value.to_lowercase();
        match (key, value.as_str()) {
            ("from" | "von", sender) if sender.chars().any(char::is_alphanumeric) => {
                self.from.push(sender.to_owned());
            }
            ("is" | "ist", "unread" | "ungelesen") => self.unread = Some(true),
            ("is" | "ist", "read" | "gelesen") => self.unread = Some(false),
            ("is" | "ist", "flagged" | "starred" | "markiert") => self.flagged = true,
            ("has" | "hat", "attachment" | "attachments" | "anhang" | "anhänge") => {
                self.attachment = true;
            }
            _ => return false,
        }
        true
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The words and senders as an FTS5 expression, `None` without either. Each word is quoted
    /// (so `-`, `OR` or `NEAR` are plain text) and matched as a prefix; a sender is matched as a
    /// phrase in the sender columns, the way the index splits it ("lena.fischer@example.org"
    /// becomes "lena fischer example org").
    pub(crate) fn fts(&self) -> Option<String> {
        let words = self.words.iter().map(|word| format!("\"{word}\"*"));
        let senders = self.from.iter().map(|sender| {
            let tokens: Vec<&str> = sender
                .split(|c: char| !c.is_alphanumeric())
                .filter(|token| !token.is_empty())
                .collect();
            format!("{{from_name from_address}} : \"{}\"*", tokens.join(" "))
        });
        let terms: Vec<String> = words.chain(senders).collect();
        (!terms.is_empty()).then(|| terms.join(" "))
    }

    /// The query as IMAP `SEARCH` criteria; `None` if nothing in it can be asked of a server (an
    /// attachment is only known once the mail is fetched). Text that is not ASCII goes as a
    /// non-synchronizing literal where the server takes those (`literals`: LITERAL+ or
    /// LITERAL-), otherwise quoted, which most servers accept after `CHARSET UTF-8`.
    pub(crate) fn imap_criteria(&self, literals: bool) -> Option<String> {
        let string = |value: &str| {
            if value.is_ascii() || !literals {
                format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
            } else {
                format!("{{{}+}}\r\n{value}", value.len())
            }
        };
        let mut parts: Vec<String> = Vec::new();
        parts.extend(
            self.words
                .iter()
                .map(|word| format!("TEXT {}", string(word))),
        );
        parts.extend(
            self.from
                .iter()
                .map(|sender| format!("FROM {}", string(sender))),
        );
        match self.unread {
            Some(true) => parts.push("UNSEEN".into()),
            Some(false) => parts.push("SEEN".into()),
            None => {}
        }
        if self.flagged {
            parts.push("FLAGGED".into());
        }
        if parts.is_empty() {
            return None;
        }
        let utf8 = self
            .words
            .iter()
            .chain(&self.from)
            .any(|value| !value.is_ascii());
        let charset = if utf8 { "CHARSET UTF-8 " } else { "" };
        Some(format!("{charset}{}", parts.join(" ")))
    }

    /// Conditions on `messages m` for the filters that are not text.
    pub(crate) fn conditions(&self) -> Vec<&'static str> {
        let mut conditions = Vec::new();
        match self.unread {
            Some(true) => conditions.push("m.is_read = 0"),
            Some(false) => conditions.push("m.is_read = 1"),
            None => {}
        }
        if self.flagged {
            conditions.push("m.is_flagged = 1");
        }
        if self.attachment {
            conditions.push("m.has_attachments = 1");
        }
        conditions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_in_english_and_german() {
        let query = SearchQuery::parse("von:lena is:unread Fotos hat:Anhang ist:markiert");
        assert_eq!(
            query,
            SearchQuery {
                words: vec!["Fotos".into()],
                from: vec!["lena".into()],
                unread: Some(true),
                flagged: true,
                attachment: true,
            }
        );
        assert_eq!(SearchQuery::parse("is:read").unread, Some(false));
        assert!(SearchQuery::parse("  ").is_empty());
    }

    #[test]
    fn unknown_filters_and_times_are_words() {
        let query = SearchQuery::parse("Re: Treffen 10:30 is:wichtig von:");
        assert_eq!(
            query.words,
            ["Re:", "Treffen", "10:30", "is:wichtig", "von:"]
        );
        assert!(query.from.is_empty());
    }

    #[test]
    fn builds_imap_criteria() {
        assert_eq!(
            SearchQuery::parse("Rechnung von:lena is:unread ist:markiert").imap_criteria(true),
            Some(r#"TEXT "Rechnung" FROM "lena" UNSEEN FLAGGED"#.into())
        );
        // Umlauts as a literal where the server takes them, quoted otherwise.
        assert_eq!(
            SearchQuery::parse("Müller").imap_criteria(true),
            Some("CHARSET UTF-8 TEXT {7+}\r\nMüller".into())
        );
        assert_eq!(
            SearchQuery::parse("Müller").imap_criteria(false),
            Some(r#"CHARSET UTF-8 TEXT "Müller""#.into())
        );
        assert_eq!(
            SearchQuery::parse(r"a\b").imap_criteria(true),
            Some(r#"TEXT "a\\b""#.into())
        );
        // Only an attachment: nothing to ask the server.
        assert_eq!(SearchQuery::parse("hat:anhang").imap_criteria(true), None);
    }

    #[test]
    fn builds_safe_full_text_queries() {
        assert_eq!(
            SearchQuery::parse("  Rech  Müller ").fts(),
            Some(r#""Rech"* "Müller"*"#.into())
        );
        assert_eq!(
            SearchQuery::parse(r#"a"b OR -x"#).fts(),
            Some(r#""ab"* "OR"* "-x"*"#.into())
        );
        assert_eq!(
            SearchQuery::parse("from:lena.fischer@example.org").fts(),
            Some(r#"{from_name from_address} : "lena fischer example org"*"#.into())
        );
        assert_eq!(SearchQuery::parse(" \" - ").fts(), None);
        assert_eq!(SearchQuery::parse("is:unread").fts(), None);
    }
}
