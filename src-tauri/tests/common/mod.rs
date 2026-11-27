//! Helpers for shell tests that call commands through Tauri's mock runtime.

#![allow(dead_code)]

use owlbum_core::appdata::AppData;
use owlbum_core::thumbs::Thumbs;
use owlbum_lib::session::Session;
use tauri::test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_context, noop_assets};
use tauri::webview::InvokeRequest;
use tauri::{Manager, WebviewWindow, WebviewWindowBuilder};

/// The address the page is served from. The permission check treats only this
/// origin as the app's own page, and it differs by platform.
pub const PAGE_URL: &str = if cfg!(any(windows, target_os = "android")) {
    "http://tauri.localhost"
} else {
    "tauri://localhost"
};

/// Builds the app and its `main` window on the mock runtime.
pub fn window(builder: tauri::Builder<MockRuntime>) -> (TestApp, WebviewWindow<MockRuntime>) {
    let app = builder
        .build(mock_context(noop_assets()))
        .expect("the app builds");
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("the window builds");
    (TestApp(app), window)
}

/// An app on the mock runtime that lets go of the folders it uses when it
/// is dropped.
///
/// Tauri never frees what an app manages: its managers and the handles kept
/// by its plugins and listeners point at one another. Left alone, the open
/// album and the app data stay open after the test, and on Windows the
/// test's temporary folder cannot be deleted (`tempfile` ignores the
/// failure and the folder is left behind, issue #97). Dropping this closes
/// the album and takes the app data and thumbnail cache out of the app.
pub struct TestApp(pub tauri::App<MockRuntime>);

impl std::ops::Deref for TestApp {
    type Target = tauri::App<MockRuntime>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for TestApp {
    // `unmanage` leaves references taken with `state` dangling. None
    // outlive the app, which is being dropped.
    #[allow(deprecated)]
    fn drop(&mut self) {
        if let Some(session) = self.0.try_state::<Session>() {
            session.close();
        }
        drop(self.0.unmanage::<Thumbs>());
        // Tagging holds the app data too, and its thread is stopped.
        drop(self.0.unmanage::<owlbum_lib::commands::tagging::TaggingState>());
        drop(self.0.unmanage::<AppData>());
    }
}

/// Calls a command as the page would, with a JSON arguments object.
pub fn invoke(
    window: &WebviewWindow<MockRuntime>,
    cmd: &str,
    args: serde_json::Value,
) -> Result<tauri::ipc::InvokeResponseBody, serde_json::Value> {
    get_ipc_response(
        window,
        InvokeRequest {
            cmd: cmd.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: PAGE_URL.parse().unwrap(),
            body: tauri::ipc::InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
}
