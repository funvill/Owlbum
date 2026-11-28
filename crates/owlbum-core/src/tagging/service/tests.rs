//! Tests of the service and the queue with the fake backend, the fake
//! catalogue and the fake server. Nothing here touches the network or a
//! real model.

use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;

use serde_json::Value;

use super::*;
use crate::album::{Album, OpenOptions};
use crate::scan::{Request, ScanEnv, Scanner};
use crate::tagging::backend::BackendFactory;
use crate::tagging::download::fake::{FakeFetch, Fault};
use crate::tagging::fake::{self, FAKE_MODELS, FakeBackend, FakeControl};
use crate::tagging::runtime::{AlbumWork, Limits, Picture, Pictures, TaggingProgress};
use crate::tagging::store;

#[derive(Default)]
struct Recorder(Mutex<Vec<(&'static str, Value)>>);

impl Emitter for Recorder {
    fn emit_json(&self, name: &'static str, payload: Value) {
        self.0.lock().unwrap().push((name, payload));
    }
}

impl Recorder {
    fn named(&self, name: &str) -> Vec<Value> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| *n == name)
            .map(|(_, v)| v.clone())
            .collect()
    }
}

/// Pictures named after the photo's file: a flat 8 × 8 picture, or none
/// for a file name starting `unreadable`.
struct NamedPictures(Arc<Album>);

impl Pictures for NamedPictures {
    fn picture(&self, id: i64) -> Picture {
        let name: Option<String> = self
            .0
            .db()
            .read(|c| {
                Ok(c.query_row(
                    "SELECT file_name FROM photos WHERE id = ?1 AND missing = 0",
                    [id],
                    |r| r.get(0),
                )
                .ok())
            })
            .unwrap_or(None);
        match name {
            None => Picture::Gone,
            Some(n) if n.starts_with("unreadable") => Picture::Unreadable(n),
            Some(n) => Picture::Ready(image::RgbImage::new(8, 8), n),
        }
    }
}

/// The fields drop in order: the tagging service (and its hold on the
/// album) first, the folder last.
struct Fx {
    tagging: Tagging,
    control: Arc<FakeControl>,
    fetch: Arc<FakeFetch>,
    events: Arc<Recorder>,
    scanner: Option<Arc<Scanner>>,
    scanning: Arc<AtomicBool>,
    factory_calls: Arc<AtomicUsize>,
    root: std::path::PathBuf,
    _tmp: tempfile::TempDir,
}

fn write_jpeg(path: &Path, shade: u8) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut img = image::RgbImage::new(32, 24);
    for (x, y, px) in img.enumerate_pixels_mut() {
        *px = image::Rgb([shade, (x * 7) as u8, (y * 9) as u8]);
    }
    img.save(path).unwrap();
}

impl Fx {
    fn new(files: &[&str]) -> Self {
        Self::with_limits(
            files,
            Limits {
                progress_event: Duration::ZERO,
                tags_event: Duration::from_secs(5),
                ..Limits::default()
            },
        )
    }

