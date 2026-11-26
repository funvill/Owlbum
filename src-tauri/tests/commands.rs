//! The command registry, and commands wired through Tauri's mock runtime.

mod common;

use std::collections::HashSet;

use common::invoke;
use owlbum_core::app::AppVersion;
use owlbum_core::media::FormatInfo;
use tauri::test::{MockRuntime, mock_builder};

fn app_window() -> (common::TestApp, tauri::WebviewWindow<MockRuntime>) {
    common::window(owlbum_lib::setup(mock_builder()))
}

#[test]
fn app_version_answers_the_workspace_version() {
    let (_app, window) = app_window();
    let reply = invoke(&window, "app_version", serde_json::json!({})).expect("app_version answers");
    let version: AppVersion = reply.deserialize().unwrap();
    assert_eq!(version.version, env!("CARGO_PKG_VERSION"));
}

#[test]
fn app_formats_answers_the_registry() {
    let (_app, window) = app_window();
    let reply = invoke(&window, "app_formats", serde_json::json!({})).expect("app_formats answers");
    let formats: Vec<FormatInfo> = reply.deserialize().unwrap();
    assert_eq!(formats, owlbum_core::media::formats());
    let first = serde_json::to_value(&formats[0]).unwrap();
    assert_eq!(first["id"], "jpeg");
    assert_eq!(first["save_as"], "jpeg");
    assert_eq!(first["kind"], "image");
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
