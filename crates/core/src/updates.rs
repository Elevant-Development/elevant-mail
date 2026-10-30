//! Whether a newer version was released: one request to GitHub's API for the latest published
//! release, carrying nothing about the user. The frontend asks only while the setting allows it.

use std::sync::Arc;
use std::time::Duration;

use rustls::ClientConfig;
use rustls_platform_verifier::BuilderVerifierExt;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Error, Result};

const LATEST: &str =
    "https://api.github.com/repos/Elevant-Development/elevant-mail/releases/latest";
/// Where the user downloads it; fixed, not taken from the answer.
pub const RELEASES: &str = "https://github.com/Elevant-Development/elevant-mail/releases/latest";

/// A version newer than this one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Update {
    /// Like "0.5.0".
    pub version: String,
    /// The release page to download it from.
    pub url: String,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// The newest published release, if it is newer than this build.
pub async fn check() -> Result<Option<Update>> {
    let failed = |error: reqwest::Error| Error::ConnectionFailed(std::io::Error::other(error));
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let tls = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .and_then(BuilderVerifierExt::with_platform_verifier)
        .map_err(Error::TlsSetup)?
        .with_no_client_auth();
    let client = reqwest::Client::builder()
        .tls_backend_preconfigured(tls)
        .timeout(Duration::from_secs(15))
        // GitHub wants a user agent; the name alone, no version or system.
        .user_agent("Elevant-Mail")
        .build()
        .map_err(failed)?;
    let response = client
        .get(LATEST)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(failed)?;
    // No release published yet.
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let release: Release = response
        .error_for_status()
        .map_err(failed)?
        .json()
        .await
        .map_err(failed)?;
    Ok(newer(&release, env!("CARGO_PKG_VERSION")))
}

fn newer(release: &Release, current: &str) -> Option<Update> {
    if release.draft || release.prerelease {
        return None;
    }
    let found = version(&release.tag_name)?;
    (found > version(current)?).then(|| Update {
        version: format!("{}.{}.{}", found.0, found.1, found.2),
        url: RELEASES.to_owned(),
    })
}

/// "v0.5.0" or "0.5.0" → (0, 5, 0).
fn version(text: &str) -> Option<(u64, u64, u64)> {
    let mut parts = text.trim().trim_start_matches('v').split('.');
    let mut next = || parts.next()?.parse().ok();
    Some((next()?, next()?, next()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str) -> Release {
        Release {
            tag_name: tag.into(),
            draft: false,
            prerelease: false,
        }
    }

    #[test]
    fn only_a_newer_published_release_is_an_update() {
        let update = newer(&release("v0.5.0"), "0.4.0").unwrap();
        assert_eq!(update.version, "0.5.0");
        assert_eq!(update.url, RELEASES);
        assert_eq!(
            newer(&release("v0.10.0"), "0.9.3").unwrap().version,
            "0.10.0"
        );
        assert!(newer(&release("v0.4.0"), "0.4.0").is_none());
        assert!(newer(&release("v0.3.9"), "0.4.0").is_none());
        assert!(newer(&release("nightly"), "0.4.0").is_none());
        let draft = Release {
            draft: true,
            ..release("v1.0.0")
        };
        assert!(newer(&draft, "0.4.0").is_none());
    }
}