    fn with_limits(files: &[&str], limits: Limits) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("album");
        for (i, f) in files.iter().enumerate() {
            write_jpeg(&root.join(f), (i * 37) as u8);
        }
        std::fs::create_dir_all(&root).unwrap();
        let data = AppData::open(&tmp.path().join("data")).unwrap();
        let control = Arc::new(FakeControl::default());
        let events = Arc::new(Recorder::default());
        let fetch = Arc::new(FakeFetch::serving(FAKE_MODELS));
        let factory_calls = Arc::new(AtomicUsize::new(0));
        let factory: BackendFactory = {
            let control = Arc::clone(&control);
            let calls = Arc::clone(&factory_calls);
            Arc::new(move || {
                calls.fetch_add(1, Ordering::Relaxed);
                Ok(Box::new(FakeBackend::new(Arc::clone(&control)))
                    as Box<dyn crate::tagging::backend::TagBackend>)
            })
        };
        let runtime = Runtime::new(
            factory,
            data.dir().join("models"),
            1,
            events.clone(),
            limits,
        );
        let tagging = Tagging::new(
            data,
            fake::catalogue(),
            fetch.clone(),
            events.clone(),
            runtime,
        )
        .with_stall(Duration::from_millis(200));
        Self {
            tagging,
            control,
            fetch,
            events,
            scanner: None,
            scanning: Arc::new(AtomicBool::new(false)),
            factory_calls,
            root,
            _tmp: tmp,
        }
    }

    /// Opens and scans the album, and gives it to the queue.
    fn open(&mut self) {
        let album = Album::open(&self.root, &OpenOptions::default()).unwrap();
        let scanner = Arc::new(Scanner::new(
            Arc::new(album),
            ScanEnv::new(self.events.clone()),
        ));
        scanner.scan_and_wait(Request::default());
        let album = Arc::clone(scanner.album());
        let scanning = Arc::clone(&self.scanning);
        self.tagging.runtime().attach(Some(AlbumWork {
            pictures: Arc::new(NamedPictures(Arc::clone(&album))),
            album,
            scanning: Arc::new(move || scanning.load(Ordering::Relaxed)),
        }));
        self.scanner = Some(scanner);
    }

    fn album(&self) -> &Arc<Album> {
        self.scanner.as_ref().unwrap().album()
    }

    fn id(&self, name: &str) -> i64 {
        self.album()
            .db()
            .read(|c| {
                Ok(
                    c.query_row("SELECT id FROM photos WHERE file_name = ?1", [name], |r| {
                        r.get(0)
                    })
                    .unwrap(),
                )
            })
            .unwrap()
    }

    /// Installs and verifies fake models and chooses them.
    fn choose(&self, describe: Option<&str>, keywords: Option<&str>) {
        for id in [describe, keywords].into_iter().flatten() {
            let m = fake::catalogue().get(id).unwrap();
            fake::install(self.tagging.models_dir(), m);
            install::verify(&install::model_dir(self.tagging.models_dir(), m), m, false);
        }
        self.tagging
            .set(describe.map(str::to_owned), keywords.map(str::to_owned))
            .unwrap();
    }

    fn status(&self) -> TaggingProgress {
        self.tagging.runtime().status()
    }

    /// Waits until `f` holds, for up to 10 s.
    fn until(&self, what: &str, f: impl Fn(&TaggingProgress) -> bool) -> TaggingProgress {
        let start = Instant::now();
        loop {
            let s = self.status();
            if f(&s) {
                return s;
            }
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "waited for {what}: {s:?}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn finished(&self) -> TaggingProgress {
        self.until("the queue to finish", |s| {
            s.total > 0 && s.done + s.failed == s.total && !s.running
        })
    }

    fn row(&self, name: &str, model: &str) -> Option<store::TagRow> {
        let id = self.id(name);
        self.album()
            .db()
            .read(|c| store::row(c, id, model))
            .unwrap()
    }

    fn rows(&self) -> i64 {
        self.album()
            .db()
            .read(|c| {
                Ok(c.query_row("SELECT count(*) FROM tags", [], |r| r.get(0))
                    .unwrap())
            })
            .unwrap()
    }
}

impl Drop for Fx {
    fn drop(&mut self) {
        self.tagging.shutdown();
        self.tagging.runtime().attach(None);
        if let Some(s) = self.scanner.take() {
            s.cancel();
        }
    }
}

const D: &str = "fake-describe-small";
const K: &str = "fake-keywords-small";

// ------------------------------------------------------------- models --

