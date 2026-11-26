//! The query commands through Tauri's mock runtime.

mod common;

use std::sync::mpsc;
use std::time::Duration;

use common::invoke;
use owlbum_core::album::{Album, OpenOptions};
use owlbum_core::query::{Facets, QueryResult, Row};
use owlbum_core::scan::ScanProgress;
use serde_json::{Value, json};
use tauri::test::{MockRuntime, mock_builder};
use tauri::{Listener, Manager};

fn app_window() -> (common::TestApp, tauri::WebviewWindow<MockRuntime>) {
    common::window(owlbum_lib::setup(mock_builder()))
}

fn default_query() -> Value {
    json!({
        "view": "grid", "timeline_unit": "auto", "sort": "date", "stack": true,
        "open_stacks": [], "filter": {}, "search": null, "want_facets": false
    })
}

/// Opens the `trip` fixture and waits for its first scan to finish.
fn open_trip(app: &tauri::App<MockRuntime>, dir: &std::path::Path) {
    let fixture = owlbum_fixtures::trip(&dir.join("trip")).unwrap();
    let (send, receive) = mpsc::channel();
    app.listen_any("scan_progress", move |event| {
        let p: ScanProgress = serde_json::from_str(event.payload()).unwrap();
        if p.finished {
            let _ = send.send(());
        }
    });
    let album = Album::open(&fixture.root, &OpenOptions::default()).unwrap();
    owlbum_lib::session::open_album(app.handle(), album);
    receive
        .recv_timeout(Duration::from_secs(60))
        .expect("the scan finishes");
}

fn run(window: &tauri::WebviewWindow<MockRuntime>, args: Value) -> QueryResult {
    invoke(window, "query_run", args)
        .expect("query_run answers")
        .deserialize()
        .unwrap()
}

#[test]
fn the_query_commands_without_an_album_answer_no_album() {
    let (_app, window) = app_window();
    for (command, args) in [
        ("query_run", json!({ "query": default_query() })),
        (
            "query_window",
            json!({ "token": "q1", "offset": 0, "limit": 10 }),
        ),
        ("query_locate", json!({ "token": "q1", "id": 1 })),
        (
            "query_ids",
            json!({ "token": "q1", "offset": 0, "limit": 10 }),
        ),
    ] {
        let err = invoke(&window, command, args).unwrap_err();
        assert_eq!(err["code"], "no_album", "{command}");
    }
}

#[test]
fn a_query_with_a_value_not_allowed_answers_bad_query() {
    let tmp = tempfile::tempdir().unwrap();
    let (app, window) = app_window();
    open_trip(&app, tmp.path());
    let mut bad = default_query();
    bad["sort"] = json!("size");
    let err = invoke(&window, "query_run", json!({ "query": bad })).unwrap_err();
    assert_eq!(err["code"], "bad_query");
    let err = invoke(
        &window,
        "query_run",
        json!({ "query": default_query(), "keep": { "token": 3 } }),
    )
    .unwrap_err();
    assert_eq!(err["code"], "bad_query");
    let mut later = default_query();
    later["view"] = json!("timeline");
    let err = invoke(&window, "query_run", json!({ "query": later })).unwrap_err();
    assert_eq!(err["code"], "bad_query");
    let mut not_a_day = default_query();
    not_a_day["filter"] = json!({ "from": "2024-02-30" });
    let err = invoke(&window, "query_run", json!({ "query": not_a_day })).unwrap_err();
    assert_eq!(err["code"], "bad_query");
    app.state::<owlbum_lib::session::Session>().close();
}

#[test]
fn a_filtered_query_answers_chips_and_query_facets_its_counts() {
    let tmp = tempfile::tempdir().unwrap();
    let (app, window) = app_window();
    open_trip(&app, tmp.path());
    let all = run(&window, json!({ "query": default_query() }));
    let mut starred = default_query();
    starred["filter"] = json!({ "star": "yes" });
    starred["want_facets"] = json!(true);
    let r = run(&window, json!({ "query": starred }));
    assert!(r.filtered);
    assert_eq!(r.panel_count, 1);
    assert_eq!(r.chips[0].text, "Starred");
    assert!(r.chips[0].remove.is_empty());
    assert_eq!(r.total, all.total);
    let facets: Facets = invoke(&window, "query_facets", json!({ "token": r.token }))
        .unwrap()
        .deserialize()
        .unwrap();
    assert_eq!(Some(&facets), r.facets.as_ref());
    assert_eq!(facets.star.yes + facets.star.no, all.total);
    let err = invoke(&window, "query_facets", json!({ "token": "q-gone" })).unwrap_err();
    assert_eq!(err["code"], "query_expired");
    app.state::<owlbum_lib::session::Session>().close();
}

#[test]
fn run_window_locate_and_ids_work_together() {
    let tmp = tempfile::tempdir().unwrap();
    let (app, window) = app_window();
    open_trip(&app, tmp.path());

    let r = run(&window, json!({ "query": default_query() }));
    assert!(r.count > 0);
    assert_eq!(r.kept, None);
    let rows: Vec<Row> = invoke(
        &window,
        "query_window",
        json!({ "token": r.token, "offset": 0, "limit": 500 }),
    )
    .unwrap()
    .deserialize()
    .unwrap();
    assert_eq!(rows.len() as u32, r.count);
    assert!(rows.iter().all(|row| !row.thumb_key.is_empty()));

    let last = rows.last().unwrap();
    let located: Value = invoke(
        &window,
        "query_locate",
        json!({ "token": r.token, "id": last.id }),
    )
    .unwrap()
    .deserialize()
    .unwrap();
    assert_eq!(located, json!({ "index": r.count - 1 }));
    let missing: Value = invoke(
        &window,
        "query_locate",
        json!({ "token": r.token, "id": 987_654 }),
    )
    .unwrap()
    .deserialize()
    .unwrap();
    assert_eq!(missing, Value::Null);

    let ids: Vec<i64> = invoke(
        &window,
        "query_ids",
        json!({ "token": r.token, "offset": 0, "limit": 100_000 }),
    )
    .unwrap()
    .deserialize()
    .unwrap();
    assert_eq!(ids, rows.iter().map(|row| row.id).collect::<Vec<_>>());
    let err = invoke(
        &window,
        "query_ids",
        json!({ "token": "no-such-token", "offset": 0, "limit": 10 }),
    )
    .unwrap_err();
    assert_eq!(err["code"], "query_expired");

    let again = run(
        &window,
        json!({
            "query": default_query(),
            "keep": { "token": r.token, "current": { "id": last.id, "index": r.count - 1 }, "anchor_id": null, "selection": [last.id] }
        }),
    );
    let kept = again.kept.unwrap();
    assert_eq!(kept.current.unwrap().id, last.id);
    assert_eq!(kept.selection, [last.id]);

    let err = invoke(
        &window,
        "query_window",
        json!({ "token": "no-such-token", "offset": 0, "limit": 10 }),
    )
    .unwrap_err();
    assert_eq!(err["code"], "query_expired");
    app.state::<owlbum_lib::session::Session>().close();
}

#[test]
fn tokens_do_not_outlive_their_album() {
    let tmp = tempfile::tempdir().unwrap();
    let (app, window) = app_window();
    open_trip(&app, tmp.path());
    let r = run(&window, json!({ "query": default_query() }));
    open_trip(&app, &tmp.path().join("second"));
    let err = invoke(
        &window,
        "query_locate",
        json!({ "token": r.token, "id": 1 }),
    )
    .unwrap_err();
    assert_eq!(err["code"], "query_expired");
    app.state::<owlbum_lib::session::Session>().close();
}
