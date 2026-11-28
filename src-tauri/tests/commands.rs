//! The command registry, and commands wired through Tauri's mock runtime.

mod common;

use std::collections::HashSet;

use common::invoke;
use owlbum_core::app::AppVersion;
use owlbum_core::info::country::CountryMatch;
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
fn app_place_search_answers_countries_with_their_central_points() {
    let (_app, window) = app_window();
    let reply = invoke(
        &window,
        "app_place_search",
        serde_json::json!({ "text": "Fran" }),
    )
    .expect("app_place_search answers");
    let found: Vec<CountryMatch> = reply.deserialize().unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "France");
    assert_eq!(
        owlbum_core::info::country::country_at(found[0].lat, found[0].lon),
        Some("France")
    );

    let reply = invoke(
        &window,
        "app_place_search",
        serde_json::json!({ "text": "a" }),
    )
    .expect("app_place_search answers");
    let found: Vec<CountryMatch> = reply.deserialize().unwrap();
    assert_eq!(found.len(), 6);
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

#[test]
fn feedback_and_the_repository_open_fixed_addresses_in_the_browser() {
    use std::sync::{Arc, Mutex};
    use tauri::Manager;
    let (_app, window) = app_window();
    let opened = Arc::new(Mutex::new(Vec::<String>::new()));
    let seen = Arc::clone(&opened);
    window
        .app_handle()
        .manage(owlbum_lib::commands::app::Browser(Arc::new(move |url| {
            seen.lock().unwrap().push(url.to_owned());
            Ok(())
        })));
    invoke(&window, "app_open_feedback", serde_json::json!({})).expect("feedback opens");
    invoke(&window, "app_open_repository", serde_json::json!({})).expect("the repository opens");
    assert_eq!(
        *opened.lock().unwrap(),
        [
            "https://github.com/funvill/Owlbum/issues",
            "https://github.com/funvill/Owlbum"
        ]
    );
}

#[test]
fn app_country_at_names_the_country_of_a_point() {
    let (_app, window) = app_window();
    let reply = invoke(
        &window,
        "app_country_at",
        serde_json::json!({ "lat": 48.85837, "lon": 2.29448 }),
    )
    .expect("app_country_at answers");
    assert_eq!(
        reply.deserialize::<Option<String>>().unwrap().as_deref(),
        Some("France")
    );
    let reply = invoke(
        &window,
        "app_country_at",
        serde_json::json!({ "lat": 0.0, "lon": -30.0 }),
    )
    .expect("app_country_at answers at sea");
    assert_eq!(reply.deserialize::<Option<String>>().unwrap(), None);
}

/// A registry hive in memory, so no test touches the real one.
#[derive(Default)]
struct MapRegistry {
    keys: std::sync::Mutex<std::collections::BTreeMap<(String, String), String>>,
}

impl owlbum_core::app::assoc::Registry for MapRegistry {
    fn set(&self, key: &str, name: &str, value: &str) -> Result<(), String> {
        let mut keys = self.keys.lock().unwrap();
        keys.insert((key.to_owned(), name.to_owned()), value.to_owned());
        Ok(())
    }
    fn get(&self, key: &str, name: &str) -> Option<String> {
        let keys = self.keys.lock().unwrap();
        keys.get(&(key.to_owned(), name.to_owned())).cloned()
    }
    fn delete_value(&self, key: &str, name: &str) -> Result<(), String> {
        let mut keys = self.keys.lock().unwrap();
        keys.remove(&(key.to_owned(), name.to_owned()));
        Ok(())
    }
    fn delete_tree(&self, key: &str) -> Result<(), String> {
        let below = format!(r"{key}\");
        let mut keys = self.keys.lock().unwrap();
        keys.retain(|(k, _), _| k != key && !k.starts_with(&below));
        Ok(())
    }
}

#[test]
fn opening_pictures_with_owlbum_registers_it_and_opens_default_apps() {
    use owlbum_core::app::assoc::Association;
    use std::sync::{Arc, Mutex};
    use tauri::Manager;
    let (_app, window) = app_window();
    let status = |window: &tauri::WebviewWindow<tauri::test::MockRuntime>| -> Association {
        invoke(window, "app_associate_status", serde_json::json!({}))
            .unwrap()
            .deserialize()
            .unwrap()
    };
    // Not offered without a registry (Linux, for now).
    assert_eq!(
        status(&window),
        Association {
            available: false,
            on: false
        }
    );

    let registry = Arc::new(MapRegistry::default());
    let opened = Arc::new(Mutex::new(Vec::<String>::new()));
    let seen = Arc::clone(&opened);
    let handle = window.app_handle();
    handle.manage(owlbum_lib::commands::app::Browser(Arc::new(move |url| {
        seen.lock().unwrap().push(url.to_owned());
        Ok(())
    })));
    handle.manage(owlbum_lib::assoc::FileTypes {
        registry: registry.clone(),
        exe: std::path::PathBuf::from(r"C:\Owlbum\owlbum.exe"),
    });
    assert!(!status(&window).on);

    let on: Association = invoke(&window, "app_associate", serde_json::json!({ "on": true }))
        .unwrap()
        .deserialize()
        .unwrap();
    assert_eq!(
        on,
        Association {
            available: true,
            on: true
        }
    );
    assert!(status(&window).on);
    assert_eq!(*opened.lock().unwrap(), ["ms-settings:defaultapps"]);
    assert!(!registry.keys.lock().unwrap().is_empty());

    let off: Association = invoke(&window, "app_associate", serde_json::json!({ "on": false }))
        .unwrap()
        .deserialize()
        .unwrap();
    assert!(!off.on);
    assert!(
        registry.keys.lock().unwrap().is_empty(),
        "turning it off removes every entry"
    );
    assert_eq!(
        opened.lock().unwrap().len(),
        1,
        "Default apps opens only to turn it on"
    );
}
