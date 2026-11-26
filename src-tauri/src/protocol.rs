//! The `owlbum://` protocol (docs/01-architecture.md, "The `owlbum://`
//! protocol"): pictures for the page.
//!
//! It is registered with Tauri's asynchronous handler. The handler itself
//! runs on the main thread, so it only hands the request to the blocking
//! pool; [`respond`] does the work there. [`respond`] is a plain function,
//! so tests call it without a webview. What to answer is decided in
//! `owlbum_core::thumbs::serve`; this module only turns it into HTTP.

use owlbum_core::thumbs::Thumbs;
use owlbum_core::thumbs::serve::{self, Reply};
use tauri::http::{Request, Response, header};
use tauri::{AppHandle, Manager, Runtime};

use crate::session::Session;

/// The scheme's name. On Windows the webview addresses it as
/// `http://owlbum.localhost/…`.
pub const SCHEME: &str = "owlbum";

/// Registers the protocol on a Tauri builder.
pub fn register<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.register_asynchronous_uri_scheme_protocol(SCHEME, |ctx, request, responder| {
        let app = ctx.app_handle().clone();
        // Never decode or read a database on the main thread.
        tauri::async_runtime::spawn_blocking(move || {
            responder.respond(respond(&app, &request));
        });
    })
}

/// Answers one request: the open album's picture the URL names, or 404.
pub fn respond<R: Runtime, B>(app: &AppHandle<R>, request: &Request<B>) -> Response<Vec<u8>> {
    let url = request.uri().to_string();
    let reply = match app.try_state::<Thumbs>() {
        Some(thumbs) => {
            // Hold the open album, not the session's lock, while a picture
            // is made: closing the album does not wait for it.
            let scanner = app.try_state::<Session>().and_then(|s| s.scanner());
            serve::answer(scanner.as_deref().map(|s| s.open()), &thumbs, &url)
        }
        None => Reply::not_found(),
    };
    to_http(reply)
}

/// The reply as an HTTP response.
pub fn to_http(reply: Reply) -> Response<Vec<u8>> {
    Response::builder()
        .status(reply.status)
        .header(header::CONTENT_TYPE, reply.content_type)
        .header(header::CACHE_CONTROL, reply.cache_control)
        // The page draws edits by applying the edit plan to a thumbnail on
        // a canvas, which needs the picture to be readable from the page's
        // own origin (tauri://localhost or http://tauri.localhost).
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .body(reply.body)
        .unwrap_or_else(|_| Response::new(Vec::new()))
}
