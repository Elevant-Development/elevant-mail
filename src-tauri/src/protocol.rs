//! `mail-asset://`: images for the mail frame. Inline images (`cid:`) come from the cache;
//! external images are fetched by the app once the user allowed them for that mail, so the
//! frame itself never reaches the web.

use std::borrow::Cow;

use elevant_mail_core::mailbox;
use percent_encoding::percent_decode_str;
use tauri::http::{Request, Response, StatusCode, header};
use tauri::{AppHandle, Manager, Runtime, UriSchemeContext, UriSchemeResponder};

use crate::state::AppState;

pub const SCHEME: &str = "mail-asset";

/// How the frame addresses the protocol: Windows maps custom schemes to `http://<scheme>.localhost`.
pub fn asset_base() -> &'static str {
    if cfg!(windows) {
        "http://mail-asset.localhost"
    } else {
        "mail-asset://localhost"
    }
}

pub fn handle<R: Runtime>(
    context: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = context.app_handle().clone();
    let path = request.uri().path().to_owned();
    tauri::async_runtime::spawn(async move {
        responder.respond(respond(&app, &path).await);
    });
}

async fn respond<R: Runtime>(app: &AppHandle<R>, path: &str) -> Response<Vec<u8>> {
    let state = app.state::<AppState>();
    let segments: Vec<Cow<'_, str>> = path
        .trim_start_matches('/')
        .split('/')
        .map(|segment| percent_decode_str(segment).decode_utf8_lossy())
        .collect();

    let image = match segments.as_slice() {
        [kind, message_id, content_id] if kind == "cid" => {
            mailbox::inline_image(&state.db.conn(), message_id, content_id)
                .inspect_err(|error| tracing::warn!(%error, "could not read an inline image"))
                .ok()
                .flatten()
        }
        [kind, message_id, index] if kind == "remote" => {
            let url = index
                .parse()
                .ok()
                .and_then(|index| state.allowed_remote_image(message_id, index));
            match url {
                Some(url) => match state.remote_images.fetch(&url).await {
                    Ok(image) => Some((image.mime_type, image.data)),
                    // The address itself stays out of the log.
                    Err(error) => {
                        tracing::info!(%error, "external image not loaded");
                        None
                    }
                },
                None => None,
            }
        }
        _ => None,
    };

    let response = match image {
        Some((mime_type, data)) if mime_type.starts_with("image/") => Response::builder()
            .header(header::CONTENT_TYPE, mime_type)
            .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
            // An SVG opened on its own must not run scripts either.
            .header(
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; style-src 'unsafe-inline'",
            )
            .header(header::CACHE_CONTROL, "private, max-age=3600")
            .body(data),
        _ => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Vec::new()),
    };
    response.unwrap_or_else(|_| Response::new(Vec::new()))
}