#[test]
fn models_state_lists_the_catalogue_and_checks_unverified_models() {
    let fx = Fx::new(&[]);
    let s = fx.tagging.models();
    assert_eq!(s.models.len(), 4);
    assert!(s.models.iter().all(|m| m.state == InstallState::Absent));
    let m = fake::catalogue().get(K).unwrap();
    fake::install(fx.tagging.models_dir(), m);
    let first = fx.tagging.models();
    let k = first.models.iter().find(|x| x.id == K).unwrap();
    assert_eq!(k.state, InstallState::Checking);
    assert_eq!(k.bytes_present, m.bytes());
    let start = Instant::now();
    while fx
        .tagging
        .models()
        .models
        .iter()
        .find(|x| x.id == K)
        .unwrap()
        .state
        != InstallState::Ok
    {
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn set_refuses_every_state_but_ok() {
    let fx = Fx::new(&[]);
    let refused = |d: Option<&str>, k: Option<&str>| {
        fx.tagging
            .set(d.map(str::to_owned), k.map(str::to_owned))
            .unwrap_err()
            .code
    };
    assert_eq!(refused(Some(D), None), ErrorCode::ModelNotReady, "absent");
    assert_eq!(refused(Some("nope"), None), ErrorCode::ModelNotReady);
    assert_eq!(
        refused(None, Some(D)),
        ErrorCode::ModelNotReady,
        "wrong part"
    );
    let m = fake::catalogue().get(D).unwrap();
    fake::install(fx.tagging.models_dir(), m);
    assert_eq!(
        refused(Some(D), None),
        ErrorCode::ModelNotReady,
        "unverified"
    );
    install::verify(&install::model_dir(fx.tagging.models_dir(), m), m, false);
    let saved = fx.tagging.set(Some(D.into()), None).unwrap();
    assert_eq!(saved.describe.as_deref(), Some(D));
    assert_eq!(fx.tagging.models().settings.describe.as_deref(), Some(D));
    assert_eq!(
        fx.tagging.set(None, None).unwrap(),
        TaggingChoice::default()
    );
}

#[test]
fn a_download_installs_and_reports_progress() {
    let fx = Fx::new(&[]);
    assert_eq!(
        fx.tagging.download("nope").unwrap_err().code,
        ErrorCode::NoSuchModel
    );
    fx.tagging.download(K).unwrap();
    let start = Instant::now();
    while fx
        .events
        .named("download_progress")
        .last()
        .is_none_or(|v| v["finished"] != true)
    {
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(10));
    }
    let last = fx.events.named("download_progress").pop().unwrap();
    assert_eq!(last["error"], Value::Null);
    assert_eq!(last["received"], last["total"]);
    let s = fx.tagging.models();
    assert_eq!(
        s.models.iter().find(|m| m.id == K).unwrap().state,
        InstallState::Ok
    );
    assert!(s.downloading.is_none());
    fx.tagging.set(None, Some(K.into())).unwrap();
}

#[test]
fn a_failed_download_is_worded_and_cleared_by_the_next() {
    let fx = Fx::new(&[]);
    fx.fetch.set_fault(Fault::NotFound);
    fx.tagging.download(K).unwrap();
    let start = Instant::now();
    while fx.tagging.models().error.is_none() {
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        fx.tagging.models().error.as_deref(),
        Some("Couldn't download Fake keywords small: Hugging Face answered 404 for model.bin.")
    );
    fx.fetch.set_fault(Fault::None);
    fx.tagging.download(K).unwrap();
    assert_eq!(fx.tagging.models().error, None);
    fx.tagging.cancel_download();
}

#[test]
fn one_download_at_a_time_and_cancel_returns_after_it_stopped() {
    let fx = Fx::new(&[]);
    fx.fetch.set_fault(Fault::Stall);
    let fx2 = Tagging::with_stall(fx.tagging.clone(), Duration::from_secs(30));
    fx2.download(K).unwrap();
    assert_eq!(fx2.download(D).unwrap_err().code, ErrorCode::DownloadBusy);
    std::thread::sleep(Duration::from_millis(50));
    fx2.cancel_download();
    let s = fx2.models();
    assert!(s.downloading.is_none());
    assert_eq!(s.error, None, "cancel is not an error");
    fx2.download(D).unwrap();
    fx2.cancel_download();
}

#[test]
fn remove_deletes_the_files_and_sets_the_part_off() {
    let fx = Fx::new(&[]);
    fx.choose(None, Some(K));
    assert_eq!(
        fx.tagging.remove("nope").unwrap_err().code,
        ErrorCode::NoSuchModel
    );
    fx.tagging.remove(K).unwrap();
    assert_eq!(fx.tagging.choice().keywords, None);
    assert!(!fx.tagging.models_dir().join(K).exists());
}

// -------------------------------------------------------------- queue --

#[test]
fn the_queue_tags_describe_then_keywords_and_counts() {
    let mut fx = Fx::new(&["lake.jpg", "harbour boats.jpg", "sub/dog park.jpg"]);
    fx.choose(Some(D), Some(K));
    fx.open();
    let s = fx.finished();
    assert_eq!((s.total, s.done, s.failed), (3, 3, 0));
    let d = fx.row("lake.jpg", D).unwrap();
    assert_eq!(d.description.as_deref(), Some("A picture of lake."));
    assert_eq!(d.objects[0].label, "lake");
    let k = fx.row("harbour boats.jpg", K).unwrap();
    assert_eq!(k.keywords[0].label, "harbour");
    let runs = fx.control.runs();
    assert_eq!(runs[0].0, Part::Describe);
    assert_eq!(runs[1].0, Part::Keywords);
    assert_eq!(
        runs[0].1, runs[1].1,
        "both models run on one photo before the next"
    );
    // The labels and search follow.
    let id = fx.id("lake.jpg");
    let labels: Vec<String> = fx
        .album()
        .db()
        .read(|c| {
            let mut st = c
                .prepare("SELECT label FROM photo_labels WHERE photo_id = ?1 ORDER BY kind")
                .unwrap();
            Ok(st
                .query_map([id], |r| r.get(0))
                .unwrap()
                .map(Result::unwrap)
                .collect())
        })
        .unwrap();
    assert_eq!(labels, ["lake", "lake"]);
    assert!(
        fx.events
            .named("album_changed")
            .iter()
            .any(|v| v["reason"] == "tags")
    );
    assert!(!fx.events.named("tagging_progress").is_empty());
}

#[test]
fn focus_comes_before_scan_order() {
    let mut fx = Fx::new(&["a.jpg", "b.jpg", "c.jpg", "d.jpg"]);
    fx.control.set_delay(Duration::from_millis(30));
    fx.choose(None, Some(K));
    // Opening clears the pause, so the queue is held by a scan instead
    // until the focus is set: nothing may start before it.
    fx.scanning.store(true, Ordering::Relaxed);
    fx.open();
    let d = fx.id("d.jpg");
    let c = fx.id("c.jpg");
    fx.tagging.runtime().focus(&[d, c]);
    fx.scanning.store(false, Ordering::Relaxed);
    fx.tagging.runtime().pause(false);
    fx.finished();
    let names: Vec<String> = fx.control.runs().into_iter().map(|(_, n)| n).collect();
    assert_eq!(names, ["d.jpg", "c.jpg", "a.jpg", "b.jpg"]);
}

#[test]
fn nothing_runs_while_paused_or_scanning() {
    let mut fx = Fx::new(&["a.jpg", "b.jpg"]);
    fx.choose(None, Some(K));
    fx.scanning.store(true, Ordering::Relaxed);
    fx.open();
    fx.until("the counts", |s| s.total == 2);
    std::thread::sleep(Duration::from_millis(100));
    assert!(fx.control.runs().is_empty(), "a scan is running");
    fx.tagging.runtime().pause(true);
    fx.scanning.store(false, Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(700));
    assert!(fx.control.runs().is_empty(), "paused");
    assert!(fx.status().paused);
    fx.tagging.runtime().pause(false);
    assert_eq!(fx.finished().done, 2);
}

#[test]
fn a_view_only_album_is_not_tagged() {
    let mut fx = Fx::new(&["a.jpg"]);
    fx.choose(None, Some(K));
    // Open once so the database exists, then open view-only.
    fx.open();
    fx.finished();
    fx.tagging.runtime().attach(None);
    fx.scanner = None;
    std::fs::write(
        fx.root.join("b.jpg"),
        std::fs::read(fx.root.join("a.jpg")).unwrap(),
    )
    .unwrap();
    let options = OpenOptions {
        view_only: true,
        ..OpenOptions::default()
    };
    let album = Arc::new(Album::open(&fx.root, &options).unwrap());
    assert!(album.is_view_only());
    fx.tagging.runtime().attach(Some(AlbumWork {
        pictures: Arc::new(NamedPictures(Arc::clone(&album))),
        album,
        scanning: Arc::new(|| false),
    }));
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(fx.status().total, 0);
    assert_eq!(fx.control.runs().len(), 1);
    fx.tagging.runtime().attach(None);
}

#[test]
fn failures_store_rows_losses_stop_the_part_and_restart_clears() {
    let mut fx = Fx::new(&["fail-cat.jpg", "unreadable.jpg", "ok.jpg"]);
    fx.choose(None, Some(K));
    fx.open();
    let s = fx.finished();
    assert_eq!((s.total, s.done, s.failed), (3, 1, 2));
    let r = fx.row("fail-cat.jpg", K).unwrap();
    assert_eq!(r.status, store::Status::Failed);
    assert_eq!(
        r.error.as_deref(),
        Some("The fake model refused this picture.")
    );
    assert_eq!(
        fx.row("unreadable.jpg", K).unwrap().error.as_deref(),
        Some("Couldn't read the photo.")
    );
    // Not retried by itself; Try failed photos again clears and re-runs.
    let before = fx.control.runs().len();
    assert_eq!(fx.tagging.runtime().retry_failed().unwrap(), 2);
    let s = fx.until("the retry", |s| {
        s.failed == 2 && !s.running && fx.control.runs().len() > before
    });
    assert_eq!(s.done, 1);
}

#[test]
fn three_losses_stop_a_part_and_two_then_a_success_do_not() {
    let mut fx = Fx::new(&["lost-a.jpg", "lost-b.jpg", "lost-c.jpg"]);
    fx.control.set_lost(2);
    fx.choose(None, Some(K));
    fx.open();
    let s = fx.finished();
    assert_eq!(s.done, 3, "two losses, then a success");
    assert!(s.not_working.is_empty());

    let mut fx = Fx::new(&["lost-a.jpg", "lost-b.jpg"]);
    fx.control.set_lost(100);
    fx.choose(None, Some(K));
    fx.open();
    let s = fx.until("the part to stop", |s| {
        !s.not_working.is_empty() && !s.running
    });
    assert_eq!(s.not_working, [Part::Keywords]);
    assert_eq!(fx.rows(), 0, "a loss writes no row");
    fx.control.set_lost(0);
    fx.tagging.runtime().restart();
    assert_eq!(fx.finished().done, 2);
}

#[test]
fn a_timeout_stores_a_failed_row_and_counts_as_a_loss() {
    let mut fx = Fx::new(&["slow-a.jpg", "ok.jpg"]);
    fx.choose(Some(D), None);
    fx.open();
    let s = fx.finished();
    assert_eq!((s.done, s.failed), (1, 1));
    assert_eq!(
        fx.row("slow-a.jpg", D).unwrap().error.as_deref(),
        Some("Tagging this photo took too long.")
    );
}

#[test]
fn a_failed_load_stops_the_part_at_once() {
    let mut fx = Fx::new(&["a.jpg"]);
    fx.control.fail_next_load();
    fx.choose(Some(D), None);
    fx.open();
    let s = fx.until("the part to stop", |s| s.not_working == [Part::Describe]);
    assert_eq!(s.done, 0);
    assert_eq!(fx.tagging.models().not_working, [Part::Describe]);
    fx.tagging.runtime().restart();
    assert_eq!(fx.finished().done, 1);
}

#[test]
fn a_copy_takes_its_twins_rows_with_no_run() {
    let mut fx = Fx::new(&["lake.jpg"]);
    std::fs::create_dir_all(fx.root.join("b")).unwrap();
    std::fs::copy(fx.root.join("lake.jpg"), fx.root.join("b/lake again.jpg")).unwrap();
    fx.choose(None, Some(K));
    fx.open();
    let s = fx.finished();
    assert_eq!((s.total, s.done), (2, 2));
    assert_eq!(fx.control.runs().len(), 1);
    assert_eq!(
        fx.row("lake again.jpg", K).unwrap().keywords[0].label,
        "lake"
    );
}

#[test]
fn a_photo_added_later_is_tagged_and_closing_mid_run_writes_nothing() {
    let mut fx = Fx::new(&["a.jpg"]);
    fx.choose(None, Some(K));
    fx.open();
    fx.finished();
    write_jpeg(&fx.root.join("new photo.jpg"), 200);
    fx.scanner
        .as_ref()
        .unwrap()
        .scan_and_wait(Request::default());
    fx.tagging.runtime().album_changed();
    let s = fx.until("the new photo", |s| s.total == 2 && s.done == 2);
    assert!(!s.running || s.done == 2);

    let mut fx = Fx::new(&["a.jpg", "b.jpg"]);
    fx.control.set_delay(Duration::from_millis(300));
    fx.choose(None, Some(K));
    fx.open();
    fx.until("a run", |_| !fx.control.runs().is_empty());
    fx.tagging.runtime().attach(None);
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(fx.rows(), 0);
    assert_eq!(fx.status().total, 0);
}

#[test]
fn reading_the_status_changes_nothing_and_an_earlier_session_counts() {
    let mut fx = Fx::new(&["a.jpg", "b.jpg"]);
    fx.choose(None, Some(K));
    fx.open();
    fx.finished();
    let runs = fx.control.runs().len();
    fx.tagging.runtime().attach(None);
    fx.open();
    let s = fx.until("the counts", |s| s.total == 2);
    assert_eq!(fx.status(), fx.status());
    assert_eq!(s.done, 2);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(
        fx.control.runs().len(),
        runs,
        "photos tagged before are not looked at again"
    );
    assert_eq!(fx.factory_calls.load(Ordering::Relaxed), 1);
}

#[test]
fn the_runtime_that_will_not_start_stops_both_parts() {
    let tmp = tempfile::tempdir().unwrap();
    let data = AppData::open(tmp.path()).unwrap();
    let events = Arc::new(Recorder::default());
    let runtime = Runtime::new(
        Arc::new(|| Err("no library".to_owned())),
        data.dir().join("models"),
        1,
        events.clone(),
        Limits::default(),
    );
    let root = tmp.path().join("album");
    write_jpeg(&root.join("a.jpg"), 1);
    let m = fake::catalogue().get(K).unwrap();
    fake::install(&data.dir().join("models"), m);
    install::verify(&install::model_dir(&data.dir().join("models"), m), m, false);
    let album = Album::open(&root, &OpenOptions::default()).unwrap();
    let scanner = Arc::new(Scanner::new(Arc::new(album), ScanEnv::new(events.clone())));
    scanner.scan_and_wait(Request::default());
    runtime.set_active([None, Some(m)]);
    runtime.attach(Some(AlbumWork {
        pictures: Arc::new(NamedPictures(Arc::clone(scanner.album()))),
        album: Arc::clone(scanner.album()),
        scanning: Arc::new(|| false),
    }));
    let start = Instant::now();
    while runtime.status().problem.is_none() {
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(10));
    }
    let s = runtime.status();
    assert_eq!(
        s.problem.as_deref(),
        Some("The model runtime would not start: no library")
    );
    assert_eq!(s.not_working, [Part::Describe, Part::Keywords]);
    runtime.shutdown();
    runtime.attach(None);
    drop(runtime);
    scanner.cancel();
    drop(scanner);
}

