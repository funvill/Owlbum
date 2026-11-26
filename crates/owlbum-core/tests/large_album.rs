//! The large-album check (#26; docs/03-media-and-scanning.md, "Speed
//! targets", and docs/05-organising.md, "Performance targets"): a new
//! album is opened and scanned, then closed and opened again with its
//! thumbnails kept, as a person reopening it would. The reopen is timed to
//! the first screen of tiles and to the end of its walk and compare, and
//! the query commands the grid uses to open, scroll and step are timed on
//! the whole result.
//!
//! Timings are printed, not asserted: they depend on the machine. Run the
//! full size once on a release build, with the album and the thumbnail
//! folder at paths you delete afterwards:
//!
//! ```text
//! owlbum-fixtures large 100000 <album>
//! OWLBUM_LARGE=<album> OWLBUM_LARGE_DATA=<empty folder> \
//!   cargo test --release -p owlbum-core --test large_album -- --ignored --nocapture
//! ```

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use owlbum_core::album::{Album, OpenOptions};
use owlbum_core::appdata::AppData;
use owlbum_core::events::Emitter;
use owlbum_core::query::{Keep, Located, PhotoAt, Query, QueryEngine, QuerySort};
use owlbum_core::scan::{Request, ScanEnv, Scanner};
use owlbum_core::thumbs::Thumbs;
use owlbum_core::thumbs::serve;

/// Tiles on the first screen: a 1920 × 1080 window at the default size.
const SCREEN: u32 = 60;

/// One block of the grid (docs/04-browsing.md, "Fetching rows").
const BLOCK: u32 = 200;

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

fn ms(d: Duration) -> String {
    format!("{:.1} ms", d.as_secs_f64() * 1000.0)
}

fn scanner(root: &Path, thumbs: &Thumbs, changes: Arc<FirstChange>) -> Scanner {
    let album = Album::open(root, &OpenOptions::default()).unwrap();
    let mut env = ScanEnv::new(changes);
    env.thumbs = Some(thumbs.clone());
    Scanner::new(Arc::new(album), env)
}

/// Serves the small thumbnail of every key, one after another, as the
/// tiles of a screen ask for them.
fn serve_small(scanner: &Scanner, thumbs: &Thumbs, keys: &[String]) {
    for key in keys {
        let reply = serve::answer(
            Some(scanner.open()),
            thumbs,
            &format!("owlbum://thumb/{key}/s"),
        );
        assert_eq!(reply.status, 200, "{key}");
    }
}

/// Opens the album, runs the default query, reads the first block and
/// serves the first screen's small thumbnails: what a person reopening it
/// waits for. Answers the scanner, the time `Album::open` took and the time
/// to the first screen of tiles.
fn reopen(root: &Path, thumbs: &Thumbs) -> (Scanner, Duration, Duration) {
    let opened = Instant::now();
    let reopened = scanner(root, thumbs, Arc::new(FirstChange::default()));
    let open_took = opened.elapsed();
    let engine = QueryEngine::new();
    let album = Arc::clone(reopened.album());
    let result = engine.run(&album, &Query::default(), None).unwrap();
    let window = engine
        .window(&album, &reopened.overlay(), &result.token, 0, BLOCK)
        .unwrap();
    let keys: Vec<String> = window
        .iter()
        .take(SCREEN as usize)
        .map(|r| r.thumb_key.clone())
        .collect();
    serve_small(&reopened, thumbs, &keys);
    (reopened, open_took, opened.elapsed())
}

