//! Helpers for shell tests that call commands through Tauri's mock runtime.

#![allow(dead_code)]

use tauri::test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_context, noop_assets};
use tauri::webview::InvokeRequest;
use tauri::{WebviewWindow, WebviewWindowBuilder};

/// The address the page is served from. The permission check treats only this
/// origin as the app's own page, and it differs by platform.
pub const PAGE_URL: &str = if cfg!(any(windows, target_os = "android")) {
    "http://tauri.localhost"
} else {
    "tauri://localhost"
};

/// Builds the app and its `main` window on the mock runtime.
pub fn window(
    builder: tauri::Builder<MockRuntime>,
) -> (tauri::App<MockRuntime>, WebviewWindow<MockRuntime>) {
    let app = builder
        .build(mock_context(noop_assets()))
        .expect("the app builds");
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("the window builds");
    (app, window)
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
