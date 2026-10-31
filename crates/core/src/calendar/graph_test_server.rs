//! Microsoft Graph's calendar API in the test process, over plain HTTP on 127.0.0.1: calendars,
//! their events (one per page, so paging is used), single events with `If-Match`, and answers to
//! invitations. Signs in with the bearer token `test-token`.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

use super::caldav_test_server::{Request, read_request};

pub(crate) const TOKEN: &str = "test-token";

#[derive(Debug, Default)]
pub(crate) struct Store {
    /// Id, name, whether it can be changed.
    pub calendars: Vec<(String, String, bool)>,
    /// By id: the calendar and the event as Graph shows it.
    pub events: BTreeMap<String, (String, Value)>,
    /// Event and action of every answer to an invitation.
    pub answers: Vec<(String, String)>,
    next: u64,
}

impl Store {
    /// Adds or replaces an event, as another device would; gives it a new `changeKey`.
    pub fn put(&mut self, calendar: &str, mut event: Value) -> String {
        self.next += 1;
        let id = event["id"]
            .as_str()
            .map_or_else(|| format!("evt-{}", self.next), str::to_owned);
        event["id"] = json!(id);
        event["changeKey"] = json!(format!("ck-{}", self.next));
        if event.get("iCalUId").is_none() {
            event["iCalUId"] = json!(format!("uid-{id}"));
        }
        self.events.insert(id.clone(), (calendar.to_owned(), event));
        id
    }
}

pub(crate) struct GraphServer {
    pub port: u16,
    pub store: Arc<Mutex<Store>>,
}

impl GraphServer {
    pub fn base(&self) -> String {
        format!("http://127.0.0.1:{}/v1.0/", self.port)
    }

    pub fn store(&self) -> std::sync::MutexGuard<'_, Store> {
        self.store.lock().unwrap()
    }
}

pub(crate) async fn start() -> GraphServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let store = Arc::new(Mutex::new(Store::default()));
    let shared = store.clone();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let store = shared.clone();
            tokio::spawn(async move {
                let _ = serve(stream, &store, port).await;
            });
        }
    });
    GraphServer { port, store }
}

async fn serve(mut stream: TcpStream, store: &Mutex<Store>, port: u16) -> std::io::Result<()> {
    let Some(request) = read_request(&mut stream).await else {
        return Ok(());
    };
    let (status, body) = handle(&request, store, port);
    let body = body.map(|body| body.to_string()).unwrap_or_default();
    let out = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(out.as_bytes()).await?;
    stream.shutdown().await
}

/// Times as Graph keeps them for an app that asks for UTC: the zone they were given in becomes
/// `originalStartTimeZone`.
fn in_utc(event: &mut Value) {
    for key in ["start", "end"] {
        let (Some(text), Some(zone)) = (
            event[key]["dateTime"].as_str().map(str::to_owned),
            event[key]["timeZone"].as_str().map(str::to_owned),
        ) else {
            continue;
        };
        let Ok(zone) = zone.parse::<chrono_tz::Tz>() else {
            continue;
        };
        let Ok(naive) = chrono::NaiveDateTime::parse_from_str(&text, "%Y-%m-%dT%H:%M:%S") else {
            continue;
        };
        use chrono::TimeZone;
        let Some(local) = zone.from_local_datetime(&naive).earliest() else {
            continue;
        };
        event[key] = json!({
            "dateTime": local.naive_utc().format("%Y-%m-%dT%H:%M:%S.0000000").to_string(),
            "timeZone": "UTC",
        });
        if key == "start" {
            event["originalStartTimeZone"] = json!(zone.name());
        }
    }
}

fn query(path: &str, name: &str) -> Option<String> {
    let (_, query) = path.split_once('?')?;
    url::form_urlencoded::parse(query.as_bytes())
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
}

