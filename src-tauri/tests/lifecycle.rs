//! The app's lifecycle commands through Tauri's mock runtime: the updater
//! with a fake source and against a local test feed, What's new, and the
//! page's log lines (docs/08-desktop-and-release.md, "Updates", "What's
//! new", "Logs"). No test reaches the network: the real updater reads a
//! feed served on 127.0.0.1.

mod common;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::invoke;
use owlbum_core::app::update::{UpdateMessage, UpdateState, UpdateStateKind};
use owlbum_core::app::whats_new::WhatsNewEntry;
use owlbum_core::appdata::{AppData, Settings};
use owlbum_lib::update::{UpdateSource, Updates};
use serde_json::json;
use tauri::test::{MockRuntime, mock_builder};
use tauri::{Listener, Manager};

struct Fixture {
    data: AppData,
    app: common::TestApp,
    window: tauri::WebviewWindow<MockRuntime>,
    /// Last: fields drop in order, and the folder cannot be deleted while
    /// a database in it is open.
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
        data,
        app,
        window,
        _tmp: tmp,
    }
}

impl Fixture {
    /// Collects the payloads of one event.
    fn collect(&self, event: &str) -> Arc<Mutex<Vec<serde_json::Value>>> {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        self.app.listen_any(event, move |e| {
            sink.lock()
                .unwrap()
                .push(serde_json::from_str(e.payload()).unwrap());
        });
        seen
    }

    fn state(&self) -> UpdateState {
        invoke(&self.window, "app_update_state", json!({}))
            .unwrap()
            .deserialize()
            .unwrap()
    }

