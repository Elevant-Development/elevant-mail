//! Mail HTML made safe to show. The cache keeps the original HTML and this runs each time a
//! message is opened, so a stricter sanitizer also covers mail that is already cached.
//!
//! This is the first of two layers: the frontend shows the result in a sandboxed frame without
//! scripts and with its own Content Security Policy (see docs/projekt-brief.md, "Mail-HTML").

use std::borrow::Cow;
use std::sync::{Arc, Mutex, PoisonError};

use ammonia::UrlRelative;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};

/// How a message is going to be shown.
#[derive(Debug, Clone, Copy)]
pub struct HtmlView<'a> {
    /// Base of the app's asset protocol: `mail-asset://localhost`, on Windows
    /// `http://mail-asset.localhost`. Inline and external images are served from there.
    pub asset_base: &'a str,
    /// Load external images (through the asset protocol); otherwise they are left out.
    pub remote_images: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafeHtml {
    pub html: String,
    /// External image URLs in document order; `<asset_base>/remote/<message>/<n>` serves the
    /// n-th once allowed.
    pub remote_images: Vec<String>,
    /// Sets its own colours or backgrounds. Dark mode then shows it on a light page instead of
    /// adapting it, since such mail is built for a light background.
    pub styled: bool,
}

/// Tags whose content goes too, not just the tag: their text would otherwise end up in the body.
const DROP_WITH_CONTENT: [&str; 4] = ["script", "title", "noscript", "template"];

/// Layout attributes old-school mail (and most newsletters) still relies on.
const LAYOUT_ATTRIBUTES: [&str; 17] = [
    "style",
    "class",
    "id",
    "align",
    "valign",
    "width",
    "height",
    "bgcolor",
    "color",
    "border",
    "cellpadding",
    "cellspacing",
    "dir",
    "face",
    "size",
    "nowrap",
    "role",
];

pub fn sanitize(html: &str, message_id: &str, view: HtmlView<'_>) -> SafeHtml {
    let remote = Arc::new(Mutex::new(Vec::new()));
    let images = ImageRewriter {
        asset_base: view.asset_base.trim_end_matches('/').to_owned(),
        message_id: message_id.to_owned(),
        remote_images: view.remote_images,
        found: Arc::clone(&remote),
    };

    let html = ammonia::Builder::default()
        // Style sheets stay: newsletters need them, and the frame's CSP keeps them from loading
        // anything (fonts, imports, background images from the web).
        .add_tags(["style", "font"])
        .rm_clean_content_tags(["style"])
        .add_clean_content_tags(DROP_WITH_CONTENT)
        .add_generic_attributes(LAYOUT_ATTRIBUTES)
        .add_tag_attributes("img", ["src", "alt", "width", "height"])
        // Checked before the filter below, which then decides per element.
        .url_schemes(["http", "https", "mailto", "cid", "data"].into())
        .url_relative(UrlRelative::Deny)
        .link_rel(Some("noopener noreferrer"))
        .attribute_filter(
            move |element, attribute, value| match (element, attribute) {
                ("img", "src") => images.rewrite(value).map(Cow::Owned),
                ("a", "href") => is_link(value).then_some(Cow::Borrowed(value)),
                _ => Some(Cow::Borrowed(value)),
            },
        )
        .clean(html)
        .to_string();

    let remote_images = std::mem::take(&mut *remote.lock().unwrap_or_else(PoisonError::into_inner));
    SafeHtml {
        styled: sets_colours(&html),
        html,
        remote_images,
    }
}

struct ImageRewriter {
    asset_base: String,
    message_id: String,
    remote_images: bool,
    found: Arc<Mutex<Vec<String>>>,
}

impl ImageRewriter {
    fn rewrite(&self, src: &str) -> Option<String> {
        let src = src.trim();
        let scheme = src
            .split_once(':')
            .map(|(scheme, _)| scheme.to_ascii_lowercase());
        match scheme.as_deref() {
            Some("cid") => Some(format!(
                "{}/cid/{}/{}",
                self.asset_base,
                self.message_id,
                utf8_percent_encode(&src[4..], NON_ALPHANUMERIC)
            )),
            Some("data") if src[5..].to_ascii_lowercase().starts_with("image/") => {
                Some(src.to_owned())
            }
            Some("http" | "https") => {
                let mut found = self.found.lock().unwrap_or_else(PoisonError::into_inner);
                found.push(src.to_owned());
                self.remote_images.then(|| {
                    format!(
                        "{}/remote/{}/{}",
                        self.asset_base,
                        self.message_id,
                        found.len() - 1
                    )
                })
            }
            _ => None,
        }
    }
}

fn is_link(href: &str) -> bool {
    let href = href.trim_start().to_ascii_lowercase();
    ["http:", "https:", "mailto:"]
        .iter()
        .any(|scheme| href.starts_with(scheme))
}

