//! The app data commands (`app_settings_get`, `app_settings_set`,
//! `app_recent_list`, `app_recent_remove`) through Tauri's mock runtime,
//! each test with its own app data folder.

mod common;

use common::invoke;
use owlbum_core::appdata::{AppData, Recent, Settings};
use serde_json::json;
use tauri::test::{MockRuntime, mock_builder};

struct Fixture {
    data: AppData,
    _app: common::TestApp,
    window: tauri::WebviewWindow<MockRuntime>,
    /// Last, so it is dropped last: fields drop in order, and on Windows
    /// the folder cannot be deleted while a database in it is still open.
    _tmp: tempfile::TempDir,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let data = AppData::open(&tmp.path().join("data")).unwrap();
    let (app, window) = common::window(owlbum_lib::with_app_data(
        owlbum_lib::setup(mock_builder()),
        data.clone(),
    ));
    Fixture {
        _tmp: tmp,
        data,
        _app: app,
        window,
    }
}

#[test]
fn settings_get_answers_every_setting_with_defaults() {
    let f = fixture();
    let reply = invoke(&f.window, "app_settings_get", json!({})).unwrap();
    let settings: Settings = reply.deserialize().unwrap();
    assert_eq!(settings, Settings::default());
}

#[test]
fn settings_set_saves_the_patch_and_answers_every_setting() {
    let f = fixture();
    let reply = invoke(
        &f.window,
        "app_settings_set",
        json!({ "patch": { "thumb_size": 240, "panel.grid_width": 500 } }),
    )
    .unwrap();
    let settings: Settings = reply.deserialize().unwrap();
    assert_eq!(settings.thumb_size, 240);
    assert_eq!(settings.panel_grid_width, 500);
    assert_eq!(f.data.settings().unwrap(), settings, "saved in app.sqlite");
}

#[test]
fn settings_set_refuses_a_value_out_of_range_and_changes_nothing() {
    let f = fixture();
    let err = invoke(
        &f.window,
        "app_settings_set",
        json!({ "patch": { "view": "map", "cache.limit_gb": 5000 } }),
    )
    .unwrap_err();
    assert_eq!(err["code"], "setting_out_of_range");
    assert_eq!(err["details"], json!({ "key": "cache.limit_gb" }));
    assert!(err["message"].as_str().unwrap().contains("cache.limit_gb"));
    assert_eq!(f.data.settings().unwrap(), Settings::default());
}

#[test]
fn recent_list_answers_newest_first_with_exists() {
    let f = fixture();
    let here = f.data.dir().to_string_lossy().into_owned();
    let gone = f.data.dir().join("gone").to_string_lossy().into_owned();
    f.data.touch_recent(&here, 1).unwrap();
    f.data.touch_recent(&gone, 2).unwrap();
    let reply = invoke(&f.window, "app_recent_list", json!({})).unwrap();
    let list: Vec<Recent> = reply.deserialize().unwrap();
    let got: Vec<(&str, bool)> = list.iter().map(|r| (r.root.as_str(), r.exists)).collect();
    assert_eq!(got, [(gone.as_str(), false), (here.as_str(), true)]);
    assert_eq!(list[0].name, "gone");
}

#[test]
fn recent_remove_takes_out_one_entry() {
    let f = fixture();
    f.data.touch_recent("/photos/a", 1).unwrap();
    f.data.touch_recent("/photos/b", 2).unwrap();
    let reply = invoke(
        &f.window,
        "app_recent_remove",
        json!({ "root": "/photos/b" }),
    )
    .unwrap();
    let list: Vec<Recent> = reply.deserialize().unwrap();
    assert_eq!(
        list.iter().map(|r| r.root.as_str()).collect::<Vec<_>>(),
        ["/photos/a"]
    );
}

#[test]
fn app_about_answers_the_version_the_data_folder_and_the_repository() {
    let f = fixture();
    let reply = invoke(&f.window, "app_about", json!({})).expect("app_about answers");
    let about: owlbum_core::app::About = reply.deserialize().unwrap();
    assert_eq!(about.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(about.repository, "https://github.com/funvill/Owlbum");
    assert_eq!(about.data_dir, f.data.dir().display().to_string());
}
