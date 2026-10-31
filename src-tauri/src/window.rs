//! The main window. It is built here instead of from tauri.conf.json (`"create": false`) so it
//! can catch what mail tries to open.

use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Emitter, Url, WebviewWindowBuilder};

/// Sent to the frontend with the address when a link in a mail is clicked.
pub const LINK_EVENT: &str = "mail-link";

/// `visible: false` when started at login: the tray opens it.
pub fn create_main(app: &AppHandle, visible: bool) -> tauri::Result<()> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == "main")
        .expect("tauri.conf.json defines the main window");
    let links = app.clone();
    WebviewWindowBuilder::from_config(app, config)?
        .visible(visible)
        // The mail frame opens every link in a new window (`<base target="_blank">`). None ever
        // opens in the app: the frontend shows the address and hands it to the system browser.
        .on_new_window(move |url, _features| {
            if is_mail_link(&url)
                && let Err(error) = links.emit(LINK_EVENT, url.as_str())
            {
                tracing::warn!(%error, "could not pass on a link");
            }
            NewWindowResponse::Deny
        })
        // Nothing in the window navigates away from the app; mail content in particular never
        // replaces it or its own frame.
        .on_navigation(is_app_page)
        .build()?;
    Ok(())
}

pub fn is_mail_link(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https" | "mailto")
}

fn is_app_page(url: &Url) -> bool {
    match url.scheme() {
        "about" | "tauri" => true,
        "http" | "https" => {
            // Windows serves the app from http://tauri.localhost, the dev server from localhost.
            url.host_str() == Some("tauri.localhost")
                || (cfg!(debug_assertions) && url.host_str() == Some("localhost"))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(value: &str) -> Url {
        Url::parse(value).unwrap()
    }

    #[test]
    fn stays_on_app_pages() {
        assert!(is_app_page(&url("tauri://localhost/index.html")));
        assert!(is_app_page(&url("http://tauri.localhost/")));
        assert!(is_app_page(&url("about:srcdoc")));
        assert!(!is_app_page(&url("https://example.org/")));
        assert!(!is_app_page(&url("http://tauri.localhost.example.org/")));
        assert!(!is_app_page(&url("file:///etc/passwd")));
    }

    #[test]
    fn passes_on_web_and_mail_links_only() {
        assert!(is_mail_link(&url("https://example.org/")));
        assert!(is_mail_link(&url("mailto:lena@example.org")));
        assert!(!is_mail_link(&url("file:///home")));
        assert!(!is_mail_link(&url("javascript:alert(1)")));
    }
}
