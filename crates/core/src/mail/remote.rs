//! External images of a mail, fetched only after the user allowed them for that mail. The app
//! serves them to the mail frame itself, so the frame never talks to the web.

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use reqwest::{Client, Url, redirect};
use rustls::ClientConfig;
use rustls_platform_verifier::BuilderVerifierExt;

const TIMEOUT: Duration = Duration::from_secs(20);
const SIZE_LIMIT: usize = 10 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;

#[derive(Debug, thiserror::Error)]
pub enum RemoteImageError {
    #[error("not an http(s) URL on a public host")]
    NotAllowed,
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("server answered {0}")]
    Status(u16),
    #[error("not an image")]
    NotAnImage,
    #[error("larger than the limit")]
    TooLarge,
    #[error("TLS setup failed: {0}")]
    Tls(#[from] rustls::Error),
}

pub struct RemoteImage {
    pub mime_type: String,
    pub data: Vec<u8>,
}

/// One client for the whole app: no cookies, no referrer, a neutral user agent.
pub struct RemoteImages {
    client: Client,
}

impl RemoteImages {
    pub fn new() -> Result<Self, RemoteImageError> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let tls = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()?
            .with_platform_verifier()?
            .with_no_client_auth();
        let client = Client::builder()
            .tls_backend_preconfigured(tls)
            .user_agent("Mozilla/5.0")
            .referer(false)
            .timeout(TIMEOUT)
            .redirect(redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= MAX_REDIRECTS || !is_public_web_url(attempt.url()) {
                    attempt.stop()
                } else {
                    attempt.follow()
                }
            }))
            .build()?;
        Ok(Self { client })
    }

    pub async fn fetch(&self, url: &str) -> Result<RemoteImage, RemoteImageError> {
        let url = Url::parse(url).map_err(|_| RemoteImageError::NotAllowed)?;
        if !is_public_web_url(&url) {
            return Err(RemoteImageError::NotAllowed);
        }
        let mut response = self.client.get(url).send().await?;
        if !response.status().is_success() {
            return Err(RemoteImageError::Status(response.status().as_u16()));
        }
        let mime_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| value.starts_with("image/"))
            .ok_or(RemoteImageError::NotAnImage)?;

        let mut data = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if data.len() + chunk.len() > SIZE_LIMIT {
                return Err(RemoteImageError::TooLarge);
            }
            data.extend_from_slice(&chunk);
        }
        Ok(RemoteImage { mime_type, data })
    }
}

/// Mail must not make the app reach into the local network or the machine itself.
fn is_public_web_url(url: &Url) -> bool {
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    match url.host() {
        Some(url::Host::Domain(domain)) => {
            let domain = domain.trim_end_matches('.').to_ascii_lowercase();
            domain != "localhost" && !domain.ends_with(".localhost") && domain.contains('.')
        }
        Some(url::Host::Ipv4(ip)) => is_public_ip(IpAddr::V4(ip)),
        Some(url::Host::Ipv6(ip)) => is_public_ip(IpAddr::V6(ip)),
        None => false,
    }
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || ip.is_documentation()
                // Carrier-grade NAT, 100.64.0.0/10 (Tailscale lives here).
                || (ip.octets()[0] == 100 && (ip.octets()[1] & 0xc0) == 64))
        }
        IpAddr::V6(ip) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                return is_public_ip(IpAddr::V4(v4));
            }
            let first = ip.segments()[0];
            !(ip.is_loopback()
                || ip.is_unspecified()
                // Unique local fc00::/7 and link-local fe80::/10.
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed(url: &str) -> bool {
        is_public_web_url(&Url::parse(url).unwrap())
    }

    #[test]
    fn only_public_web_hosts() {
        assert!(allowed("https://cdn.example.org/logo.png"));
        assert!(allowed("http://203.0.114.7/a.gif"));
        for blocked in [
            "http://localhost/x",
            "http://mail-asset.localhost/remote/m/0",
            "http://127.0.0.1:8080/",
            "http://192.168.1.1/router.png",
            "http://10.0.0.5/",
            "http://100.64.0.1/",
            "http://169.254.169.254/latest/meta-data",
            "http://[::1]/",
            "http://[fd00::1]/",
            "http://[::ffff:127.0.0.1]/",
            "http://intranet/",
            "file:///etc/passwd",
            "ftp://example.org/a.png",
        ] {
            assert!(!allowed(blocked), "{blocked} was allowed");
        }
    }

    #[tokio::test]
    async fn refuses_local_addresses_without_connecting() {
        let images = RemoteImages::new().unwrap();
        assert!(matches!(
            images.fetch("http://127.0.0.1:1/pixel.gif").await,
            Err(RemoteImageError::NotAllowed)
        ));
    }
}
