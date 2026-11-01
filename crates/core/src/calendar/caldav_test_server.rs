//! A CalDAV and CardDAV server in the test process, over plain HTTP on 127.0.0.1: one user
//! (`alex`, password `secret`) with a principal, calendars at `/dav/calendars/alex/<name>/` and
//! address books at `/dav/addressbooks/alex/<name>/`. `/.well-known/caldav` and
//! `/.well-known/carddav` redirect to the principal, as Mailcow's SOGo and Nextcloud do. Enough of
//! WebDAV for the client: PROPFIND, the `calendar-multiget` and `addressbook-multiget` REPORTs, PUT
//! and DELETE with `If-Match`/`If-None-Match`.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use super::xml;

pub(crate) const USER: &str = "alex";
pub(crate) const PASSWORD: &str = "secret";
const PRINCIPAL: &str = "/dav/principals/alex/";
const HOME: &str = "/dav/calendars/alex/";
const ADDRESS_HOME: &str = "/dav/addressbooks/alex/";

#[derive(Debug, Clone, Default)]
pub(crate) struct Calendar {
    pub name: String,
    pub read_only: bool,
    /// By name in the calendar (`event.ics`): ETag number and text.
    pub events: BTreeMap<String, (u64, String)>,
    pub version: u64,
}

#[derive(Debug, Default)]
pub(crate) struct Store {
    /// By path segment (`work`).
    pub calendars: BTreeMap<String, Calendar>,
    /// Address books, by path segment, with their vCards; a slug of its own, apart from the
    /// calendars'.
    pub address_books: BTreeMap<String, Calendar>,
    next_etag: u64,
    /// Method and path of every request, for tests that count them.
    pub requests: Vec<String>,
}

impl Store {
    pub fn add_calendar(&mut self, slug: &str, name: &str, read_only: bool) {
        self.calendars.insert(
            slug.into(),
            Calendar {
                name: name.into(),
                read_only,
                ..Calendar::default()
            },
        );
    }

    pub fn add_address_book(&mut self, slug: &str, name: &str, read_only: bool) {
        self.address_books.insert(
            slug.into(),
            Calendar {
                name: name.into(),
                read_only,
                ..Calendar::default()
            },
        );
    }

    fn collection(&mut self, slug: &str) -> &mut Calendar {
        match self.calendars.get_mut(slug) {
            Some(calendar) => calendar,
            None => self.address_books.get_mut(slug).expect("a collection"),
        }
    }

    /// Puts an event or contact in, as another device would; returns its ETag.
    pub fn put(&mut self, slug: &str, file: &str, text: &str) -> String {
        self.next_etag += 1;
        let etag = self.next_etag;
        let collection = self.collection(slug);
        collection.events.insert(file.into(), (etag, text.into()));
        collection.version += 1;
        format!("\"{etag}\"")
    }

    pub fn remove(&mut self, slug: &str, file: &str) {
        let collection = self.collection(slug);
        collection.events.remove(file);
        collection.version += 1;
    }
}

pub(crate) struct CaldavServer {
    pub port: u16,
    pub store: Arc<Mutex<Store>>,
}

impl CaldavServer {
    pub fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    pub fn store(&self) -> std::sync::MutexGuard<'_, Store> {
        self.store.lock().unwrap()
    }
}

pub(crate) async fn start() -> CaldavServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let store = Arc::new(Mutex::new(Store::default()));
    let shared = store.clone();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let store = shared.clone();
            tokio::spawn(async move {
                let _ = serve(stream, &store).await;
            });
        }
    });
    CaldavServer { port, store }
}