// ------------------------------------------------------- what's in it --

#[test]
fn photo_tags_follow_the_states_table() {
    use crate::tagging::tags::TagsState;
    let mut fx = Fx::new(&["lake dog.jpg", "fail-cat.jpg"]);
    fx.open();
    let id = fx.id("lake dog.jpg");
    let tags = |fx: &Fx, id| fx.tagging.photo_tags(fx.album(), id).unwrap();
    assert_eq!(tags(&fx, id).state, TagsState::Off);
    fx.tagging.runtime().pause(true);
    fx.choose(Some(D), Some(K));
    let t = tags(&fx, id);
    assert_eq!(t.state, TagsState::Waiting);
    assert_eq!(t.waiting_for.as_deref(), Some("Fake describer small"));
    assert_eq!(t.description, None);
    fx.tagging.runtime().pause(false);
    fx.finished();
    let t = tags(&fx, id);
    assert_eq!(t.state, TagsState::Done);
    assert_eq!(t.description.as_deref(), Some("A picture of lake dog."));
    assert_eq!(t.written_by.as_deref(), Some("Fake describer small"));
    assert_eq!(t.keywords, ["lake", "dog"]);
    assert_eq!(t.objects[0].label, "lake");
    assert_eq!(t.objects[0].count, 2);
    assert!(t.can_regenerate);
    assert_eq!(t.waiting_for, None);
    let failed = tags(&fx, fx.id("fail-cat.jpg"));
    assert_eq!(failed.state, TagsState::Failed);
    assert_eq!(
        failed.error.as_deref(),
        Some("The fake model refused this picture.")
    );

    // Another describe model: the old results stay, waiting for the new one.
    fx.tagging.runtime().pause(true);
    fx.choose(Some("fake-describe-large"), Some(K));
    let t = tags(&fx, id);
    assert_eq!(t.state, TagsState::Waiting);
    assert_eq!(t.waiting_for.as_deref(), Some("Fake describer large"));
    assert_eq!(t.description.as_deref(), Some("A picture of lake dog."));
    assert!(!t.can_regenerate, "the shown row is not the active model's");

    // Off, with rows from before: done.
    fx.tagging.set(None, None).unwrap();
    let t = tags(&fx, id);
    assert_eq!(t.state, TagsState::Done);
    assert!(!t.can_regenerate);
}

