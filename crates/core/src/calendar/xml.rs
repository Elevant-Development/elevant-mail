//! The XML of WebDAV (RFC 4918) and CalDAV (RFC 4791) answers, read into a small tree: element
//! names with their namespace, attributes, text and children. Answers are small (a calendar's list of
//! events with their ETags), so a tree is simpler than reading them as a stream.

use quick_xml::NsReader;
use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::ResolveResult;

pub(crate) const DAV: &str = "DAV:";
pub(crate) const CALDAV: &str = "urn:ietf:params:xml:ns:caldav";
pub(crate) const CARDDAV: &str = "urn:ietf:params:xml:ns:carddav";
/// `getctag`, a calendar's version, as Apple's and most other servers report it.
pub(crate) const CALENDARSERVER: &str = "http://calendarserver.org/ns/";
/// `calendar-color`.
pub(crate) const APPLE_ICAL: &str = "http://apple.com/ns/ical/";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Element {
    pub namespace: String,
    pub name: String,
    /// By local name; namespaced attributes do not occur in these answers.
    pub attributes: Vec<(String, String)>,
    pub text: String,
    pub children: Vec<Element>,
}

impl Element {
    pub fn is(&self, namespace: &str, name: &str) -> bool {
        self.namespace == namespace && self.name == name
    }

    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// The first child of that name.
    pub fn child(&self, namespace: &str, name: &str) -> Option<&Element> {
        self.children.iter().find(|child| child.is(namespace, name))
    }

    pub fn children_named<'a>(
        &'a self,
        namespace: &'a str,
        name: &'a str,
    ) -> impl Iterator<Item = &'a Element> {
        self.children
            .iter()
            .filter(move |child| child.is(namespace, name))
    }

    /// Text inside, trimmed; `None` when empty.
    pub fn value(&self) -> Option<&str> {
        Some(self.text.trim()).filter(|text| !text.is_empty())
    }

    /// The text of a `href` child, as in `<current-user-principal><href>…</href>`.
    pub fn href(&self) -> Option<&str> {
        self.child(DAV, "href").and_then(Element::value)
    }
}

