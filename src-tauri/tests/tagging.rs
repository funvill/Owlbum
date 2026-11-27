//! The `tagging_` commands through Tauri's mock runtime. No model is
//! installed and nothing is downloaded.

mod common;

use common::invoke;
use owlbum_core::appdata::AppData;
use owlbum_core::tagging::runtime::TaggingProgress;
use owlbum_core::tagging::service::ModelsState;
use serde_json::json;
use tauri::test::{MockRuntime, mock_builder};

struct Fixture {
    _app: common::TestApp,
    window: tauri::WebviewWindow<MockRuntime>,
    /// Last: the folder goes after everything that uses it.
    _tmp: tempfile::TempDir,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let data = AppData::open(&tmp.path().join("data")).unwrap();
    let (app, window) = common::window(owlbum_lib::with_app_data(
        owlbum_lib::setup(mock_builder()),
        data,
    ));
    Fixture {
        _app: app,
        window,
        _tmp: tmp,
    }
}

#[test]
fn tagging_models_lists_the_catalogue_with_nothing_installed() {
    let f = fixture();
    let state: ModelsState = invoke(&f.window, "tagging_models", json!({}))
        .unwrap()
        .deserialize()
        .unwrap();
    let ids: Vec<&str> = state.models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(
        ids,
        ["florence2-base-q8", "florence2-large-q8", "clip-base-q8", "clip-large-q8"]
    );
    let value = serde_json::to_value(&state).unwrap();
    assert_eq!(value["models"][0]["state"], "absent");
    assert_eq!(value["settings"], json!({ "describe": null, "keywords": null }));
    assert_eq!(value["downloading"], json!(null));
}

#[test]
fn tagging_set_refuses_a_model_that_is_not_downloaded() {
    let f = fixture();
    let err = invoke(
        &f.window,
        "tagging_set",
        json!({ "describe": "florence2-base-q8", "keywords": null }),
    )
    .unwrap_err();
    assert_eq!(err["code"], "model_not_ready");
    let ok = invoke(&f.window, "tagging_set", json!({ "describe": null, "keywords": null })).unwrap();
    assert_eq!(ok.deserialize::<serde_json::Value>().unwrap(), json!({ "describe": null, "keywords": null }));
    let err = invoke(&f.window, "tagging_download", json!({ "model": "nope" })).unwrap_err();
    assert_eq!(err["code"], "no_such_model");
    let err = invoke(&f.window, "tagging_remove", json!({ "model": "nope" })).unwrap_err();
    assert_eq!(err["code"], "no_such_model");
}

#[test]
fn tagging_status_is_idle_with_no_model_and_pause_is_not_saved() {
    let f = fixture();
    invoke(&f.window, "tagging_pause", json!({ "paused": true })).unwrap();
    let s: TaggingProgress = invoke(&f.window, "tagging_status", json!({}))
        .unwrap()
        .deserialize()
        .unwrap();
    assert_eq!((s.total, s.running, s.paused), (0, false, true));
    invoke(&f.window, "tagging_focus", json!({ "ids": [1, 2] })).unwrap();
    let cleared = invoke(&f.window, "tagging_retry_failed", json!({})).unwrap();
    assert_eq!(cleared.deserialize::<serde_json::Value>().unwrap(), json!({ "cleared": 0 }));
}
