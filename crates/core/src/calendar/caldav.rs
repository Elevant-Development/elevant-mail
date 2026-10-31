//! A CalDAV client (RFC 4791) for the calendars of mail accounts: finding them, listing their
//! events with ETags, fetching and writing events. It signs in with the account's user name and
//! password (Basic over TLS); plain HTTP is only allowed to this computer, for tests.

use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{CONTENT_TYPE, ETAG, HeaderValue, LOCATION};
use reqwest::{Method, StatusCode};
use rustls::ClientConfig;
use rustls_platform_verifier::BuilderVerifierExt;
use url::Url;

use super::xml::{self, APPLE_ICAL, CALDAV, CALENDARSERVER, DAV, Element, Response};
use crate::secrets::Secret;
use crate::{Error, Result};

const TIMEOUT: Duration = Duration::from_secs(30);
/// Redirects followed, as from `/.well-known/caldav` to the server's own path.
const MAX_REDIRECTS: usize = 5;
/// Events asked for in one `calendar-multiget`.
const MULTIGET_BATCH: usize = 50;

/// A calendar on the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RemoteCalendar {
    pub url: Url,
    pub name: String,
    /// `#rrggbb` or `#rrggbbaa`, as the server keeps it.
    pub color: Option<String>,
    pub read_only: bool,
    /// Changes whenever an event in it does (`getctag`, or the `sync-token`).
    pub version: Option<String>,
}

/// An event on the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RemoteEvent {
    pub url: Url,
    pub etag: Option<String>,
    pub ics: String,
}

/// What a write may overwrite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Condition<'a> {
    /// Only a new event: nothing at that address yet.
    New,
    /// Only this version (ETag).
    Version(&'a str),
    /// Whatever is there: the server gave no ETag to check against.
    Any,
}

/// How a write went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Written {
    /// Done; the new ETag if the server said it.
    Done(Option<String>),
    /// Someone changed or removed it on the server since: the server's version stays.
    Conflict,
}

pub(crate) struct Dav {
    client: reqwest::Client,
    username: String,
    password: Secret,
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
        .timeout(TIMEOUT)
        // Followed by hand: reqwest would turn PROPFIND into GET.
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| Error::Protocol(error.to_string()))
}

/// Only HTTPS, except to this computer.
pub(crate) fn allowed(url: &Url) -> bool {
    match url.scheme() {
        "https" => true,
        "http" => matches!(
            url.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
        ),
        _ => false,
    }
}

fn network_error(error: &reqwest::Error) -> Error {
    if error.is_timeout() {
        Error::Timeout
    } else if error.is_connect() {
        Error::ConnectionFailed(std::io::Error::other(error.to_string()))
    } else {
        Error::Protocol(error.to_string())
    }
}

