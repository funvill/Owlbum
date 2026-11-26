//! The scan commands and events through Tauri's mock runtime.

mod common;

use std::sync::mpsc;
use std::time::Duration;

use common::invoke;
use owlbum_core::album::{Album, OpenOptions};
use owlbum_core::scan::{ScanPhase, ScanProgress};
use tauri::test::{MockRuntime, mock_builder};
use tauri::{Listener, Manager};

fn app_window() -> (tauri::App<MockRuntime>, tauri::WebviewWindow<MockRuntime>) {
    common::window(owlbum_lib::setup(mock_builder()))
}

/// Every `scan_progress` the app sends, as it is sent.
fn progress_events(app: &tauri::App<MockRuntime>) -> mpsc::Receiver<ScanProgress> {
    let (send, receive) = mpsc::channel();
    app.listen_any("scan_progress", move |event| {
        let p: ScanProgress = serde_json::from_str(event.payload()).unwrap();
        let _ = send.send(p);
    });
    receive
}

/// Waits for the next `scan_progress` that ends a scan.
fn next_finished(events: &mpsc::Receiver<ScanProgress>) -> ScanProgress {
    loop {
        let p = events
            .recv_timeout(Duration::from_secs(60))
            .expect("the scan finishes");
        if p.finished {
            return p;
        }
    }
}

#[test]
fn rescan_without_an_album_answers_no_album() {
    let (_app, window) = app_window();
    let err = invoke(&window, "album_rescan", serde_json::json!({})).unwrap_err();
    assert_eq!(err["code"], "no_album");
}

#[test]
fn cancel_without_an_album_is_not_an_error() {
    let (_app, window) = app_window();
    invoke(&window, "album_scan_cancel", serde_json::json!({})).expect("no error");
}

#[test]
fn opening_scans_and_rescan_scans_again_with_progress_events() {
    let tmp = tempfile::tempdir().unwrap();
    let fixture = owlbum_fixtures::trip(&tmp.path().join("trip")).unwrap();
    let (app, window) = app_window();
    let events = progress_events(&app);

    let album = Album::open(&fixture.root, &OpenOptions::default()).unwrap();
    owlbum_lib::session::open_album(app.handle(), album);
    let first = next_finished(&events);
    assert!(!first.cancelled);
    assert_eq!(first.phase, ScanPhase::Idle);
    // Every file but the HEIC has a row.
    assert_eq!(first.found as usize, fixture.files.len() - 1);
    assert_eq!(first.done, first.found);

    invoke(&window, "album_rescan", serde_json::json!({})).expect("album_rescan answers");
    let second = next_finished(&events);
    assert_eq!(second.found, first.found);
    // A Rescan retries the rows with errors: the corrupt file, the RAW and
    // the video fail again.
    assert_eq!(second.failed, 3);

    invoke(&window, "album_scan_cancel", serde_json::json!({})).expect("no error");
    app.state::<owlbum_lib::session::Session>().close();
}
