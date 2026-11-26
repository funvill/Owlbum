//! The scan commands and events through Tauri's mock runtime.

mod common;

use std::sync::mpsc;
use std::time::Duration;

use common::invoke;
use owlbum_core::album::{Album, OpenOptions};
use owlbum_core::scan::report::{ScanErrors, ScanReport, UnsupportedFiles};
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
fn the_report_commands_without_an_album_answer_no_album() {
    let (_app, window) = app_window();
    let page = serde_json::json!({ "offset": 0, "limit": 10 });
    for (command, args) in [
        ("album_scan_report", serde_json::json!({})),
        ("album_scan_errors", page.clone()),
        ("album_unsupported", page),
    ] {
        let err = invoke(&window, command, args).unwrap_err();
        assert_eq!(err["code"], "no_album", "{command}");
    }
}

#[test]
fn the_report_commands_answer_from_the_last_scan() {
    let tmp = tempfile::tempdir().unwrap();
    let fixture = owlbum_fixtures::extras(&tmp.path().join("extras")).unwrap();
    let (app, window) = app_window();
    let events = progress_events(&app);
    let album = Album::open(&fixture.root, &OpenOptions::default()).unwrap();
    owlbum_lib::session::open_album(app.handle(), album);
    next_finished(&events);

    let report: ScanReport = invoke(&window, "album_scan_report", serde_json::json!({}))
        .unwrap()
        .deserialize()
        .unwrap();
    assert_eq!(report.nested_album_count, 1);
    assert_eq!(report.nested_albums[0].rel_path, "nested");
    let unsupported_count = fixture
        .files
        .iter()
        .filter(|f| f.kind == owlbum_fixtures::Kind::Unsupported)
        .count();
    assert_eq!(
        report
            .unsupported
            .iter()
            .map(|u| u.count as usize)
            .sum::<usize>(),
        unsupported_count
    );

    let page = serde_json::json!({ "offset": 0, "limit": 500 });
    let unsupported: UnsupportedFiles = invoke(&window, "album_unsupported", page.clone())
        .unwrap()
        .deserialize()
        .unwrap();
    assert_eq!(unsupported.total as usize, unsupported_count);

    let errors: ScanErrors = invoke(&window, "album_scan_errors", page)
        .unwrap()
        .deserialize()
        .unwrap();
    assert!(errors.total > 0);
    assert!(errors.items.iter().all(|e| !e.code.is_empty()));
    app.state::<owlbum_lib::session::Session>().close();
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

#[test]
fn a_file_added_to_the_open_album_is_scanned_with_reason_watch() {
    use owlbum_core::album::{AlbumChanged, ChangeReason};
    use owlbum_core::scan::watch::WatchEnv;
    use owlbum_core::scan::watch::intake::Timing;

    let tmp = tempfile::tempdir().unwrap();
    let fixture = owlbum_fixtures::trip(&tmp.path().join("trip")).unwrap();
    let (app, _window) = app_window();
    let events = progress_events(&app);
    let (send, changes) = mpsc::channel();
    app.listen_any("album_changed", move |event| {
        let c: AlbumChanged = serde_json::from_str(event.payload()).unwrap();
        let _ = send.send(c);
    });

    let album = Album::open(&fixture.root, &OpenOptions::default()).unwrap();
    let env = owlbum_core::scan::ScanEnv::new(std::sync::Arc::new(
        owlbum_lib::events::TauriEmitter(app.handle().clone()),
    ));
    let watch = WatchEnv {
        timing: Timing {
            quiet: Duration::from_millis(200),
            most: Duration::from_secs(2),
            max_folders: 500,
        },
        ..WatchEnv::default()
    };
    let session = app.state::<owlbum_lib::session::Session>();
    session.open_with(album, env, watch);
    next_finished(&events);
    while changes.try_recv().is_ok() {}
    let watching = |s: &owlbum_lib::session::Session| {
        s.with(|open| Ok(open.scanner().status().watching)).unwrap()
    };
    let until = std::time::Instant::now() + Duration::from_secs(30);
    while !watching(&session) {
        assert!(std::time::Instant::now() < until, "the watch starts");
        std::thread::sleep(Duration::from_millis(50));
    }

    let copy_from = fixture.root.join(&fixture.files[0].rel_path);
    std::fs::copy(copy_from, fixture.root.join("added.jpg")).unwrap();
    let change = loop {
        let c = changes
            .recv_timeout(Duration::from_secs(30))
            .expect("album_changed arrives");
        if c.reason == ChangeReason::Watch {
            break c;
        }
    };
    assert!(change.revision > 0);
    // Focus while watched starts nothing.
    session.focused();
    session.close();
}
