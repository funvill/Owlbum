//! The closing steps through Tauri's mock runtime
//! (docs/08-desktop-and-release.md, "Closing and quitting"): a quit whose
//! album close never finishes still exits after 5 seconds. The exit goes to
//! a hook, so the test process is not ended.

mod common;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::invoke;
use owlbum_core::appdata::AppData;
use owlbum_lib::closing::{CLOSE_WAIT, Closing, Exit};
use owlbum_lib::session::Session;
use serde_json::json;
use tauri::Manager;
use tauri::test::{MockRuntime, mock_builder};

struct Fixture {
    app: common::TestApp,
    window: tauri::WebviewWindow<MockRuntime>,
    exits: Arc<Mutex<Vec<(Exit, Instant)>>>,
    /// Last: fields drop in order, and the folder cannot be deleted while
    /// a database in it is open.
    _tmp: tempfile::TempDir,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let data = AppData::open(&tmp.path().join("data")).unwrap();
    let (app, window) = common::window(owlbum_lib::with_app_data(
        owlbum_lib::setup(mock_builder()),
        data,
    ));
    let exits = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&exits);
    app.state::<Closing>().set_exit_hook(Arc::new(move |how| {
        sink.lock().unwrap().push((how, Instant::now()));
    }));
    Fixture {
        app,
        window,
        exits,
        _tmp: tmp,
    }
}

impl Fixture {
    /// Waits for the first exit, at most `limit`.
    fn exit_within(&self, limit: Duration) -> Option<(Exit, Instant)> {
        let deadline = Instant::now() + limit;
        loop {
            if let Some(first) = self.exits.lock().unwrap().first() {
                return Some(*first);
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Quits as File > Quit does, with the page answering at once.
    fn quit(&self) -> Instant {
        let asked = Instant::now();
        invoke(&self.window, "app_quit", json!({})).unwrap();
        invoke(&self.window, "app_close_ready", json!({})).unwrap();
        asked
    }
}

#[test]
fn a_quit_exits_once_the_album_is_closed() {
    let f = fixture();
    let asked = f.quit();
    let (how, at) = f.exit_within(Duration::from_secs(10)).expect("an exit");
    assert_eq!(how, Exit::Closed);
    assert!(
        at - asked < CLOSE_WAIT,
        "no watchdog wait: {:?}",
        at - asked
    );
}

#[test]
fn a_quit_whose_album_close_never_finishes_exits_after_5_seconds() {
    let f = fixture();
    // An album open or close holds this gate; the quit's close waits on it
    // for as long as the test holds it.
    let session = f.app.state::<Session>();
    let stall = session.one_at_a_time();
    let asked = f.quit();
    let (how, at) = f
        .exit_within(CLOSE_WAIT + Duration::from_secs(10))
        .expect("the watchdog exits");
    assert_eq!(how, Exit::Stalled);
    let waited = at - asked;
    assert!(waited >= CLOSE_WAIT, "exited early: {waited:?}");
    // The close finishes late: it does not exit a second time.
    drop(stall);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(f.exits.lock().unwrap().len(), 1);
}