#[test]
fn regenerate_replaces_only_the_sentence_for_the_photo_and_its_copies() {
    let mut fx = Fx::new(&["lake.jpg", "other.jpg"]);
    std::fs::create_dir_all(fx.root.join("b")).unwrap();
    std::fs::copy(fx.root.join("lake.jpg"), fx.root.join("b/lake again.jpg")).unwrap();
    fx.choose(Some(D), Some(K));
    fx.open();
    fx.finished();
    let id = fx.id("lake.jpg");
    let before_k = fx.row("lake.jpg", K).unwrap();
    let before_other = fx.row("other.jpg", D).unwrap();
    // It works while paused.
    fx.tagging.runtime().pause(true);
    let t = fx.tagging.regenerate(fx.album(), id).unwrap();
    assert_eq!(t.description.as_deref(), Some("Another look at lake (1)."));
    let d = fx.row("lake.jpg", D).unwrap();
    assert_eq!(d.description.as_deref(), Some("Another look at lake (1)."));
    assert_eq!(d.objects[0].label, "lake", "objects are kept");
    assert_eq!(fx.row("lake.jpg", K).unwrap(), before_k, "keywords are kept");
    assert_eq!(
        fx.row("lake again.jpg", D).unwrap().description.as_deref(),
        Some("Another look at lake (1).")
    );
    assert_eq!(fx.row("other.jpg", D).unwrap(), before_other);
    let t = fx.tagging.regenerate(fx.album(), id).unwrap();
    assert_eq!(t.description.as_deref(), Some("Another look at lake (2)."));
    assert!(
        fx.events
            .named("album_changed")
            .iter()
            .any(|v| v["reason"] == "tags")
    );
}

