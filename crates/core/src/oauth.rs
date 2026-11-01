//! Sign-in with Microsoft for Microsoft 365 and Outlook.com mail, which take no passwords over
//! IMAP. OAuth 2.0 authorization code flow with PKCE for a public client (RFC 7636): the system
//! browser shows Microsoft's sign-in, which redirects to a one-time listener on the loopback
//! interface (RFC 8252). The refresh token goes into the keyring; access tokens live only in
//! memory, for about an hour each.

use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::digest::{SHA256, digest};
use ring::rand::{SecureRandom, SystemRandom};
use rustls::ClientConfig;
use rustls_platform_verifier::BuilderVerifierExt;
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use url::Url;

use crate::ids::now_millis;
use crate::secrets::{Secret, SecretKind, SecretStore};
use crate::{Error, Result};

/// Registered as a multi-tenant public client in Elevant's Entra ID tenant. Not a secret:
/// desktop apps cannot keep one, PKCE protects the flow instead.
pub const MICROSOFT_CLIENT_ID: &str = "430b3404-3b08-41df-a2f6-23e5375d9bd7";

/// `common`: work and school accounts of any tenant as well as personal Outlook.com accounts.
const MICROSOFT_AUTHORIZE: &str = "https://login.microsoftonline.com/common/oauth2/v2.0/authorize";
const MICROSOFT_TOKEN: &str = "https://login.microsoftonline.com/common/oauth2/v2.0/token";

/// Mail over IMAP, sending (phase 4) so it needs no second consent later, a refresh token, and
/// the signed-in address.
const MICROSOFT_SCOPES: &str = "https://outlook.office.com/IMAP.AccessAsUser.All \
https://outlook.office.com/SMTP.Send offline_access openid email";

/// Calendars over Microsoft Graph. Asked for at sign-in, so one consent covers mail and calendar;
/// a token for it comes from the same refresh token ([`graph_token`]), since one token serves
/// only one resource.
pub const GRAPH_CALENDARS: &str = "https://graph.microsoft.com/Calendars.ReadWrite";
/// Contacts over Microsoft Graph, asked for at sign-in like the calendars.
pub const GRAPH_CONTACTS: &str = "https://graph.microsoft.com/Contacts.ReadWrite";

/// How long the browser sign-in may take before the app stops waiting.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const TOKEN_TIMEOUT: Duration = Duration::from_secs(30);
/// Access tokens are renewed this long before they expire.
const EXPIRY_MARGIN_MS: i64 = 5 * 60 * 1000;

/// Where to sign in; tests point this at a local stand-in.
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub client_id: String,
    pub authorize_url: String,
    pub token_url: String,
    /// What the tokens of [`access_token`] are for: mail.
    pub scopes: String,
    /// Asked for consent at sign-in as well; tokens for these come with [`token_for`].
    pub extra_consent: String,
}

impl Endpoints {
    pub fn microsoft() -> Self {
        Self {
            client_id: MICROSOFT_CLIENT_ID.into(),
            authorize_url: MICROSOFT_AUTHORIZE.into(),
            token_url: MICROSOFT_TOKEN.into(),
            scopes: MICROSOFT_SCOPES.into(),
            extra_consent: format!("{GRAPH_CALENDARS} {GRAPH_CONTACTS}"),
        }
    }
}

pub struct Tokens {
    pub access_token: Secret,
    /// Microsoft hands out a new one with every refresh; the old one stops working later.
    pub refresh_token: Option<Secret>,
    /// Unix milliseconds.
    pub expires_at: i64,
    /// The address the user actually signed in with, from the ID token.
    pub user: Option<String>,
}

