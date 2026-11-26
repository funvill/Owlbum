//! Opening a new album (#106, docs/03-media-and-scanning.md, "Speed
//! targets"): how soon the first rows and the first screen of thumbnails
//! are there, and how soon a screen the scan has not reached comes after
//! the page asks for it. It plays the page's part without a webview: it
//! reads the first screen of rows at the first `album_changed`, asks for
//! their small thumbnails all at once through the `owlbum://` handler, and
//! later asks for a screen near the end of the walk. Timings are printed,
//! not asserted. Run it on a release build against a `large-N` album that
//! has never been opened:
//!
//! ```text
//! owlbum-fixtures large 2000 <dir> --megapixels 12-24
//! OWLBUM_FIRST_OPEN=<dir> cargo test --release -p owlbum-core --test first_open -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use owlbum_core::album::{Album, OpenOptions};
use owlbum_core::appdata::AppData;
use owlbum_core::events::Emitter;
use owlbum_core::scan::{Request, ScanEnv, Scanner};
use owlbum_core::thumbs::Thumbs;
use owlbum_core::thumbs::serve;

/// Tiles on the first screen: a 1920 × 1080 window at the default size.
const SCREEN: usize = 60;

/// Remembers when the first `album_changed` was sent.
#[derive(Default)]
struct FirstChange(Mutex<Option<Instant>>);

impl Emitter for FirstChange {
    fn emit_json(&self, name: &'static str, _: serde_json::Value) {
        if name == "album_changed" {
            self.0.lock().unwrap().get_or_insert_with(Instant::now);
        }
    }
}

/// Asks for every key's small thumbnail at once, as the page's tiles do,
/// and answers how long the slowest and the middle one took.
fn ask(scanner: &Scanner, thumbs: &Thumbs, keys: &[String]) -> (Duration, Duration) {
    let start = Instant::now();
    let mut took: Vec<Duration> = std::thread::scope(|s| {
        let asks: Vec<_> = keys
            .iter()
            .map(|key| {
                s.spawn(move || {
                    let reply = serve::answer(
                        Some(scanner.open()),
                        thumbs,
                        &format!("owlbum://thumb/{key}/s"),
                    );
                    assert_eq!(reply.status, 200, "{key}");
                    start.elapsed()
                })
            })
            .collect();
        asks.into_iter().map(|a| a.join().unwrap()).collect()
    });
    took.sort();
    (took[took.len() - 1], took[took.len() / 2])
}

fn keys(scanner: &Scanner, sql: &str) -> Vec<String> {
    scanner
        .album()
        .db()
        .read(|c| {
            let mut st = c.prepare(sql).unwrap();
            Ok(st
                .query_map([], |r| r.get(0))
                .unwrap()
                .collect::<Result<Vec<String>, _>>()
                .unwrap())
        })
        .unwrap()
}

#[test]
#[ignore = "needs a large album: set OWLBUM_FIRST_OPEN"]
fn first_open_of_a_new_album() {
    let Ok(root) = std::env::var("OWLBUM_FIRST_OPEN") else {
        eprintln!("OWLBUM_FIRST_OPEN is not set");
        return;
    };
    let root = PathBuf::from(root);
    let data_dir = tempfile::tempdir().unwrap();
    let thumbs = Thumbs::open(AppData::open(data_dir.path()).unwrap()).unwrap();
    let changes = Arc::new(FirstChange::default());

    let opened = Instant::now();
    let album = Album::open(&root, &OpenOptions::default()).unwrap();
    let mut env = ScanEnv::new(changes.clone());
    env.thumbs = Some(thumbs.clone());
    let threads = env.threads;
    let scanner = Scanner::new(Arc::new(album), env);
    scanner.request(Request::default());

    // The page re-runs its query at the first album_changed.
    let first = loop {
        if let Some(t) = *changes.0.lock().unwrap() {
            break t;
        }
        std::thread::sleep(Duration::from_millis(1));
    };
    let screen = keys(
        &scanner,
        &format!(
            "SELECT thumb_key FROM photos WHERE scan_error IS NULL
             ORDER BY captured_at DESC LIMIT {SCREEN}"
        ),
    );
    let rows_at = opened.elapsed();
    let rows_then = keys(&scanner, "SELECT thumb_key FROM photos").len();
    let (slowest, middle) = ask(&scanner, &thumbs, &screen);
    let visible_at = first.duration_since(opened) + slowest;

    // A screen the scan has not reached: the last rows of the walk.
    let far = keys(
        &scanner,
        &format!(
            "SELECT thumb_key FROM photos WHERE analysis IS NULL AND scan_error IS NULL
             ORDER BY id DESC LIMIT {SCREEN}"
        ),
    );
    let far_asked = opened.elapsed();
    let (far_slowest, far_middle) = if far.is_empty() {
        (Duration::ZERO, Duration::ZERO)
    } else {
        ask(&scanner, &thumbs, &far)
    };

    scanner.wait();
    let total = opened.elapsed();
    let summary = scanner.last().unwrap();
    println!("scan threads: {threads}; photos found: {}", summary.found);
    println!(
        "first album_changed {:?} after opening, {rows_then} rows then; first screen read at {rows_at:?}",
        first.duration_since(opened)
    );
    println!(
        "first screen of {} thumbnails: all by {visible_at:?} after opening (slowest {slowest:?}, middle {middle:?} after asking)",
        screen.len()
    );
    println!(
        "a screen of {} the scan had not reached, asked at {far_asked:?}: slowest {far_slowest:?}, middle {far_middle:?}",
        far.len()
    );
    println!("scan finished {total:?} after opening");
}