/// Reads a document into its root element; `None` for something that is not XML.
pub(crate) fn parse(xml: &str) -> Option<Element> {
    let mut reader = NsReader::from_str(xml);
    let mut stack: Vec<Element> = Vec::new();
    let mut root = None;
    let element = |resolved: ResolveResult<'_>, start: &BytesStart<'_>| Element {
        namespace: match resolved {
            ResolveResult::Bound(namespace) => namespace.into_inner().to_owned(),
            _ => String::new(),
        },
        name: start.local_name().as_ref().to_owned(),
        attributes: start
            .attributes()
            .flatten()
            .filter_map(|attribute| {
                Some((
                    attribute.key.local_name().as_ref().to_owned(),
                    attribute
                        .normalized_value(XmlVersion::Implicit1_0)
                        .ok()?
                        .into_owned(),
                ))
            })
            .collect(),
        ..Element::default()
    };
    loop {
        let (resolved, event) = reader.read_resolved_event().ok()?;
        match event {
            Event::Start(start) => {
                stack.push(element(resolved, &start));
            }
            Event::Empty(empty) => {
                let child = element(resolved, &empty);
                match stack.last_mut() {
                    Some(parent) => parent.children.push(child),
                    None => root = Some(child),
                }
            }
            Event::End(_) => {
                let done = stack.pop()?;
                match stack.last_mut() {
                    Some(parent) => parent.children.push(done),
                    None => root = Some(done),
                }
            }
            Event::Text(text) => {
                if let Some(current) = stack.last_mut() {
                    current.text.push_str(&text.xml10_content());
                }
            }
            Event::CData(data) => {
                if let Some(current) = stack.last_mut() {
                    current.text.push_str(&data.into_inner());
                }
            }
            Event::GeneralRef(reference) => {
                let character = if reference.is_char_ref() {
                    reference.resolve_char_ref().ok().flatten()
                } else {
                    match &*reference {
                        "amp" => Some('&'),
                        "lt" => Some('<'),
                        "gt" => Some('>'),
                        "quot" => Some('"'),
                        "apos" => Some('\''),
                        _ => None,
                    }
                };
                if let (Some(current), Some(character)) = (stack.last_mut(), character) {
                    current.text.push(character);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    root
}

/// One `response` of a `multistatus`: the resource and its properties that were found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Response {
    pub href: String,
    /// The properties from the `propstat` with status 200; missing ones are left out.
    pub props: Vec<Element>,
    /// The response's own status, for members that are gone (`404` in a `sync-collection`).
    pub status: Option<u16>,
}

impl Response {
    pub fn prop(&self, namespace: &str, name: &str) -> Option<&Element> {
        self.props.iter().find(|prop| prop.is(namespace, name))
    }
}

/// The code from a status line like `HTTP/1.1 200 OK`.
fn status_code(line: &str) -> Option<u16> {
    line.split_whitespace().nth(1)?.parse().ok()
}

/// The responses of a `207 Multi-Status` answer.
pub(crate) fn multistatus(xml: &str) -> Vec<Response> {
    let Some(root) = parse(xml) else {
        return Vec::new();
    };
    root.children_named(DAV, "response")
        .filter_map(|response| {
            let href = response.href()?.to_owned();
            let props = response
                .children_named(DAV, "propstat")
                .filter(|propstat| {
                    propstat
                        .child(DAV, "status")
                        .and_then(Element::value)
                        .and_then(status_code)
                        .is_none_or(|code| (200..300).contains(&code))
                })
                .filter_map(|propstat| propstat.child(DAV, "prop"))
                .flat_map(|prop| prop.children.iter().cloned())
                .collect();
            Some(Response {
                href,
                props,
                status: response
                    .child(DAV, "status")
                    .and_then(Element::value)
                    .and_then(status_code),
            })
        })
        .collect()
}

/// Text for an XML element's content: `&`, `<` and `>` escaped.
pub(crate) fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_multistatus_with_its_namespaces() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<d:multistatus xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav" xmlns:cs="http://calendarserver.org/ns/">
  <d:response>
    <d:href>/dav/alex/work/</d:href>
    <d:propstat>
      <d:prop>
        <d:displayname>Arbeit &amp; Team</d:displayname>
        <d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>
        <cal:supported-calendar-component-set><cal:comp name="VEVENT"/></cal:supported-calendar-component-set>
        <cs:getctag>"42"</cs:getctag>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
    <d:propstat>
      <d:prop><cal:calendar-description/></d:prop>
      <d:status>HTTP/1.1 404 Not Found</d:status>
    </d:propstat>
  </d:response>
  <d:response>
    <d:href>/dav/alex/work/gone.ics</d:href>
    <d:status>HTTP/1.1 404 Not Found</d:status>
  </d:response>
</d:multistatus>"#;
        let responses = multistatus(xml);
        assert_eq!(responses.len(), 2);
        let calendar = &responses[0];
        assert_eq!(calendar.href, "/dav/alex/work/");
        assert_eq!(
            calendar.prop(DAV, "displayname").and_then(Element::value),
            Some("Arbeit & Team")
        );
        assert!(
            calendar
                .prop(DAV, "resourcetype")
                .unwrap()
                .child(CALDAV, "calendar")
                .is_some()
        );
        assert_eq!(
            calendar
                .prop(CALENDARSERVER, "getctag")
                .and_then(Element::value),
            Some("\"42\"")
        );
        assert!(calendar.prop(CALDAV, "calendar-description").is_none());
        let components = calendar
            .prop(CALDAV, "supported-calendar-component-set")
            .unwrap();
        assert_eq!(components.children[0].attribute("name"), Some("VEVENT"));
        assert_eq!(responses[1].status, Some(404));
    }

    #[test]
    fn calendar_data_keeps_its_line_breaks() {
        let xml = "<multistatus xmlns=\"DAV:\"><response><href>/a.ics</href><propstat><prop>\
<calendar-data xmlns=\"urn:ietf:params:xml:ns:caldav\">BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n</calendar-data>\
</prop><status>HTTP/1.1 200 OK</status></propstat></response></multistatus>";
        let responses = multistatus(xml);
        let data = responses[0].prop(CALDAV, "calendar-data").unwrap();
        assert!(
            data.text.starts_with("BEGIN:VCALENDAR\n"),
            "{:?}",
            data.text
        );
    }
}