pub(crate) struct Request {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

pub(crate) async fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut data = Vec::new();
    let mut buffer = [0; 4096];
    let head_end = loop {
        let read = stream.read(&mut buffer).await.ok()?;
        if read == 0 {
            return None;
        }
        data.extend_from_slice(&buffer[..read]);
        if let Some(end) = data.windows(4).position(|window| window == b"\r\n\r\n") {
            break end;
        }
    };
    let head = String::from_utf8_lossy(&data[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split_whitespace();
    let method = first.next()?.to_owned();
    let path = first.next()?.to_owned();
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
        .collect();
    let length: usize = headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.parse().ok())
        .unwrap_or(0);
    let mut body = data[head_end + 4..].to_vec();
    while body.len() < length {
        let read = stream.read(&mut buffer).await.ok()?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&buffer[..read]);
    }
    Some(Request {
        method,
        path,
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}

struct Answer {
    status: &'static str,
    headers: Vec<(&'static str, String)>,
    body: String,
}

fn answer(status: &'static str) -> Answer {
    Answer {
        status,
        headers: Vec::new(),
        body: String::new(),
    }
}

fn multistatus(responses: &str) -> Answer {
    Answer {
        status: "207 Multi-Status",
        headers: vec![("Content-Type", "application/xml; charset=utf-8".into())],
        body: format!(
            r#"<?xml version="1.0" encoding="utf-8"?><d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav" xmlns:card="urn:ietf:params:xml:ns:carddav" xmlns:cs="http://calendarserver.org/ns/" xmlns:ic="http://apple.com/ns/ical/">{responses}</d:multistatus>"#
        ),
    }
}

fn response(href: &str, props: &str) -> String {
    format!(
        "<d:response><d:href>{href}</d:href><d:propstat><d:prop>{props}</d:prop>\
         <d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>"
    )
}

async fn serve(mut stream: TcpStream, store: &Mutex<Store>) -> std::io::Result<()> {
    let Some(request) = read_request(&mut stream).await else {
        return Ok(());
    };
    let answer = handle(&request, store);
    let mut out = format!(
        "HTTP/1.1 {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        answer.status,
        answer.body.len()
    );
    for (name, value) in &answer.headers {
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    out.push_str("\r\n");
    out.push_str(&answer.body);
    stream.write_all(out.as_bytes()).await?;
    stream.shutdown().await
}

fn handle(request: &Request, store: &Mutex<Store>) -> Answer {
    let mut store = store.lock().unwrap();
    store
        .requests
        .push(format!("{} {}", request.method, request.path));
    let expected = format!("Basic {}", STANDARD.encode(format!("{USER}:{PASSWORD}")));
    if request.header("Authorization") != Some(expected.as_str()) {
        let mut refused = answer("401 Unauthorized");
        refused
            .headers
            .push(("WWW-Authenticate", "Basic realm=\"test\"".into()));
        return refused;
    }
    let path = request.path.as_str();
    match (request.method.as_str(), path) {
        ("OPTIONS", _) => {
            let mut done = answer("200 OK");
            done.headers.push((
                "DAV",
                "1, 2, access-control, calendar-access, calendar-auto-schedule".into(),
            ));
            done
        }
        ("PROPFIND", "/.well-known/caldav" | "/.well-known/carddav") => {
            let mut moved = answer("301 Moved Permanently");
            moved.headers.push(("Location", PRINCIPAL.into()));
            moved
        }
        ("PROPFIND", PRINCIPAL) => multistatus(&response(
            PRINCIPAL,
            &format!(
                "<d:current-user-principal><d:href>{PRINCIPAL}</d:href></d:current-user-principal>\
                 <c:calendar-home-set><d:href>{HOME}</d:href></c:calendar-home-set>\
                 <card:addressbook-home-set><d:href>{ADDRESS_HOME}</d:href></card:addressbook-home-set>"
            ),
        )),
        // Any address of the user names the principal, as the client asks it of the calendars.
        ("PROPFIND", _) if request.body.contains("addressbook-home-set") => multistatus(&response(
            path,
            &format!(
                "<d:current-user-principal><d:href>{PRINCIPAL}</d:href></d:current-user-principal>"
            ),
        )),
        ("PROPFIND", ADDRESS_HOME) => {
            let mut responses = response(
                ADDRESS_HOME,
                "<d:resourcetype><d:collection/></d:resourcetype>",
            );
            for (slug, book) in &store.address_books {
                let privileges = if book.read_only {
                    "<d:privilege><d:read/></d:privilege>"
                } else {
                    "<d:privilege><d:read/></d:privilege><d:privilege><d:write/></d:privilege>"
                };
                responses.push_str(&response(
                    &format!("{ADDRESS_HOME}{slug}/"),
                    &format!(
                        "<d:resourcetype><d:collection/><card:addressbook/></d:resourcetype>\
                         <d:displayname>{}</d:displayname>\
                         <d:current-user-privilege-set>{privileges}</d:current-user-privilege-set>\
                         <cs:getctag>\"{}\"</cs:getctag>",
                        xml::escape(&book.name),
                        book.version
                    ),
                ));
            }
            multistatus(&responses)
        }
        ("PROPFIND", HOME) => {
            let mut responses = response(HOME, "<d:resourcetype><d:collection/></d:resourcetype>");
            for (slug, calendar) in &store.calendars {
                let privileges = if calendar.read_only {
                    "<d:privilege><d:read/></d:privilege>"
                } else {
                    "<d:privilege><d:read/></d:privilege><d:privilege><d:write/></d:privilege>"
                };
                responses.push_str(&response(
                    &format!("{HOME}{slug}/"),
                    &format!(
                        "<d:resourcetype><d:collection/><c:calendar/></d:resourcetype>\
                         <d:displayname>{}</d:displayname>\
                         <d:current-user-privilege-set>{privileges}</d:current-user-privilege-set>\
                         <c:supported-calendar-component-set><c:comp name=\"VEVENT\"/></c:supported-calendar-component-set>\
                         <cs:getctag>\"{}\"</cs:getctag><ic:calendar-color>#3366ccff</ic:calendar-color>",
                        xml::escape(&calendar.name),
                        calendar.version
                    ),
                ));
            }
            // A task list, which the client leaves out.
            responses.push_str(&response(
                &format!("{HOME}tasks/"),
                "<d:resourcetype><d:collection/><c:calendar/></d:resourcetype>\
                 <d:displayname>Tasks</d:displayname>\
                 <c:supported-calendar-component-set><c:comp name=\"VTODO\"/></c:supported-calendar-component-set>",
            ));
            multistatus(&responses)
        }
        (method, _) if path.starts_with(HOME) || path.starts_with(ADDRESS_HOME) => {
            let contacts = path.starts_with(ADDRESS_HOME);
            let (home, kind, data) = if contacts {
                (ADDRESS_HOME, "<card:addressbook/>", "card:address-data")
            } else {
                (HOME, "<c:calendar/>", "c:calendar-data")
            };
            let rest = &path[home.len()..];
            let (slug, file) = rest.split_once('/').unwrap_or((rest, ""));
            let collections = if contacts {
                &store.address_books
            } else {
                &store.calendars
            };
            let Some(calendar) = collections.get(slug).cloned() else {
                return answer("404 Not Found");
            };
            match (method, file) {
                ("PROPFIND", "") => {
                    let mut responses = response(
                        path,
                        &format!("<d:resourcetype><d:collection/>{kind}</d:resourcetype>"),
                    );
                    for (name, (etag, _)) in &calendar.events {
                        responses.push_str(&response(
                            &format!("{home}{slug}/{name}"),
                            &format!("<d:getetag>\"{etag}\"</d:getetag><d:resourcetype/>"),
                        ));
                    }
                    multistatus(&responses)
                }
                ("REPORT", "") => {
                    let asked: Vec<String> = xml::parse(&request.body)
                        .map(|root| {
                            root.children_named(xml::DAV, "href")
                                .filter_map(|href| href.value().map(str::to_owned))
                                .collect()
                        })
                        .unwrap_or_default();
                    let responses: String = asked
                        .iter()
                        .filter_map(|href| {
                            let name = href.rsplit('/').next()?;
                            let (etag, ics) = calendar.events.get(name)?;
                            Some(response(
                                href,
                                &format!(
                                    "<d:getetag>\"{etag}\"</d:getetag><{data}>{}</{data}>",
                                    xml::escape(ics)
                                ),
                            ))
                        })
                        .collect();
                    multistatus(&responses)
                }
                ("PUT", file) if !file.is_empty() => {
                    if calendar.read_only {
                        return answer("403 Forbidden");
                    }
                    let current = calendar
                        .events
                        .get(file)
                        .map(|(etag, _)| format!("\"{etag}\""));
                    let allowed =
                        match (request.header("If-None-Match"), request.header("If-Match")) {
                            (Some("*"), _) => current.is_none(),
                            (_, Some(expected)) => current.as_deref() == Some(expected),
                            _ => true,
                        };
                    if !allowed {
                        return answer("412 Precondition Failed");
                    }
                    let etag = store.put(slug, file, &request.body);
                    let mut done = answer(if current.is_some() {
                        "204 No Content"
                    } else {
                        "201 Created"
                    });
                    done.headers.push(("ETag", etag));
                    done
                }
                ("DELETE", file) if !file.is_empty() => {
                    let current = calendar
                        .events
                        .get(file)
                        .map(|(etag, _)| format!("\"{etag}\""));
                    match (current, request.header("If-Match")) {
                        (None, _) => answer("404 Not Found"),
                        (Some(current), Some(expected)) if current != expected => {
                            answer("412 Precondition Failed")
                        }
                        _ => {
                            store.remove(slug, file);
                            answer("204 No Content")
                        }
                    }
                }
                _ => answer("405 Method Not Allowed"),
            }
        }
        _ => answer("404 Not Found"),
    }
}