#[test]
fn regenerate_tries_once_more_after_a_lost_model_and_words_its_errors() {
    let mut fx = Fx::new(&["lost-lake.jpg", "b.jpg"]);
    fx.open();
    let lake = fx.id("lost-lake.jpg");
    let e = fx.tagging.regenerate(fx.album(), lake).unwrap_err();
    assert_eq!(e.code, ErrorCode::NoDescribeModel);

    fx.tagging.runtime().pause(true);
    fx.choose(Some(D), None);
    let e = fx.tagging.regenerate(fx.album(), lake).unwrap_err();
    assert_eq!(e.code, ErrorCode::NotDescribedYet);
    assert_eq!(
        e.message,
        "Fake describer small has not described this photo yet."
    );
    let e = fx.tagging.regenerate(fx.album(), 999_999).unwrap_err();
    assert_eq!(e.code, ErrorCode::PhotoNotFound);

    fx.tagging.runtime().pause(false);
    fx.finished();
    // One loss: the second attempt works.
    fx.control.set_lost(1);
    let t = fx.tagging.regenerate(fx.album(), lake).unwrap();
    assert!(
        t.description
            .unwrap()
            .starts_with("Another look at lost lake")
    );
    // Two losses: tagging_lost, and the old sentence stays.
    fx.control.set_lost(2);
    let before = fx.row("lost-lake.jpg", D).unwrap().description;
    let e = fx.tagging.regenerate(fx.album(), lake).unwrap_err();
    assert_eq!(e.code, ErrorCode::TaggingLost);
    assert_eq!(e.message, "Fake describer small stopped working. Try again.");
    assert_eq!(fx.row("lost-lake.jpg", D).unwrap().description, before);
}