/// Opens the sign-in in the browser (through `open_browser`) and waits for its answer, until
/// `cancel` completes or the time runs out.
pub async fn sign_in(
    endpoints: &Endpoints,
    login_hint: &str,
    open_browser: impl FnOnce(&str) -> Result<()>,
    cancel: impl Future<Output = ()>,
) -> Result<Tokens> {
    if endpoints.client_id.trim_matches(['0', '-']).is_empty() {
        return Err(Error::SignInFailed("no client id configured".into()));
    }
    let verifier = random_token(32)?;
    let challenge = URL_SAFE_NO_PAD.encode(digest(&SHA256, verifier.as_bytes()));
    let state = random_token(16)?;

    // Browsers may resolve "localhost" to either address; both listen on the same port.
    let ipv4 = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(Error::Io)?;
    let port = ipv4.local_addr().map_err(Error::Io)?.port();
    let ipv6 = TcpListener::bind((Ipv6Addr::LOCALHOST, port)).await.ok();
    let redirect_uri = format!("http://localhost:{port}");

    let mut url = Url::parse(&endpoints.authorize_url).map_err(|_| Error::InvalidLink)?;
    url.query_pairs_mut()
        .append_pair("client_id", &endpoints.client_id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("response_mode", "query")
        .append_pair(
            "scope",
            format!("{} {}", endpoints.scopes, endpoints.extra_consent).trim(),
        )
        .append_pair("state", &state)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("login_hint", login_hint)
        .append_pair("prompt", "select_account");
    open_browser(url.as_str())?;

    let answer = tokio::select! {
        answer = wait_for_redirect(&ipv4, ipv6.as_ref(), &state) => answer?,
        () = cancel => return Err(Error::SignInCancelled),
        () = tokio::time::sleep(SIGN_IN_TIMEOUT) => return Err(Error::Timeout),
    };
    let form = [
        ("client_id", endpoints.client_id.as_str()),
        ("grant_type", "authorization_code"),
        ("code", answer.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
        ("code_verifier", verifier.as_str()),
        ("scope", endpoints.scopes.as_str()),
    ];
    request_tokens(&endpoints.token_url, &form).await
}

/// A new access token from a refresh token, for mail.
pub async fn refresh(endpoints: &Endpoints, refresh_token: &Secret) -> Result<Tokens> {
    refresh_for(endpoints, refresh_token, &endpoints.scopes).await
}

async fn refresh_for(endpoints: &Endpoints, refresh_token: &Secret, scope: &str) -> Result<Tokens> {
    let form = [
        ("client_id", endpoints.client_id.as_str()),
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token.expose()),
        ("scope", scope),
    ];
    request_tokens(&endpoints.token_url, &form).await
}

/// The cached access token of an account for mail, renewed with its refresh token when it is
/// (almost) expired. A rotated refresh token replaces the old one in the keyring.
pub async fn access_token(secrets: &dyn SecretStore, account_id: &str) -> Result<Secret> {
    access_token_with(&Endpoints::microsoft(), secrets, account_id).await
}

/// The access token for the account's calendars over Microsoft Graph.
/// [`Error::CalendarAccessMissing`] where the account signed in before the app asked for them.
pub async fn graph_token(secrets: &dyn SecretStore, account_id: &str) -> Result<Secret> {
    token_for(
        &Endpoints::microsoft(),
        secrets,
        account_id,
        GRAPH_CALENDARS,
    )
    .await
}

/// The access token for the account's contacts over Microsoft Graph.
/// [`Error::ContactsAccessMissing`] where the account signed in before the app asked for them.
pub async fn graph_contacts_token(secrets: &dyn SecretStore, account_id: &str) -> Result<Secret> {
    token_for(&Endpoints::microsoft(), secrets, account_id, GRAPH_CONTACTS).await
}

pub(crate) async fn access_token_with(
    endpoints: &Endpoints,
    secrets: &dyn SecretStore,
    account_id: &str,
) -> Result<Secret> {
    token_for(endpoints, secrets, account_id, &endpoints.scopes.clone()).await
}

/// An access token for `scope`, cached per account and scope.
pub(crate) async fn token_for(
    endpoints: &Endpoints,
    secrets: &dyn SecretStore,
    account_id: &str,
    scope: &str,
) -> Result<Secret> {
    let key = cache_key(endpoints, account_id, scope);
    if let Some(token) = cached(&key) {
        return Ok(token);
    }
    let refresh_token = secrets
        .get(account_id, SecretKind::OAuthRefreshToken)
        .map_err(|error| match error {
            Error::SecretNotFound => Error::SignInExpired,
            other => other,
        })?;
    let tokens = refresh_for(endpoints, &refresh_token, scope)
        .await
        .map_err(|error| match error {
            // Signed in before the app asked for this: allowed with the next sign-in.
            Error::ConsentRequired if scope == GRAPH_CONTACTS => Error::ContactsAccessMissing,
            Error::ConsentRequired if scope != endpoints.scopes => Error::CalendarAccessMissing,
            other => other,
        })?;
    if let Some(rotated) = &tokens.refresh_token {
        secrets.set(account_id, SecretKind::OAuthRefreshToken, rotated)?;
    }
    remember(&key, &tokens);
    Ok(tokens.access_token)
}

/// Mail's tokens are kept under the account's id, others under the id and their scope.
fn cache_key(endpoints: &Endpoints, account_id: &str, scope: &str) -> String {
    if scope == endpoints.scopes {
        account_id.to_owned()
    } else {
        format!("{account_id} {scope}")
    }
}

type TokenCache = Mutex<HashMap<String, (Arc<Secret>, i64)>>;
static ACCESS_TOKENS: LazyLock<TokenCache> = LazyLock::new(Mutex::default);

/// Keeps an access token for the next sessions of the account.
pub fn remember(account_id: &str, tokens: &Tokens) {
    ACCESS_TOKENS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(
            account_id.to_owned(),
            (
                Arc::new(Secret::new(tokens.access_token.expose())),
                tokens.expires_at,
            ),
        );
}

/// Drops the account's tokens, for mail and the rest.
pub fn forget(account_id: &str) {
    let prefix = format!("{account_id} ");
    ACCESS_TOKENS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .retain(|key, _| key != account_id && !key.starts_with(&prefix));
}

fn cached(account_id: &str) -> Option<Secret> {
    let cache = ACCESS_TOKENS.lock().unwrap_or_else(PoisonError::into_inner);
    let (token, expires_at) = cache.get(account_id)?;
    (now_millis() + EXPIRY_MARGIN_MS < *expires_at).then(|| Secret::new(token.expose()))
}

fn random_token(bytes: usize) -> Result<String> {
    let mut buffer = vec![0u8; bytes];
    SystemRandom::new()
        .fill(&mut buffer)
        .map_err(|_| Error::SignInFailed("no randomness from the system".into()))?;
    Ok(URL_SAFE_NO_PAD.encode(buffer))
}

/// Answers the browser's redirect and returns the authorization code. Other requests (a
/// favicon, a stray tab) get a 404 and the wait goes on.
async fn wait_for_redirect(
    ipv4: &TcpListener,
    ipv6: Option<&TcpListener>,
    expected_state: &str,
) -> Result<String> {
    loop {
        let (mut stream, _) = match ipv6 {
            Some(ipv6) => tokio::select! {
                accepted = ipv4.accept() => accepted,
                accepted = ipv6.accept() => accepted,
            },
            None => ipv4.accept().await,
        }
        .map_err(Error::Io)?;
        let Some(target) = read_request_target(&mut stream).await else {
            continue;
        };
        let Ok(url) = Url::parse(&format!("http://localhost{target}")) else {
            continue;
        };
        if url.path() != "/" {
            respond(&mut stream, "404 Not Found", "").await;
            continue;
        }
        let params: HashMap<String, String> = url.query_pairs().into_owned().collect();
        if params.get("state").map(String::as_str) != Some(expected_state) {
            respond(&mut stream, "400 Bad Request", PAGE_FAILED).await;
            continue;
        }
        if let Some(error) = params.get("error") {
            respond(&mut stream, "200 OK", PAGE_FAILED).await;
            let description = params.get("error_description").cloned().unwrap_or_default();
            return Err(authorize_error(error, &description));
        }
        let Some(code) = params.get("code") else {
            respond(&mut stream, "400 Bad Request", PAGE_FAILED).await;
            continue;
        };
        respond(&mut stream, "200 OK", PAGE_DONE).await;
        return Ok(code.clone());
    }
}

// Shown in the user's browser, which may not use the app's language: both languages.
const PAGE_DONE: &str = "<!doctype html><meta charset=utf-8><title>Elevant Mail</title>\
<body style=\"font-family:sans-serif;padding:3rem\"><h1>Signed in</h1>\
<p>You can close this window and return to Elevant Mail.</p>\
<p lang=de>Anmeldung abgeschlossen. Du kannst dieses Fenster schließen und zu Elevant Mail \
zurückkehren.</p>";
const PAGE_FAILED: &str = "<!doctype html><meta charset=utf-8><title>Elevant Mail</title>\
<body style=\"font-family:sans-serif;padding:3rem\"><h1>Sign-in not completed</h1>\
<p>Elevant Mail shows what happened. You can close this window.</p>\
<p lang=de>Anmeldung nicht abgeschlossen. Elevant Mail zeigt, was passiert ist. Du kannst \
dieses Fenster schließen.</p>";

async fn read_request_target(stream: &mut TcpStream) -> Option<String> {
    let mut buffer = vec![0u8; 8192];
    let mut filled = 0;
    while filled < buffer.len() {
        let read =
            tokio::time::timeout(Duration::from_secs(10), stream.read(&mut buffer[filled..]))
                .await
                .ok()?
                .ok()?;
        if read == 0 {
            break;
        }
        filled += read;
        if buffer[..filled].windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let request = String::from_utf8_lossy(&buffer[..filled]);
    let mut parts = request.lines().next()?.split(' ');
    (parts.next()? == "GET").then(|| parts.next().map(str::to_owned))?
}

async fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
Cache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

fn authorize_error(error: &str, description: &str) -> Error {
    // AADSTS65001: no consent; AADSTS90094/90095: the tenant only lets admins consent.
    if error == "consent_required"
        || ["AADSTS65001", "AADSTS90094", "AADSTS90095"]
            .iter()
            .any(|code| description.contains(code))
    {
        Error::ConsentRequired
    } else if error == "access_denied" {
        Error::SignInCancelled
    } else {
        Error::SignInFailed(format!("{error}: {description}"))
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    id_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

async fn request_tokens(token_url: &str, form: &[(&str, &str)]) -> Result<Tokens> {
    let response = http_client()?
        .post(token_url)
        .form(form)
        .send()
        .await
        .map_err(|error| Error::SignInFailed(error.to_string()))?;
    let body: TokenResponse = response
        .json()
        .await
        .map_err(|error| Error::SignInFailed(error.to_string()))?;
    if let Some(error) = body.error {
        let description = body.error_description.unwrap_or_default();
        return Err(if description.contains("AADSTS65001") {
            // A scope nobody consented to yet.
            Error::ConsentRequired
        } else if error == "invalid_grant" {
            // Refresh token expired, revoked, or the password changed: sign in again.
            Error::SignInExpired
        } else {
            authorize_error(&error, &description)
        });
    }
    let access_token = body
        .access_token
        .ok_or_else(|| Error::SignInFailed("no access token".into()))?;
    Ok(Tokens {
        access_token: Secret::new(access_token),
        refresh_token: body.refresh_token.map(Secret::new),
        expires_at: now_millis() + body.expires_in.unwrap_or(3600) * 1000,
        user: body.id_token.as_deref().and_then(id_token_user),
    })
}

fn http_client() -> Result<reqwest::Client> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let tls = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .and_then(BuilderVerifierExt::with_platform_verifier)
        .map_err(Error::TlsSetup)?
        .with_no_client_auth();
    reqwest::Client::builder()
        .tls_backend_preconfigured(tls)
        .timeout(TOKEN_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| Error::SignInFailed(error.to_string()))
}

/// The signed-in address from the ID token's claims. The token came straight from Microsoft over
/// TLS and only names the account, so its signature is not checked here.
fn id_token_user(id_token: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct Claims {
        preferred_username: Option<String>,
        email: Option<String>,
    }
    let payload = id_token.split('.').nth(1)?;
    let claims: Claims = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).ok()?).ok()?;
    claims
        .preferred_username
        .or(claims.email)
        .filter(|user| user.contains('@'))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;

    use super::*;
    use crate::secrets::MemoryStore;

    /// A stand-in for Microsoft's token endpoint, over plain HTTP on the loopback interface.
    struct TokenServer {
        url: String,
        /// The PKCE challenge from the authorize request, checked against the verifier.
        challenge: Arc<StdMutex<Option<String>>>,
    }

    fn id_token(user: &str) -> String {
        let claims = URL_SAFE_NO_PAD.encode(format!(r#"{{"preferred_username":"{user}"}}"#));
        format!("eyJhbGciOiJub25lIn0.{claims}.")
    }

    async fn token_server() -> TokenServer {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let url = format!("http://{}/token", listener.local_addr().unwrap());
        let challenge = Arc::new(StdMutex::new(None::<String>));
        let expected = Arc::clone(&challenge);
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                let mut request = Vec::new();
                let mut buffer = [0u8; 4096];
                loop {
                    let read = stream.read(&mut buffer).await.unwrap();
                    request.extend_from_slice(&buffer[..read]);
                    let text = String::from_utf8_lossy(&request);
                    if let Some((head, body)) = text.split_once("\r\n\r\n") {
                        let length: usize = head
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length: ")
                                    .map(str::to_owned)
                            })
                            .and_then(|l| l.trim().parse().ok())
                            .unwrap_or(0);
                        if body.len() >= length || read == 0 {
                            break;
                        }
                    }
                }
                let text = String::from_utf8_lossy(&request).into_owned();
                let body = text
                    .split_once("\r\n\r\n")
                    .map(|(_, b)| b)
                    .unwrap_or_default();
                let form: HashMap<String, String> = url::form_urlencoded::parse(body.as_bytes())
                    .into_owned()
                    .collect();
                let reply = match form.get("grant_type").map(String::as_str) {
                    Some("authorization_code") => {
                        let verifier = form.get("code_verifier").cloned().unwrap_or_default();
                        let computed = URL_SAFE_NO_PAD.encode(digest(&SHA256, verifier.as_bytes()));
                        let ok = form.get("code").map(String::as_str) == Some("the-code")
                            && expected.lock().unwrap().as_deref() == Some(computed.as_str());
                        if ok {
                            (200, format!(
                                r#"{{"access_token":"access-1","refresh_token":"refresh-1","expires_in":3600,"id_token":"{}"}}"#,
                                id_token("alex@contoso.example")
                            ))
                        } else {
                            (400, r#"{"error":"invalid_grant"}"#.to_owned())
                        }
                    }
                    Some("refresh_token") if form.get("refresh_token").map(String::as_str) == Some("no-calendars") => (
                        400,
                        r#"{"error":"invalid_grant","error_description":"AADSTS65001: The user or administrator has not consented"}"#.to_owned(),
                    ),
                    Some("refresh_token") if form.get("refresh_token").map(String::as_str) == Some("refresh-1") => (
                        200,
                        r#"{"access_token":"access-2","refresh_token":"refresh-2","expires_in":3600}"#.to_owned(),
                    ),
                    _ => (400, r#"{"error":"invalid_grant","error_description":"AADSTS70008: expired"}"#.to_owned()),
                };
                let response = format!(
                    "HTTP/1.1 {} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    reply.0,
                    reply.1.len(),
                    reply.1
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });
        TokenServer { url, challenge }
    }

    fn endpoints(server: &TokenServer) -> Endpoints {
        Endpoints {
            client_id: "test-client".into(),
            authorize_url: "https://login.example.org/authorize".into(),
            token_url: server.url.clone(),
            scopes: "imap offline_access".into(),
            extra_consent: "calendars".into(),
        }
    }

    /// Plays the browser: reads the authorize URL and calls the redirect like Microsoft would.
    fn browser(
        server: &TokenServer,
        answer: impl Fn(&str) -> String + Send + 'static,
    ) -> impl FnOnce(&str) -> Result<()> {
        let challenge = Arc::clone(&server.challenge);
        move |authorize_url: &str| {
            let url = Url::parse(authorize_url).unwrap();
            let params: HashMap<String, String> = url.query_pairs().into_owned().collect();
            assert_eq!(params["code_challenge_method"], "S256");
            assert_eq!(params["login_hint"], "alex@contoso.example");
            *challenge.lock().unwrap() = Some(params["code_challenge"].clone());
            let redirect = format!("{}/?{}", params["redirect_uri"], answer(&params["state"]));
            tokio::spawn(async move {
                let url = Url::parse(&redirect).unwrap();
                let port = url.port().unwrap();
                let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port))
                    .await
                    .unwrap();
                let request = format!(
                    "GET /?{} HTTP/1.1\r\nHost: localhost\r\n\r\n",
                    url.query().unwrap_or_default()
                );
                stream.write_all(request.as_bytes()).await.unwrap();
                let mut page = String::new();
                let _ = stream.read_to_string(&mut page).await;
                assert!(page.starts_with("HTTP/1.1 200"), "{page}");
            });
            Ok(())
        }
    }

    #[tokio::test]
    async fn signs_in_with_pkce_and_reads_the_address() {
        let server = token_server().await;
        let tokens = sign_in(
            &endpoints(&server),
            "alex@contoso.example",
            browser(&server, |state| format!("code=the-code&state={state}")),
            std::future::pending(),
        )
        .await
        .unwrap();
        assert_eq!(tokens.access_token.expose(), "access-1");
        assert_eq!(tokens.refresh_token.unwrap().expose(), "refresh-1");
        assert_eq!(tokens.user.as_deref(), Some("alex@contoso.example"));
        assert!(tokens.expires_at > now_millis());
    }

    #[tokio::test]
    async fn a_redirect_with_the_wrong_state_is_ignored() {
        let server = token_server().await;
        let result = tokio::time::timeout(
            Duration::from_millis(500),
            sign_in(
                &endpoints(&server),
                "alex@contoso.example",
                browser(&server, |_| "code=the-code&state=forged".into()),
                std::future::pending(),
            ),
        )
        .await;
        // Still waiting for the real answer.
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn refusals_and_cancelling_are_told_apart() {
        let server = token_server().await;
        let refused = sign_in(
            &endpoints(&server),
            "alex@contoso.example",
            browser(&server, |state| {
                format!("error=access_denied&error_description=AADSTS90094%3A+admin+approval&state={state}")
            }),
            std::future::pending(),
        )
        .await;
        assert!(
            matches!(refused, Err(Error::ConsentRequired)),
            "{:?}",
            refused.err()
        );

        let cancelled = sign_in(
            &endpoints(&server),
            "alex@contoso.example",
            |_: &str| Ok(()),
            async {},
        )
        .await;
        assert!(matches!(cancelled, Err(Error::SignInCancelled)));
    }

    #[tokio::test]
    async fn access_tokens_are_cached_and_renewed() {
        let server = token_server().await;
        let secrets = MemoryStore::default();
        let account = crate::ids::new_id();
        secrets
            .set(
                &account,
                SecretKind::OAuthRefreshToken,
                &Secret::new("refresh-1"),
            )
            .unwrap();

        let first = access_token_with(&endpoints(&server), &secrets, &account)
            .await
            .unwrap();
        assert_eq!(first.expose(), "access-2");
        // Microsoft rotated the refresh token; the new one is kept.
        let stored = secrets
            .get(&account, SecretKind::OAuthRefreshToken)
            .unwrap();
        assert_eq!(stored.expose(), "refresh-2");
        // The next session reuses the token instead of asking again (refresh-2 would fail).
        let again = access_token_with(&endpoints(&server), &secrets, &account)
            .await
            .unwrap();
        assert_eq!(again.expose(), "access-2");

        forget(&account);
        let expired = access_token_with(&endpoints(&server), &secrets, &account).await;
        assert!(matches!(expired, Err(Error::SignInExpired)));
    }

    #[tokio::test]
    async fn calendars_have_their_own_token_and_consent() {
        let server = token_server().await;
        let secrets = MemoryStore::default();
        let account = crate::ids::new_id();
        let refresh = |token: &str| {
            secrets
                .set(&account, SecretKind::OAuthRefreshToken, &Secret::new(token))
                .unwrap();
        };
        refresh("refresh-1");
        let calendars = token_for(&endpoints(&server), &secrets, &account, "calendars")
            .await
            .unwrap();
        assert_eq!(calendars.expose(), "access-2");
        // Kept apart from the mail token: mail still asks the server.
        refresh("no-calendars");
        assert!(matches!(
            access_token_with(&endpoints(&server), &secrets, &account).await,
            Err(Error::ConsentRequired)
        ));
        // An account that signed in before the app asked for calendars.
        forget(&account);
        assert!(matches!(
            token_for(&endpoints(&server), &secrets, &account, "calendars").await,
            Err(Error::CalendarAccessMissing)
        ));
    }

    #[test]
    fn reads_the_address_from_the_id_token() {
        assert_eq!(
            id_token_user(&id_token("alex@contoso.example")).as_deref(),
            Some("alex@contoso.example")
        );
        assert_eq!(id_token_user("not-a-token"), None);
    }
}