fn handle(request: &Request, shared: &Mutex<Store>, port: u16) -> (&'static str, Option<Value>) {
    if request.header("Authorization") != Some(&format!("Bearer {TOKEN}")) {
        return ("401 Unauthorized", None);
    }
    let mut store = shared.lock().unwrap();
    let path = request.path.as_str();
    let route = path
        .split('?')
        .next()
        .unwrap_or_default()
        .trim_start_matches("/v1.0/");
    let segments: Vec<&str> = route.split('/').collect();
    let body = || serde_json::from_str::<Value>(&request.body).unwrap_or(Value::Null);
    match (request.method.as_str(), segments.as_slice()) {
        ("POST", ["$batch"]) => {
            drop(store);
            let requests = body()["requests"].as_array().cloned().unwrap_or_default();
            let responses: Vec<Value> = requests
                .iter()
                .map(|inner| {
                    let single = Request {
                        method: inner["method"].as_str().unwrap_or("GET").to_owned(),
                        path: format!("/v1.0{}", inner["url"].as_str().unwrap_or_default()),
                        headers: request.headers.clone(),
                        body: inner["body"].to_string(),
                    };
                    let (status, body) = handle(&single, shared, port);
                    let code: u16 = status[..3].parse().unwrap_or(500);
                    json!({ "id": inner["id"], "status": code, "body": body.unwrap_or(Value::Null) })
                })
                .collect();
            ("200 OK", Some(json!({ "responses": responses })))
        }
        ("GET", ["me", "calendars"]) => {
            let value: Vec<Value> = store
                .calendars
                .iter()
                .map(|(id, name, can_edit)| {
                    json!({ "id": id, "name": name, "hexColor": "#ff8c00", "canEdit": can_edit })
                })
                .collect();
            ("200 OK", Some(json!({ "value": value })))
        }
        ("GET", ["me", "calendars", calendar, "events"]) => {
            let listed: Vec<Value> = store
                .events
                .iter()
                .filter(|(_, (in_calendar, _))| in_calendar == calendar)
                .map(|(id, (_, event))| json!({ "id": id, "changeKey": event["changeKey"] }))
                .collect();
            // One per page.
            let skip: usize = query(path, "$skip")
                .and_then(|skip| skip.parse().ok())
                .unwrap_or(0);
            let mut page = json!({ "value": listed.iter().skip(skip).take(1).collect::<Vec<_>>() });
            if skip + 1 < listed.len() {
                page["@odata.nextLink"] = json!(format!(
                    "http://127.0.0.1:{port}/v1.0/me/calendars/{calendar}/events?$skip={}",
                    skip + 1
                ));
            }
            ("200 OK", Some(page))
        }
        ("POST", ["me", "calendars", calendar, "events"]) => {
            let Some((_, _, true)) = store.calendars.iter().find(|(id, ..)| id == calendar) else {
                return ("403 Forbidden", None);
            };
            let mut event = body();
            in_utc(&mut event);
            event["type"] = json!(if event["recurrence"].is_null() {
                "singleInstance"
            } else {
                "seriesMaster"
            });
            let id = store.put(calendar, event);
            ("201 Created", Some(store.events[&id].1.clone()))
        }
        ("GET", ["me", "events"]) => {
            let filter = query(path, "$filter").unwrap_or_default();
            let uid = filter
                .strip_prefix("iCalUId eq '")
                .and_then(|rest| rest.strip_suffix('\''))
                .unwrap_or_default();
            let value: Vec<Value> = store
                .events
                .values()
                .filter(|(_, event)| event["iCalUId"] == uid)
                .map(|(_, event)| json!({ "id": event["id"], "changeKey": event["changeKey"] }))
                .collect();
            ("200 OK", Some(json!({ "value": value })))
        }
        ("GET", ["me", "events", id]) => match store.events.get(*id) {
            Some((_, event)) => ("200 OK", Some(event.clone())),
            None => ("404 Not Found", None),
        },
        ("PATCH", ["me", "events", id]) => {
            let Some((calendar, current)) = store.events.get(*id).cloned() else {
                return ("404 Not Found", None);
            };
            let expected = format!(
                "W/\"{}\"",
                current["changeKey"].as_str().unwrap_or_default()
            );
            if request
                .header("If-Match")
                .is_some_and(|tag| tag != expected)
            {
                return ("412 Precondition Failed", None);
            }
            let mut event = current;
            let mut changes = body();
            in_utc(&mut changes);
            if let Value::Object(changes) = changes {
                for (key, value) in changes {
                    event[key] = value;
                }
            }
            store.put(&calendar, event);
            ("200 OK", Some(store.events[*id].1.clone()))
        }
        ("DELETE", ["me", "events", id]) => {
            if store.events.remove(*id).is_some() {
                ("204 No Content", None)
            } else {
                ("404 Not Found", None)
            }
        }
        ("POST", ["me", "events", id, action]) => {
            if !store.events.contains_key(*id) {
                return ("404 Not Found", None);
            }
            store.answers.push(((*id).to_owned(), (*action).to_owned()));
            ("202 Accepted", None)
        }
        _ => ("404 Not Found", None),
    }
}