#[test]
fn regenerate_in_a_view_only_album_is_refused() {
    let mut fx = Fx::new(&["a.jpg"]);
    fx.choose(Some(D), None);
    fx.open();
    fx.finished();
    let id = fx.id("a.jpg");
    fx.tagging.runtime().attach(None);
    if let Some(s) = fx.scanner.take() {
        s.cancel();
    }
    let options = OpenOptions {
        view_only: true,
        ..OpenOptions::default()
    };
    let album = Album::open(&fx.root, &options).unwrap();
    let e = fx.tagging.regenerate(&album, id).unwrap_err();
    assert_eq!(e.code, ErrorCode::ViewOnly);
    assert!(!fx.tagging.photo_tags(&album, id).unwrap().can_regenerate);
}

#[test]
fn a_stuck_run_writes_the_thread_off_and_a_new_one_carries_on() {
    let mut fx = Fx::with_limits(
        &["hang-a.jpg", "zebra.jpg"],
        Limits {
            progress_event: Duration::ZERO,
            describe: Duration::from_millis(50),
            stuck: Duration::from_millis(100),
            ..Limits::default()
        },
    );
    fx.control.set_hang(Duration::from_millis(1500));
    fx.choose(Some(D), None);
    fx.open();
    let s = fx.finished();
    assert_eq!((s.total, s.done, s.failed), (2, 1, 1));
    assert!(s.not_working.is_empty(), "one loss does not stop the part");
    assert_eq!(
        fx.row("hang-a.jpg", D).unwrap().error.as_deref(),
        Some("Tagging this photo took too long.")
    );
    assert_eq!(
        fx.factory_calls.load(Ordering::Relaxed),
        2,
        "a new thread with empty slots"
    );
    // The stuck thread returns later and writes nothing.
    let rows = fx.rows();
    std::thread::sleep(Duration::from_millis(1600));
    assert_eq!(fx.rows(), rows);
    assert_eq!(
        fx.row("hang-a.jpg", D).unwrap().error.as_deref(),
        Some("Tagging this photo took too long.")
    );
}

#[test]
fn shutdown_does_not_wait_forever_for_a_stuck_run() {
    let mut fx = Fx::with_limits(
        &["hang-a.jpg"],
        Limits {
            describe: Duration::from_secs(60),
            stuck: Duration::from_millis(100),
            ..Limits::default()
        },
    );
    fx.control.set_hang(Duration::from_millis(2000));
    fx.choose(Some(D), None);
    fx.open();
    fx.until("the run", |_| !fx.control.runs().is_empty());
    let start = Instant::now();
    fx.tagging.shutdown();
    assert!(start.elapsed() < Duration::from_millis(1500));
    // Let the left-behind thread finish before the folder goes.
    std::thread::sleep(Duration::from_millis(2100));
}