    /// Waits until the updater reaches `kind`.
    fn wait_for(&self, kind: UpdateStateKind) -> UpdateState {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let s = self.state();
            if s.state == kind {
                return s;
            }
            assert!(
                Instant::now() < deadline,
                "stuck at {s:?}, waiting for {kind:?}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

fn messages(seen: &Mutex<Vec<serde_json::Value>>) -> Vec<UpdateMessage> {
    seen.lock()
        .unwrap()
        .iter()
        .map(|v| serde_json::from_value(v.clone()).unwrap())
        .collect()
}

/// A source that answers what it is told.
struct Fake {
    found: Mutex<Result<Option<String>, String>>,
    download: Mutex<Result<(), String>>,
    problem: Mutex<Option<String>>,
}

impl UpdateSource for Fake {
    fn check(&self) -> Result<Option<String>, String> {
        self.found.lock().unwrap().clone()
    }
    fn download(&self, progress: &mut dyn FnMut(u8)) -> Result<(), String> {
        progress(50);
        progress(100);
        self.download.lock().unwrap().clone()
    }
    fn install_problem(&self) -> Option<String> {
        self.problem.lock().unwrap().clone()
    }
    fn install(&self) -> Result<(), String> {
        Err("not in a test".into())
    }
}

fn with_fake(f: &Fixture) -> Arc<Fake> {
    let fake = Arc::new(Fake {
        found: Mutex::new(Ok(None)),
        download: Mutex::new(Ok(())),
        problem: Mutex::new(None),
    });
    f.app
        .state::<Updates>()
        .configure(Some(Arc::clone(&fake) as Arc<dyn UpdateSource>), false);
    fake
}

#[test]
fn without_a_key_the_updater_is_unavailable_and_says_so() {
    let f = fixture();
    let seen = f.collect("update_message");
    assert_eq!(f.state().state, UpdateStateKind::Unavailable);
    invoke(&f.window, "app_check_updates", json!({})).unwrap();
    assert_eq!(messages(&seen), [UpdateMessage::NotSetUp]);
    f.app.state::<Updates>().configure(None, true);
    invoke(&f.window, "app_check_updates", json!({})).unwrap();
    assert_eq!(messages(&seen)[1], UpdateMessage::DevBuild);
}

#[test]
fn a_manual_check_offers_a_newer_version_then_downloads_it() {
    let f = fixture();
    let fake = with_fake(&f);
    *fake.found.lock().unwrap() = Ok(Some("99.0.0".into()));
    let seen = f.collect("update_message");
    let states = f.collect("update_state");
    invoke(&f.window, "app_check_updates", json!({})).unwrap();
    let s = f.wait_for(UpdateStateKind::Available);
    assert_eq!(s.version.as_deref(), Some("99.0.0"));
    assert_eq!(
        messages(&seen),
        [UpdateMessage::Found {
            latest: "99.0.0".into(),
            current: env!("CARGO_PKG_VERSION").into()
        }]
    );
    assert!(
        f.data.settings().unwrap().updates_last_check.is_some(),
        "the check is recorded"
    );

    invoke(&f.window, "app_update_download", json!({})).unwrap();
    f.wait_for(UpdateStateKind::Ready);
    assert!(messages(&seen).contains(&UpdateMessage::Downloaded {
        version: "99.0.0".into()
    }));
    let percents: Vec<_> = states
        .lock()
        .unwrap()
        .iter()
        .filter(|s| s["state"] == "downloading")
        .map(|s| s["percent"].clone())
        .collect();
    assert_eq!(percents, [json!(0), json!(50), json!(100)]);
}

#[test]
fn an_older_version_is_never_offered_and_a_manual_check_says_up_to_date() {
    let f = fixture();
    let fake = with_fake(&f);
    *fake.found.lock().unwrap() = Ok(Some("0.0.1".into()));
    let seen = f.collect("update_message");
    invoke(&f.window, "app_check_updates", json!({})).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while messages(&seen).is_empty() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        messages(&seen),
        [UpdateMessage::UpToDate {
            current: env!("CARGO_PKG_VERSION").into()
        }]
    );
    assert_eq!(f.state().state, UpdateStateKind::Idle);
}

#[test]
fn a_failed_check_is_said_and_not_recorded() {
    let f = fixture();
    let fake = with_fake(&f);
    *fake.found.lock().unwrap() = Err("\nno network here\nmore".into());
    let seen = f.collect("update_message");
    invoke(&f.window, "app_check_updates", json!({})).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while messages(&seen).is_empty() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        messages(&seen),
        [UpdateMessage::CheckFailed {
            reason: "no network here".into()
        }]
    );
    assert_eq!(f.data.settings().unwrap().updates_last_check, None);
}

#[test]
fn a_failed_download_goes_back_to_the_offer() {
    let f = fixture();
    let fake = with_fake(&f);
    *fake.found.lock().unwrap() = Ok(Some("99.0.0".into()));
    *fake.download.lock().unwrap() = Err("signature mismatch".into());
    let seen = f.collect("update_message");
    invoke(&f.window, "app_check_updates", json!({})).unwrap();
    f.wait_for(UpdateStateKind::Available);
    invoke(&f.window, "app_update_download", json!({})).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while messages(&seen).len() < 2 {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        messages(&seen)[1],
        UpdateMessage::DownloadFailed {
            reason: "signature mismatch".into()
        }
    );
    assert_eq!(f.state().state, UpdateStateKind::Available);
}

#[test]
fn an_install_that_cannot_run_closes_nothing() {
    let f = fixture();
    let fake = with_fake(&f);
    *fake.found.lock().unwrap() = Ok(Some("99.0.0".into()));
    *fake.problem.lock().unwrap() = Some("the downloaded update is gone.".into());
    let seen = f.collect("update_message");
    let closing = f.collect("app_closing");
    invoke(&f.window, "app_check_updates", json!({})).unwrap();
    f.wait_for(UpdateStateKind::Available);
    invoke(&f.window, "app_update_download", json!({})).unwrap();
    f.wait_for(UpdateStateKind::Ready);
    invoke(&f.window, "app_update_install", json!({})).unwrap();
    assert_eq!(
        messages(&seen).last().unwrap(),
        &UpdateMessage::CannotInstall {
            problem: "the downloaded update is gone.".into()
        }
    );
    assert_eq!(f.state().state, UpdateStateKind::Ready);
    assert!(closing.lock().unwrap().is_empty(), "nothing was closed");
}

#[test]
fn the_first_start_notice_answer_is_recorded() {
    let f = fixture();
    let reply = invoke(
        &f.window,
        "app_update_notice_answer",
        json!({ "keep_on": false }),
    )
    .unwrap();
    let settings: Settings = reply.deserialize().unwrap();
    assert!(settings.updates_notice_seen);
    assert!(!settings.updates_auto);
    assert_eq!(f.data.settings().unwrap(), settings);
}

#[test]
fn release_notes_open_the_offered_versions_page() {
    let f = fixture();
    let fake = with_fake(&f);
    *fake.found.lock().unwrap() = Ok(Some("99.0.0".into()));
    let opened = Arc::new(Mutex::new(Vec::<String>::new()));
    let seen = Arc::clone(&opened);
    f.app
        .manage(owlbum_lib::commands::app::Browser(Arc::new(move |url| {
            seen.lock().unwrap().push(url.to_owned());
            Ok(())
        })));
    invoke(&f.window, "app_open_release_notes", json!({})).unwrap_err();
    invoke(&f.window, "app_check_updates", json!({})).unwrap();
    f.wait_for(UpdateStateKind::Available);
    invoke(&f.window, "app_open_release_notes", json!({})).unwrap();
    invoke(&f.window, "app_open_release_page", json!({})).unwrap();
    assert_eq!(
        *opened.lock().unwrap(),
        [
            "https://github.com/funvill/Owlbum/releases/tag/v99.0.0",
            "https://github.com/funvill/Owlbum/releases/latest"
        ]
    );
}

#[test]
fn whats_new_answers_the_running_versions_notes_and_records_when_seen() {
    let f = fixture();
    let reply = invoke(&f.window, "app_whats_new", json!({})).unwrap();
    let entries: Vec<WhatsNewEntry> = reply.deserialize().unwrap();
    assert!(
        entries
            .iter()
            .all(|e| e.version == env!("CARGO_PKG_VERSION"))
    );
    invoke(&f.window, "app_whats_new_seen", json!({})).unwrap();
    assert_eq!(
        f.data.settings().unwrap().whats_new_last_seen.as_deref(),
        Some(env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn a_development_build_shows_no_whats_new_at_start() {
    let f = fixture();
    let reply = invoke(&f.window, "app_startup", json!({})).unwrap();
    let startup: owlbum_core::app::Startup = reply.deserialize().unwrap();
    assert!(startup.whats_new.is_empty());
    assert_eq!(
        f.data.settings().unwrap().whats_new_last_seen,
        None,
        "nothing recorded"
    );
}

#[test]
fn the_page_may_log_a_line() {
    let f = fixture();
    invoke(
        &f.window,
        "app_log",
        json!({ "level": "error", "message": "x".repeat(5_000) }),
    )
    .unwrap();
}

#[test]
fn the_page_log_is_limited_to_100_lines_a_minute() {
    let log = owlbum_lib::commands::app::PageLog::new();
    let start = Instant::now();
    for _ in 0..100 {
        assert!(log.allow(start));
    }
    assert!(!log.allow(start + Duration::from_secs(59)));
    assert!(log.allow(start + Duration::from_secs(60)), "a new minute");
}

/// Serves `answers` in turn on 127.0.0.1, one request each.
fn feed(answers: Vec<(u16, String)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}/latest.json", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for (status, body) in answers {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut buf = [0_u8; 4096];
            let _ = stream.read(&mut buf);
            let reply = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(reply.as_bytes());
        }
    });
    address
}

