//! Suggests server settings from an email address: known providers by domain, then the
//! domain's MX record (self-hosted servers usually receive and serve mail on the same host),
//! then a guess. Only the user's own DNS is asked, no third-party lookup service.

use std::time::Duration;

use hickory_resolver::TokioResolver;
use hickory_resolver::proto::rr::{RData, RecordType};
use serde::Serialize;
use ts_rs::TS;

use crate::mail::settings::{ImapSettings, Provider, Security, SmtpSettings};
use crate::{Error, Result};

const MX_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SuggestionSource {
    /// A provider we know.
    Preset,
    /// Derived from the domain's mail server.
    MxRecord,
    /// `imap.<domain>`, to be checked by the user.
    Guess,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SuggestedSettings {
    pub imap: ImapSettings,
    pub source: SuggestionSource,
    pub provider: Option<Provider>,
    /// The provider accepts no passwords (Microsoft); sign-in comes with OAuth.
    pub needs_oauth: bool,
}

struct Preset {
    host: &'static str,
    provider: Option<Provider>,
}

fn preset_for_domain(domain: &str) -> Option<Preset> {
    let (host, provider) = match domain {
        "gmail.com" | "googlemail.com" => ("imap.gmail.com", Some(Provider::Gmail)),
        "outlook.com" | "outlook.de" | "hotmail.com" | "hotmail.de" | "live.com" | "live.de"
        | "msn.com" => ("outlook.office365.com", Some(Provider::Microsoft)),
        "web.de" => ("imap.web.de", Some(Provider::WebDe)),
        "gmx.de" | "gmx.net" | "gmx.at" | "gmx.ch" | "gmx.com" => {
            ("imap.gmx.net", Some(Provider::Gmx))
        }
        "icloud.com" | "me.com" | "mac.com" => ("imap.mail.me.com", Some(Provider::Icloud)),
        "yahoo.com" | "yahoo.de" => ("imap.mail.yahoo.com", Some(Provider::Yahoo)),
        "posteo.de" | "posteo.net" => ("posteo.de", None),
        "mailbox.org" => ("imap.mailbox.org", None),
        "t-online.de" | "magenta.de" => ("secureimap.t-online.de", None),
        _ => return None,
    };
    Some(Preset { host, provider })
}

/// Hosted mail whose MX host is not its IMAP host.
fn preset_for_mx(mx_host: &str) -> Option<Preset> {
    let ends = |suffix: &str| mx_host.ends_with(suffix);
    let (host, provider) = if ends(".google.com") || ends(".googlemail.com") {
        ("imap.gmail.com", Some(Provider::Gmail))
    } else if ends(".mail.protection.outlook.com") {
        ("outlook.office365.com", Some(Provider::Microsoft))
    } else if ends(".web.de") {
        ("imap.web.de", Some(Provider::WebDe))
    } else if ends(".gmx.net") {
        ("imap.gmx.net", Some(Provider::Gmx))
    } else if ends(".mail.icloud.com") {
        ("imap.mail.me.com", Some(Provider::Icloud))
    } else if ends(".ionos.de") || ends(".kundenserver.de") {
        ("imap.ionos.de", None)
    } else if ends(".rzone.de") {
        ("imap.strato.de", None)
    } else if ends(".mailbox.org") {
        ("imap.mailbox.org", None)
    } else {
        return None;
    };
    Some(Preset { host, provider })
}

fn suggestion(
    host: &str,
    email: &str,
    source: SuggestionSource,
    provider: Option<Provider>,
) -> SuggestedSettings {
    SuggestedSettings {
        imap: ImapSettings {
            host: host.to_owned(),
            port: 993,
            security: Security::Tls,
            username: email.to_owned(),
        },
        source,
        provider,
        needs_oauth: provider.is_some_and(Provider::needs_oauth),
    }
}

/// The outgoing server that goes with an account's incoming one: known providers by their IMAP
/// host, otherwise `smtp.` in place of `imap.` (or the same host, as on self-hosted servers), with
/// TLS on port 465. The user name is the IMAP one.
pub fn smtp_for(imap: &ImapSettings, email: &str) -> SmtpSettings {
    let imap_host = imap.host.trim().trim_end_matches('.').to_ascii_lowercase();
    let consumer_microsoft = email_domain(email).is_ok_and(|domain| {
        let name = domain.split('.').next().unwrap_or_default();
        matches!(name, "outlook" | "hotmail" | "live" | "msn")
    });
    let (host, port, security) = match Provider::from_imap_host(&imap_host) {
        Some(Provider::Gmail) => ("smtp.gmail.com".to_owned(), 465, Security::Tls),
        Some(Provider::Microsoft) if consumer_microsoft => {
            ("smtp-mail.outlook.com".to_owned(), 587, Security::StartTls)
        }
        Some(Provider::Microsoft) => ("smtp.office365.com".to_owned(), 587, Security::StartTls),
        Some(Provider::WebDe) => ("smtp.web.de".to_owned(), 587, Security::StartTls),
        Some(Provider::Gmx) => ("mail.gmx.net".to_owned(), 587, Security::StartTls),
        Some(Provider::Icloud) => ("smtp.mail.me.com".to_owned(), 587, Security::StartTls),
        Some(Provider::Yahoo) => ("smtp.mail.yahoo.com".to_owned(), 465, Security::Tls),
        None => {
            let host = match imap_host.as_str() {
                "secureimap.t-online.de" => "securesmtp.t-online.de".to_owned(),
                other => match other.strip_prefix("imap.") {
                    Some(rest) => format!("smtp.{rest}"),
                    None => other.to_owned(),
                },
            };
            (host, 465, Security::Tls)
        }
    };
    SmtpSettings {
        host,
        port,
        security,
        username: imap.username.trim().to_owned(),
    }
}

pub fn email_domain(email: &str) -> Result<String> {
    let (local, domain) = email.trim().rsplit_once('@').ok_or(Error::InvalidEmail)?;
    let domain = domain.trim().trim_end_matches('.').to_ascii_lowercase();
    if local.is_empty() || domain.is_empty() || !domain.contains('.') {
        return Err(Error::InvalidEmail);
    }
    Ok(domain)
}

pub async fn suggest_settings(email: &str) -> Result<SuggestedSettings> {
    let email = email.trim();
    let domain = email_domain(email)?;
    if let Some(preset) = preset_for_domain(&domain) {
        return Ok(suggestion(
            preset.host,
            email,
            SuggestionSource::Preset,
            preset.provider,
        ));
    }
    match mx_host(&domain).await {
        Some(mx) => Ok(match preset_for_mx(&mx) {
            Some(preset) => suggestion(
                preset.host,
                email,
                SuggestionSource::Preset,
                preset.provider,
            ),
            None => suggestion(&mx, email, SuggestionSource::MxRecord, None),
        }),
        None => Ok(suggestion(
            &format!("imap.{domain}"),
            email,
            SuggestionSource::Guess,
            None,
        )),
    }
}

/// The preferred MX host of a domain, without the trailing dot. None if DNS has no answer.
async fn mx_host(domain: &str) -> Option<String> {
    let resolver = TokioResolver::builder_tokio().ok()?.build().ok()?;
    let lookup = tokio::time::timeout(MX_TIMEOUT, resolver.lookup(domain, RecordType::MX))
        .await
        .ok()?
        .ok()?;
    lookup
        .answers()
        .iter()
        .filter_map(|record| match &record.data {
            RData::MX(mx) => Some((mx.preference, mx.exchange.to_utf8())),
            _ => None,
        })
        .min_by_key(|(preference, _)| *preference)
        .map(|(_, host)| host.trim_end_matches('.').to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn known_domains_use_presets_without_dns() {
        let gmail = suggest_settings("alex@gmail.com").await.unwrap();
        assert_eq!(gmail.imap.host, "imap.gmail.com");
        assert_eq!(gmail.imap.port, 993);
        assert_eq!(gmail.imap.security, Security::Tls);
        assert_eq!(gmail.imap.username, "alex@gmail.com");
        assert_eq!(gmail.provider, Some(Provider::Gmail));
        assert!(!gmail.needs_oauth);

        let outlook = suggest_settings(" Alex@Hotmail.de ").await.unwrap();
        assert_eq!(outlook.imap.host, "outlook.office365.com");
        assert!(outlook.needs_oauth);
    }

    #[test]
    fn outgoing_servers_follow_the_incoming_one() {
        let smtp = |host: &str, email: &str| {
            let imap = ImapSettings {
                host: host.into(),
                port: 993,
                security: Security::Tls,
                username: email.into(),
            };
            let smtp = smtp_for(&imap, email);
            assert_eq!(smtp.username, email);
            (smtp.host, smtp.port, smtp.security)
        };
        assert_eq!(
            smtp("imap.gmail.com", "alex@gmail.com"),
            ("smtp.gmail.com".into(), 465, Security::Tls)
        );
        // Microsoft: Outlook.com and Microsoft 365 share the IMAP host, not the SMTP one.
        assert_eq!(
            smtp("outlook.office365.com", "alex@hotmail.de"),
            ("smtp-mail.outlook.com".into(), 587, Security::StartTls)
        );
        assert_eq!(
            smtp("outlook.office365.com", "alex@firma.example"),
            ("smtp.office365.com".into(), 587, Security::StartTls)
        );
        assert_eq!(
            smtp("imap.web.de", "alex@web.de"),
            ("smtp.web.de".into(), 587, Security::StartTls)
        );
        assert_eq!(
            smtp("imap.example.org", "alex@example.org"),
            ("smtp.example.org".into(), 465, Security::Tls)
        );
        // Self-hosted: one host for both.
        assert_eq!(
            smtp("mail.example.org", "alex@example.org"),
            ("mail.example.org".into(), 465, Security::Tls)
        );
    }

    #[test]
    fn hosted_mail_is_recognised_by_its_mx() {
        assert_eq!(
            preset_for_mx("example-com.mail.protection.outlook.com")
                .unwrap()
                .host,
            "outlook.office365.com"
        );
        assert_eq!(
            preset_for_mx("aspmx.l.google.com").unwrap().host,
            "imap.gmail.com"
        );
        assert!(preset_for_mx("mail.example.org").is_none());
    }

    #[test]
    fn rejects_malformed_addresses() {
        for email in ["", "alex", "@example.org", "alex@", "alex@localhost"] {
            assert!(
                matches!(email_domain(email), Err(Error::InvalidEmail)),
                "{email}"
            );
        }
        assert_eq!(email_domain("a@Example.ORG.").unwrap(), "example.org");
    }
}
