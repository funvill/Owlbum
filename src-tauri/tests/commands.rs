//! The command registry, and commands wired through Tauri's mock runtime.

use std::collections::HashSet;

use owlbum_core::app::AppVersion;
use tauri::test::{
    INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets,
};
use tauri::webview::InvokeRequest;
use tauri::{WebviewWindow, WebviewWindowBuilder};

fn app_window() -> (tauri::App<MockRuntime>, WebviewWindow<MockRuntime>) {
    let app = owlbum_lib::setup(mock_builder())
        .build(mock_context(noop_assets()))
        .expect("the app builds");
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("the window builds");
    (app, window)
}

fn invoke(
    window: &WebviewWindow<MockRuntime>,
    cmd: &str,
    body: serde_json::Value,
) -> Result<tauri::ipc::InvokeResponseBody, serde_json::Value> {
    get_ipc_response(
        window,
        InvokeRequest {
            cmd: cmd.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: "http://tauri.localhost".parse().unwrap(),
            body: tauri::ipc::InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
}

#[test]
fn app_version_answers_the_workspace_version() {
    let (_app, window) = app_window();
    let reply = invoke(&window, "app_version", serde_json::json!({})).expect("app_version answers");
    let version: AppVersion = reply.deserialize().unwrap();
    assert_eq!(version.version, env!("CARGO_PKG_VERSION"));
}

#[test]
fn an_unknown_command_is_refused() {
    let (_app, window) = app_window();
    assert!(invoke(&window, "app_no_such_command", serde_json::json!({})).is_err());
}

#[test]
fn every_command_starts_with_its_area_prefix() {
    for m in owlbum_lib::commands::modules::<MockRuntime>() {
        for c in &m.commands {
            assert!(
                c.name.starts_with(&format!("{}_", m.prefix)),
                "command `{}` is in area `{}`",
                c.name,
                m.prefix
            );
        }
    }
}

#[test]
fn command_and_event_names_are_unique() {
    let mut commands = HashSet::new();
    let mut events = HashSet::new();
    for m in owlbum_lib::commands::modules::<MockRuntime>() {
        for c in &m.commands {
            assert!(
                commands.insert(c.name),
                "command `{}` is declared twice",
                c.name
            );
        }
        for e in &m.events {
            assert!(
                events.insert(e.name),
                "event `{}` is declared twice",
                e.name
            );
        }
    }
}

#[test]
fn area_prefixes_match_the_architecture_table() {
    let prefixes: Vec<&str> = owlbum_lib::commands::modules::<MockRuntime>()
        .iter()
        .map(|m| m.prefix)
        .collect();
    assert_eq!(
        prefixes,
        [
            "album", "app", "edit", "file", "photo", "query", "search", "tagging"
        ]
    );
}