fn release(version: &str) -> String {
    let platform = json!({ "signature": "c2ln", "url": "http://127.0.0.1:1/owlbum-setup.exe" });
    json!({
        "version": version,
        "notes": "",
        "pub_date": "2026-11-27T00:00:00Z",
        "platforms": {
            "windows-x86_64": platform,
            "windows-x86_64-nsis": platform,
            "linux-x86_64": platform,
            "linux-x86_64-appimage": platform,
            "linux-x86_64-deb": platform,
        }
    })
    .to_string()
}

/// The real updater plugin on the mock runtime, reading a local feed.
fn real_source(feed: &str) -> owlbum_lib::update::PluginSource<MockRuntime> {
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    context.config_mut().plugins.0.insert(
        "updater".into(),
        json!({ "pubkey": "test-key", "endpoints": [] }),
    );
    let app = mock_builder()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .build(context)
        .expect("the app builds");
    let handle = app.handle().clone();
    // The app lives as long as the test process; it holds no folder.
    std::mem::forget(app);
    owlbum_lib::update::PluginSource::new(handle, feed.parse().unwrap())
}

#[test]
fn the_real_updater_reads_the_test_feed() {
    let url = feed(vec![
        (200, release("99.0.0")),
        (200, release("0.0.1")),
        (404, "Not Found".into()),
        (500, "{\"oops\": 1}".into()),
    ]);
    let source = real_source(&url);
    assert_eq!(
        source.check(),
        Ok(Some("99.0.0".into())),
        "a newer version is offered"
    );
    assert_eq!(source.check(), Ok(None), "an older one is not");
    assert_eq!(source.check(), Ok(None), "404: nothing published yet");
    assert_eq!(
        source.check(),
        Ok(None),
        "a server error reads as nothing published"
    );
}
