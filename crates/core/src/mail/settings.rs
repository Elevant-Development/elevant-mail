//! Server settings of an account and the providers we know by name.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Plain-text IMAP is not supported: either TLS from the start or STARTTLS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Security {
    Tls,
    StartTls,
}

impl Security {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Security::Tls => "tls",
            Security::StartTls => "starttls",
        }
    }

    pub fn from_db_str(value: &str) -> Self {
        if value == "starttls" {
            Security::StartTls
        } else {
            Security::Tls
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImapSettings {
    pub host: String,
    pub port: u16,
    pub security: Security,
    pub username: String,
}

/// Providers with known quirks, used for presets and for helpful error hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Provider {
    Gmail,
    Microsoft,
    WebDe,
    Gmx,
    Icloud,
    Yahoo,
}

impl Provider {
    /// Recognises a provider from an IMAP host the user typed or we suggested.
    pub fn from_imap_host(host: &str) -> Option<Self> {
        let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
        let is = |suffix: &str| host == suffix || host.ends_with(&format!(".{suffix}"));
        if is("gmail.com") || is("googlemail.com") {
            Some(Provider::Gmail)
        } else if is("office365.com") || is("outlook.com") {
            Some(Provider::Microsoft)
        } else if is("web.de") {
            Some(Provider::WebDe)
        } else if is("gmx.net") || is("gmx.com") {
            Some(Provider::Gmx)
        } else if is("mail.me.com") {
            Some(Provider::Icloud)
        } else if is("yahoo.com") {
            Some(Provider::Yahoo)
        } else {
            None
        }
    }

    /// Exchange Online and Outlook.com accept no passwords for IMAP any more.
    pub fn needs_oauth(self) -> bool {
        self == Provider::Microsoft
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_providers_from_host() {
        assert_eq!(
            Provider::from_imap_host("imap.gmail.com"),
            Some(Provider::Gmail)
        );
        assert_eq!(
            Provider::from_imap_host("IMAP.web.de."),
            Some(Provider::WebDe)
        );
        assert_eq!(
            Provider::from_imap_host("outlook.office365.com"),
            Some(Provider::Microsoft)
        );
        assert_eq!(Provider::from_imap_host("mail.example.org"), None);
        // A look-alike domain is not the provider.
        assert_eq!(Provider::from_imap_host("evilweb.de"), None);
    }

    #[test]
    fn security_round_trips_through_the_database_value() {
        for security in [Security::Tls, Security::StartTls] {
            assert_eq!(Security::from_db_str(security.as_db_str()), security);
        }
    }
}