fn check(root: &Path, data: &Path, expect: Option<usize>) {
    let thumbs = Thumbs::open(AppData::open(data).unwrap()).unwrap();

    // First open of a new album: rows and the whole scan.
    let changes = Arc::new(FirstChange::default());
    let opened = Instant::now();
    let first = scanner(root, &thumbs, changes.clone());
    let summary = first.scan_and_wait(Request::default());
    let scanned = opened.elapsed();
    let first_change = changes.0.lock().unwrap().map(|t| t.duration_since(opened));
    assert!(summary.error.is_none(), "{:?}", summary.error);
    let found = summary.found as usize;
    if let Some(n) = expect {
        assert_eq!(found, n);
    }
    drop(first);

    // Reopen with the thumbnails kept. The first reopen of the day also
    // makes the day's backup, after the open answers (#124).
    let (reopened, open_took, first_tiles) = reopen(root, &thumbs);
    let engine = QueryEngine::new();
    let album = Arc::clone(reopened.album());

    // The walk and compare of the reopen, which finds nothing to do.
    let walked = Instant::now();
    let again = reopened.scan_and_wait(Request::default());
    let walk_took = walked.elapsed();
    assert!(again.error.is_none(), "{:?}", again.error);
    assert_eq!(
        (again.added, again.changed, again.missing, again.moved),
        (0, 0, 0, 0),
        "a reopen with nothing changed"
    );

    // The query commands, each once, on the whole result.
    let date = Query::default();
    let name = Query {
        sort: QuerySort::Name,
        ..Query::default()
    };
    let t = Instant::now();
    let by_date = engine.run(&album, &date, None).unwrap();
    let run_date = t.elapsed();
    let t = Instant::now();
    let by_name = engine.run(&album, &name, None).unwrap();
    let run_name = t.elapsed();
    let count = by_date.count;
    assert_eq!(count, by_name.count);
    let mut other_sorts = Vec::new();
    for sort in [QuerySort::Score, QuerySort::Interesting, QuerySort::Colour] {
        let q = Query {
            sort,
            ..Query::default()
        };
        let t = Instant::now();
        let r = engine.run(&album, &q, None).unwrap();
        other_sorts.push((sort, t.elapsed()));
        assert_eq!(r.count, count);
    }
    // The engine holds only a few results, and the runs above pushed the
    // date-sorted one out: run it again for the re-run below.
    let by_date = engine.run(&album, &date, None).unwrap();

    // A re-run that keeps the place: a current photo near the end and a
    // selection of a thousand, as after an `album_changed`.
    let ids = engine.ids(&by_date.token, 0, count).unwrap();
    let last = *ids.last().unwrap();
    let keep = Keep {
        token: by_date.token.clone(),
        current: Some(PhotoAt {
            id: last,
            index: count - 1,
        }),
        anchor_id: None,
        selection: ids.iter().step_by(ids.len() / 1000 + 1).copied().collect(),
        view: None,
    };
    let t = Instant::now();
    let kept = engine.run(&album, &date, Some(&keep)).unwrap();
    let run_keep = t.elapsed();
    assert_eq!(kept.kept.unwrap().current.unwrap().id, last);
    let token = kept.token;

    let mut windows = Vec::new();
    for offset in [0, count / 2, count - BLOCK.min(count)] {
        let t = Instant::now();
        let rows = engine
            .window(&album, &reopened.overlay(), &token, offset, BLOCK)
            .unwrap();
        windows.push((offset, t.elapsed()));
        assert_eq!(rows.len(), BLOCK.min(count - offset) as usize);
    }

    let t = Instant::now();
    let at = engine.locate(&token, last).unwrap();
    let locate_last = t.elapsed();
    assert_eq!(at, Some(Located::At { index: count - 1 }));
    let t = Instant::now();
    let all_ids = engine.ids(&token, 0, count).unwrap();
    let ids_all = t.elapsed();
    assert_eq!(all_ids.len(), count as usize);

    // One star, as the star command will write it (#27): one row through
    // the album's writer, the index following before it answers.
    let t = Instant::now();
    album
        .write_photos(move |tx, touched| {
            tx.execute("UPDATE photos SET star = 1 - star WHERE id = ?1", [last])
                .map_err(|e| {
                    owlbum_core::error::AppError::internal("star")
                        .with_details(serde_json::json!({ "cause": e.to_string() }))
                })?;
            touched.photo(last);
            Ok(())
        })
        .unwrap();
    let star = t.elapsed();

    // A later reopen the same day: no backup, the file cache warm.
    drop(album);
    drop(reopened);
    let (again, later_open, later_tiles) = reopen(root, &thumbs);
    drop(again);

    println!("large album, {found} photos");
    println!(
        "  first open: first album_changed {:?}, scan done {scanned:?}",
        first_change
    );
    println!("  reopen: Album::open with index load {}", ms(open_took));
    println!(
        "  reopen: first {SCREEN} tiles served (target 1 s) {}",
        ms(first_tiles)
    );
    println!(
        "  later reopen: Album::open with index load {}",
        ms(later_open)
    );
    println!(
        "  later reopen: first {SCREEN} tiles served (target 1 s) {}",
        ms(later_tiles)
    );
    println!("  reopen: walk and compare (target 15 s) {walk_took:?}");
    println!("  query_run, date (target 30 ms) {}", ms(run_date));
    println!("  query_run, name (target 30 ms) {}", ms(run_name));
    for (sort, took) in other_sorts {
        println!("  query_run, {sort:?} (target 30 ms) {}", ms(took));
    }
    println!(
        "  query_run, date, keeping place and 1,000 selected {}",
        ms(run_keep)
    );
    for (offset, took) in windows {
        println!(
            "  query_window of {BLOCK} at {offset} (target 10 ms) {}",
            ms(took)
        );
    }
    println!(
        "  query_locate of the last photo (target 5 ms) {}",
        ms(locate_last)
    );
    println!("  query_ids of the whole result {}", ms(ids_all));
    println!(
        "  star one photo through write_photos (target 50 ms) {}",
        ms(star)
    );
}

#[test]
fn a_small_large_album() {
    let album = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let root = album.path().join("large");
    owlbum_fixtures::large(&root, 300, 7).unwrap();
    check(&root, data.path(), Some(300));
}

#[test]
#[ignore = "needs a large album: set OWLBUM_LARGE and OWLBUM_LARGE_DATA"]
fn large_album() {
    let (Ok(root), Ok(data)) = (
        std::env::var("OWLBUM_LARGE"),
        std::env::var("OWLBUM_LARGE_DATA"),
    ) else {
        eprintln!("OWLBUM_LARGE and OWLBUM_LARGE_DATA are not set");
        return;
    };
    check(Path::new(&root), Path::new(&data), None);
}