/// Whether the mail picks its own colours: in attributes, inline styles or style sheets.
fn sets_colours(html: &str) -> bool {
    let html = html.to_ascii_lowercase();
    ["bgcolor", "background", "color:", " color="]
        .iter()
        .any(|marker| html.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW: HtmlView<'static> = HtmlView {
        asset_base: "mail-asset://localhost",
        remote_images: false,
    };

    fn clean(html: &str) -> String {
        sanitize(html, "m1", VIEW).html
    }

    #[test]
    fn removes_scripts_and_event_handlers() {
        let html = clean(
            r#"<p onclick="steal()">Hi</p><script>alert(1)</script><img src=x onerror="steal()">
<svg><script>alert(2)</script></svg><noscript>fallback</noscript>"#,
        );
        assert!(!html.contains("script"), "{html}");
        assert!(!html.contains("onclick"), "{html}");
        assert!(!html.contains("onerror"), "{html}");
        assert!(!html.contains("alert"), "{html}");
        assert!(!html.contains("fallback"), "{html}");
        assert!(html.contains("<p>Hi</p>"), "{html}");
    }

    #[test]
    fn removes_forms_frames_meta_and_base() {
        let html = clean(
            r#"<meta http-equiv="refresh" content="0;url=https://evil.example"><base href="https://evil.example/">
<form action="https://evil.example"><input name="password"><button>Login</button></form>
<iframe src="https://evil.example"></iframe><object data="x"></object><embed src="x">
<link rel="stylesheet" href="https://evil.example/track.css"><title>Newsletter</title>"#,
        );
        for gone in [
            "meta",
            "refresh",
            "base",
            "form",
            "input",
            "iframe",
            "object",
            "embed",
            "link",
            "evil",
            "Newsletter",
        ] {
            assert!(!html.contains(gone), "{gone} survived: {html}");
        }
        // The button's label is plain text now.
        assert!(html.contains("Login"), "{html}");
    }

    #[test]
    fn keeps_only_web_and_mail_links() {
        let html = clean(
            r#"<a href="https://example.org/a">web</a><a href="mailto:lena@example.org">mail</a>
<a href="javascript:steal()">js</a><a href="JaVaScRiPt:steal()">js2</a><a href="data:text/html,x">data</a>
<a href="file:///etc/passwd">file</a><a href="/relative">relative</a>"#,
        );
        assert!(html.contains(r#"href="https://example.org/a""#), "{html}");
        assert!(html.contains(r#"href="mailto:lena@example.org""#), "{html}");
        assert!(html.contains(r#"rel="noopener noreferrer""#), "{html}");
        for gone in ["javascript", "JaVaScRiPt", "data:", "file:", "/relative"] {
            assert!(!html.contains(gone), "{gone} survived: {html}");
        }
    }

    #[test]
    fn blocks_external_images_by_default() {
        let safe = sanitize(
            r#"<img src="https://tracker.example/pixel.gif" width="1" height="1"><img src="http://cdn.example/logo.png" alt="Logo">"#,
            "m1",
            VIEW,
        );
        assert!(!safe.html.contains("tracker"), "{}", safe.html);
        assert!(!safe.html.contains("src="), "{}", safe.html);
        assert!(safe.html.contains(r#"alt="Logo""#), "{}", safe.html);
        assert_eq!(
            safe.remote_images,
            [
                "https://tracker.example/pixel.gif",
                "http://cdn.example/logo.png"
            ]
        );
    }

    #[test]
    fn allowed_external_images_go_through_the_asset_protocol() {
        let safe = sanitize(
            r#"<img src="https://cdn.example/a.png"><img src="https://cdn.example/b.png">"#,
            "m1",
            HtmlView {
                asset_base: "http://mail-asset.localhost/",
                remote_images: true,
            },
        );
        assert!(
            safe.html
                .contains(r#"src="http://mail-asset.localhost/remote/m1/0""#),
            "{}",
            safe.html
        );
        assert!(safe.html.contains("/remote/m1/1"), "{}", safe.html);
        assert!(!safe.html.contains("cdn.example"), "{}", safe.html);
    }

    #[test]
    fn inline_and_embedded_images() {
        let html = clean(
            r#"<img src="cid:logo@example.org"><img src="data:image/png;base64,iVBORw0KGgo=">
<img src="data:text/html;base64,PHNjcmlwdD4="><img src="mail-asset://localhost/remote/other/0">"#,
        );
        assert!(
            html.contains(r#"src="mail-asset://localhost/cid/m1/logo%40example%2Eorg""#),
            "{html}"
        );
        assert!(html.contains("data:image/png"), "{html}");
        assert!(!html.contains("data:text"), "{html}");
        // Mail cannot point at assets of other messages.
        assert!(!html.contains("other"), "{html}");
    }

    #[test]
    fn keeps_newsletter_layout_and_styles() {
        let safe = sanitize(
            r##"<style>.box { background: #f4f4f4; }</style>
<table width="600" cellpadding="0" bgcolor="#ffffff"><tr><td class="box" style="padding: 8px">Hi</td></tr></table>"##,
            "m1",
            VIEW,
        );
        assert!(safe.html.contains("<style>.box"), "{}", safe.html);
        assert!(safe.html.contains(r#"width="600""#), "{}", safe.html);
        assert!(safe.html.contains(r#"class="box""#), "{}", safe.html);
        assert!(
            safe.html.contains(r#"style="padding: 8px""#),
            "{}",
            safe.html
        );
        assert!(safe.styled);
    }

    #[test]
    fn plain_html_mail_is_not_styled() {
        let safe = sanitize(
            "<div>Hallo Alex,</div><div><br></div><blockquote>Zitat</blockquote><p><b>Gruß</b></p>",
            "m1",
            VIEW,
        );
        assert!(!safe.styled);
        assert!(safe.remote_images.is_empty());
    }
}