/// The ETag as the server sent it, quotes included: `If-Match` wants it back like that.
fn etag_of(response: &reqwest::Response) -> Option<String> {
    response
        .headers()
        .get(ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

impl Dav {
    pub fn new(username: &str, password: Secret) -> Result<Self> {
        Ok(Self {
            client: http_client()?,
            username: username.to_owned(),
            password,
        })
    }

    /// One request, following redirects; returns where it ended up and the answer.
    async fn send(
        &self,
        method: &str,
        url: &Url,
        headers: &[(&str, &str)],
        body: Option<&str>,
    ) -> Result<(Url, reqwest::Response)> {
        let method = Method::from_bytes(method.as_bytes()).expect("a valid method");
        let mut url = url.clone();
        for _ in 0..=MAX_REDIRECTS {
            if !allowed(&url) {
                return Err(Error::InvalidLink);
            }
            let mut request = self
                .client
                .request(method.clone(), url.clone())
                .basic_auth(&self.username, Some(self.password.expose()));
            for (name, value) in headers {
                request = request.header(*name, *value);
            }
            if let Some(body) = body {
                request = request
                    .header(
                        CONTENT_TYPE,
                        HeaderValue::from_static("application/xml; charset=utf-8"),
                    )
                    .body(body.to_owned());
            }
            let response = request
                .send()
                .await
                .map_err(|error| network_error(&error))?;
            if response.status().is_redirection()
                && let Some(location) = response
                    .headers()
                    .get(LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|location| url.join(location).ok())
            {
                url = location;
                continue;
            }
            if response.status() == StatusCode::UNAUTHORIZED {
                return Err(Error::AuthenticationFailed { provider: None });
            }
            return Ok((url, response));
        }
        Err(Error::Protocol("too many redirects".into()))
    }

    /// A PROPFIND for `props` (XML elements in the `d:` and `c:` namespaces); `None` where the
    /// server knows no such resource or does not speak WebDAV there.
    async fn propfind(
        &self,
        url: &Url,
        depth: &str,
        props: &str,
    ) -> Result<Option<(Url, Vec<Response>)>> {
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:" xmlns:c="{CALDAV}" xmlns:cs="{CALENDARSERVER}" xmlns:ic="{APPLE_ICAL}"><d:prop>{props}</d:prop></d:propfind>"#
        );
        let (url, response) = self
            .send("PROPFIND", url, &[("Depth", depth)], Some(&body))
            .await?;
        match response.status().as_u16() {
            207 => {
                let text = response
                    .text()
                    .await
                    .map_err(|error| network_error(&error))?;
                Ok(Some((url, xml::multistatus(&text))))
            }
            404 | 405 | 501 => Ok(None),
            code if (400..500).contains(&code) => Ok(None),
            code => Err(Error::Protocol(format!("PROPFIND answered {code}"))),
        }
    }

    /// Where the user's calendars are (the calendar home), starting from a server address such
    /// as `https://mail.example.org/.well-known/caldav`; `None` when there is no CalDAV there.
    pub async fn find_home(&self, start: &Url) -> Result<Option<Url>> {
        let Some((url, responses)) = self
            .propfind(
                start,
                "0",
                "<d:current-user-principal/><c:calendar-home-set/>",
            )
            .await?
        else {
            return Ok(None);
        };
        let prop = |namespace: &str, name: &str| {
            responses
                .iter()
                .find_map(|response| response.prop(namespace, name)?.href().map(str::to_owned))
        };
        if let Some(home) = prop(CALDAV, "calendar-home-set") {
            return Ok(url.join(&home).ok());
        }
        let Some(principal) = prop(DAV, "current-user-principal").and_then(|p| url.join(&p).ok())
        else {
            return Ok(None);
        };
        let Some((url, responses)) = self
            .propfind(&principal, "0", "<c:calendar-home-set/>")
            .await?
        else {
            return Ok(None);
        };
        Ok(responses
            .iter()
            .find_map(|response| response.prop(CALDAV, "calendar-home-set")?.href())
            .and_then(|home| url.join(home).ok()))
    }

    /// The calendars in the home that hold events.
    pub async fn calendars(&self, home: &Url) -> Result<Vec<RemoteCalendar>> {
        let Some((url, responses)) = self
            .propfind(
                home,
                "1",
                "<d:resourcetype/><d:displayname/><d:current-user-privilege-set/>\
                 <c:supported-calendar-component-set/><cs:getctag/><d:sync-token/>\
                 <ic:calendar-color/>",
            )
            .await?
        else {
            return Err(Error::Protocol("the calendar home is gone".into()));
        };
        Ok(responses
            .iter()
            .filter_map(|response| {
                response
                    .prop(DAV, "resourcetype")?
                    .child(CALDAV, "calendar")?;
                // Where the server says which components a calendar takes: only those with events,
                // not task lists.
                if let Some(components) = response.prop(CALDAV, "supported-calendar-component-set")
                    && !components.children.is_empty()
                    && !components.children_named(CALDAV, "comp").any(|component| {
                        component
                            .attribute("name")
                            .is_some_and(|name| name.eq_ignore_ascii_case("VEVENT"))
                    })
                {
                    return None;
                }
                let url = url.join(&response.href).ok()?;
                let read_only = response
                    .prop(DAV, "current-user-privilege-set")
                    .is_some_and(|privileges| !can_write(privileges));
                Some(RemoteCalendar {
                    name: response
                        .prop(DAV, "displayname")
                        .and_then(Element::value)
                        .map(str::to_owned)
                        .unwrap_or_else(|| last_segment(&url)),
                    color: response
                        .prop(APPLE_ICAL, "calendar-color")
                        .and_then(Element::value)
                        .filter(|color| color.starts_with('#'))
                        .map(str::to_owned),
                    read_only,
                    version: version(response),
                    url,
                })
            })
            .collect())
    }

    /// Whether the server sends invitations by itself (`calendar-auto-schedule` in its `DAV`
    /// header, RFC 6638).
    pub async fn schedules(&self, url: &Url) -> Result<bool> {
        let (_, response) = self.send("OPTIONS", url, &[], None).await?;
        Ok(response
            .headers()
            .get_all("DAV")
            .iter()
            .filter_map(|value| value.to_str().ok())
            .any(|value| {
                value.split(',').any(|feature| {
                    feature
                        .trim()
                        .eq_ignore_ascii_case("calendar-auto-schedule")
                })
            }))
    }

    /// The events in a calendar, with their ETags.
    pub async fn etags(&self, calendar: &Url) -> Result<Vec<(Url, Option<String>)>> {
        let Some((url, responses)) = self
            .propfind(calendar, "1", "<d:getetag/><d:resourcetype/>")
            .await?
        else {
            return Err(Error::Protocol("the calendar is gone".into()));
        };
        Ok(responses
            .iter()
            .filter(|response| {
                // Not the calendar itself, nor collections inside it.
                response
                    .prop(DAV, "resourcetype")
                    .is_none_or(|kind| kind.child(DAV, "collection").is_none())
            })
            .filter_map(|response| {
                let member = url.join(&response.href).ok()?;
                (member.path() != url.path()).then(|| {
                    (
                        member,
                        response
                            .prop(DAV, "getetag")
                            .and_then(Element::value)
                            .map(str::to_owned),
                    )
                })
            })
            .collect())
    }

    /// The events at `urls`, with their iCalendar text, in batches.
    pub async fn fetch(&self, calendar: &Url, urls: &[Url]) -> Result<Vec<RemoteEvent>> {
        let mut events = Vec::new();
        for batch in urls.chunks(MULTIGET_BATCH) {
            let hrefs: String = batch
                .iter()
                .map(|url| format!("<d:href>{}</d:href>", xml::escape(url.path())))
                .collect();
            let body = format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<c:calendar-multiget xmlns:d="DAV:" xmlns:c="{CALDAV}"><d:prop><d:getetag/><c:calendar-data/></d:prop>{hrefs}</c:calendar-multiget>"#
            );
            let (url, response) = self
                .send("REPORT", calendar, &[("Depth", "1")], Some(&body))
                .await?;
            if response.status().as_u16() != 207 {
                return Err(Error::Protocol(format!(
                    "calendar-multiget answered {}",
                    response.status().as_u16()
                )));
            }
            let text = response
                .text()
                .await
                .map_err(|error| network_error(&error))?;
            events.extend(xml::multistatus(&text).into_iter().filter_map(|response| {
                Some(RemoteEvent {
                    url: url.join(&response.href).ok()?,
                    etag: response
                        .prop(DAV, "getetag")
                        .and_then(Element::value)
                        .map(str::to_owned),
                    ics: response.prop(CALDAV, "calendar-data")?.value()?.to_owned(),
                })
            }));
        }
        Ok(events)
    }

    /// Writes an event, under the condition.
    pub async fn put(&self, url: &Url, ics: &str, condition: Condition<'_>) -> Result<Written> {
        if !allowed(url) {
            return Err(Error::InvalidLink);
        }
        let mut request = self
            .client
            .put(url.clone())
            .basic_auth(&self.username, Some(self.password.expose()))
            .header(CONTENT_TYPE, "text/calendar; charset=utf-8")
            .body(ics.to_owned());
        request = match condition {
            Condition::New => request.header("If-None-Match", "*"),
            Condition::Version(etag) => request.header("If-Match", etag),
            Condition::Any => request,
        };
        let response = request
            .send()
            .await
            .map_err(|error| network_error(&error))?;
        match response.status().as_u16() {
            200..=299 => Ok(Written::Done(etag_of(&response))),
            401 => Err(Error::AuthenticationFailed { provider: None }),
            403 => Err(Error::CalendarReadOnly),
            412 => Ok(Written::Conflict),
            code => Err(Error::Protocol(format!("PUT answered {code}"))),
        }
    }

    /// Deletes an event, only if it is still that version; gone already counts as done.
    pub async fn delete(&self, url: &Url, etag: Option<&str>) -> Result<Written> {
        if !allowed(url) {
            return Err(Error::InvalidLink);
        }
        let mut request = self
            .client
            .delete(url.clone())
            .basic_auth(&self.username, Some(self.password.expose()));
        if let Some(etag) = etag {
            request = request.header("If-Match", etag);
        }
        let response = request
            .send()
            .await
            .map_err(|error| network_error(&error))?;
        match response.status().as_u16() {
            200..=299 | 404 | 410 => Ok(Written::Done(None)),
            401 => Err(Error::AuthenticationFailed { provider: None }),
            403 => Err(Error::CalendarReadOnly),
            412 => Ok(Written::Conflict),
            code => Err(Error::Protocol(format!("DELETE answered {code}"))),
        }
    }
}

fn can_write(privileges: &Element) -> bool {
    privileges
        .children_named(DAV, "privilege")
        .any(|privilege| {
            privilege.children.iter().any(|kind| {
                kind.namespace == DAV
                    && matches!(kind.name.as_str(), "write" | "write-content" | "all")
            })
        })
}

fn version(response: &Response) -> Option<String> {
    response
        .prop(CALENDARSERVER, "getctag")
        .or_else(|| response.prop(DAV, "sync-token"))
        .and_then(Element::value)
        .map(str::to_owned)
}

/// The last part of a calendar's path, for one without a name.
fn last_segment(url: &Url) -> String {
    url.path_segments()
        .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
        .map(|segment| {
            percent_encoding::percent_decode_str(segment)
                .decode_utf8_lossy()
                .into_owned()
        })
        .unwrap_or_default()
}
