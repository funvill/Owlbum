//! The folder watcher with a real temporary folder
//! (docs/03-media-and-scanning.md, "Tests": watcher).

use super::*;
use crate::album::ChangeReason;
use crate::scan::watch::intake::Timing;
use crate::scan::watch::{AlbumWatch, WatchEnv, WatchState};

/// Short waits, so a test takes a second, not ten.
fn quick() -> WatchEnv {
    WatchEnv {
        timing: Timing {
            quiet: Duration::from_millis(300),
            most: Duration::from_secs(2),
            max_folders: 500,
        },
        focus_after: Duration::from_secs(3600),
        check_every: Duration::from_millis(200),
        ..WatchEnv::default()
    }
}

/// Waits up to 30 s for `done`.
fn eventually(what: &str, mut done: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(30);
    while Instant::now() < until {
        if done() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for {what}");
}

/// An album of `rels`, scanned, with a scanner and a watch on it.
struct Watched {
    scanner: Arc<Scanner>,
    watch: Option<AlbumWatch>,
    db: Arc<Album>,
    /// Last, so the album's folder is deleted after the album closes.
    fx: Fx,
}

impl Watched {
    fn new(rels: &[&str], env: WatchEnv) -> Self {
        Self::scoped(rels, None, env)
    }

    fn scoped(rels: &[&str], scope: Option<&str>, env: WatchEnv) -> Self {
        let mut fx = Fx::empty();
        for (i, rel) in rels.iter().enumerate() {
            picture(&fx.path(rel), u8::try_from(i).unwrap());
        }
        if let Some(scope) = scope {
            let folder = fx.path(scope);
            fx.reopen(&folder, &OpenOptions::default());
        }
        let album = fx.take_album();
        let scanner = Arc::new(Scanner::new(Arc::clone(&album), fx.env.clone()));
        scanner.scan_and_wait(Request::default());
        let watch = AlbumWatch::start(Arc::clone(&scanner), env);
        let w = Self {
            fx,
            scanner,
            watch: Some(watch),
            db: album,
        };
        eventually("the watch to start", || {
            w.watch().state() != WatchState::Starting
        });
        w
    }

    fn watch(&self) -> &AlbumWatch {
        self.watch.as_ref().unwrap()
    }

    fn row(&self, rel: &str) -> Option<(i64, bool, i64)> {
        self.db
            .db()
            .read(|c| {
                Ok(c.query_row(
                    "SELECT id, missing, size FROM photos WHERE rel_path = ?1",
                    [rel],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .ok())
            })
            .unwrap()
    }

    /// Waits for the scans the watch asked for to settle.
    fn settle(&self) {
        std::thread::sleep(Duration::from_millis(800));
        self.scanner.wait();
    }

    fn scans_finished(&self) -> usize {
        self.fx
            .events
            .progress()
            .iter()
            .filter(|(p, _)| p.finished)
            .count()
    }
}

impl Drop for Watched {
    fn drop(&mut self) {
        // The watch first, then the scanner and the album.
        self.watch = None;
    }
}

#[test]
fn files_added_changed_removed_and_renamed_change_their_rows() {
    let w = Watched::new(&["a/1.jpg", "a/2.jpg", "b/3.jpg"], quick());
    assert_eq!(w.watch().state(), WatchState::Watching);
    assert!(w.scanner.status().watching);
    w.fx.events.clear();

    // Added.
    picture(&w.fx.path("a/new.jpg"), 50);
    eventually("the new file's row", || w.row("a/new.jpg").is_some());
    let changes = w.fx.events.changes();
    assert!(
        changes.iter().all(|(c, _)| c.reason == ChangeReason::Watch),
        "{changes:?}"
    );

    // Changed: bigger, so its size tells.
    let (id, _, size) = w.row("a/1.jpg").unwrap();
    let img = RgbImage::from_fn(120, 90, |x, y| Rgb([x as u8, y as u8, 7]));
    img.save(w.fx.path("a/1.jpg")).unwrap();
    eventually("the changed file's row", || {
        w.row("a/1.jpg")
            .is_some_and(|(i, _, s)| i == id && s != size)
    });

    // Removed.
    std::fs::remove_file(w.fx.path("a/2.jpg")).unwrap();
    eventually("the removed file's row to go missing", || {
        w.row("a/2.jpg").is_some_and(|(_, missing, _)| missing)
    });

    // Renamed across folders.
    let (id, ..) = w.row("b/3.jpg").unwrap();
    rename(&w.fx, "b/3.jpg", "a/3-renamed.jpg");
    eventually("the renamed file to keep its row", || {
        w.row("a/3-renamed.jpg")
            .is_some_and(|(i, missing, _)| i == id && !missing)
    });
    assert!(w.row("b/3.jpg").is_none());
}

#[test]
fn a_new_folder_moved_into_place_is_scanned_whole() {
    let w = Watched::new(&["a/1.jpg"], quick());
    // Made outside the album, then moved in.
    let outside = w.fx._tmp.path().join("outside");
    picture(&outside.join("x.jpg"), 1);
    picture(&outside.join("deep/y.jpg"), 2);
    std::fs::rename(&outside, w.fx.path("a/came")).unwrap();
    eventually("both files of the moved-in folder", || {
        w.row("a/came/x.jpg").is_some() && w.row("a/came/deep/y.jpg").is_some()
    });
}

#[test]
fn a_folder_removed_marks_its_rows_missing() {
    let w = Watched::new(&["a/sub/1.jpg", "a/sub/deeper/2.jpg", "b/3.jpg"], quick());
    std::fs::remove_dir_all(w.fx.path("a/sub")).unwrap();
    eventually("the folder's rows to go missing", || {
        w.row("a/sub/1.jpg").is_some_and(|r| r.1)
            && w.row("a/sub/deeper/2.jpg").is_some_and(|r| r.1)
    });
    assert!(!w.row("b/3.jpg").unwrap().1);
}

#[test]
fn changes_owlbum_makes_in_its_own_folder_and_dot_folders_start_no_scan() {
    let w = Watched::new(&["a/1.jpg"], quick());
    w.settle();
    let before = w.scans_finished();
    // The album's own database is written (a typed change) and a picture
    // lands in a dot folder.
    w.db.db()
        .write(|tx| {
            tx.execute("UPDATE photos SET star = 1", [])
                .map_err(sqlite_error(text::album::WRITING_DATABASE))
        })
        .unwrap();
    picture(&w.fx.path(".hidden/x.jpg"), 3);
    picture(&w.fx.path("$RECYCLE.BIN/y.jpg"), 4);
    std::fs::write(w.fx.path("a/notes.txt"), b"not a picture").unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    w.scanner.wait();
    assert_eq!(w.scans_finished(), before, "no scan was asked for");
}

#[test]
fn a_scan_the_watch_asked_for_does_not_start_another() {
    let w = Watched::new(&["a/1.jpg"], quick());
    picture(&w.fx.path("a/2.jpg"), 9);
    eventually("the new row", || w.row("a/2.jpg").is_some());
    w.settle();
    let after_first = w.scans_finished();
    // The scan read the file and wrote the database: neither is a change.
    std::thread::sleep(Duration::from_millis(1500));
    w.scanner.wait();
    assert_eq!(w.scans_finished(), after_first);
}

fn fails_to_start(
    _: &Path,
    _: Box<dyn FnMut(notify::Result<notify::Event>) + Send>,
) -> notify::Result<Box<dyn std::any::Any + Send>> {
    Err(notify::Error::new(notify::ErrorKind::MaxFilesWatch))
}

#[test]
fn a_watch_that_will_not_start_falls_back_to_rescanning_on_focus() {
    let env = WatchEnv {
        start: fails_to_start,
        focus_after: Duration::ZERO,
        ..quick()
    };
    let w = Watched::new(&["a/1.jpg"], env);
    assert_eq!(w.watch().state(), WatchState::Failed);
    assert!(!w.scanner.status().watching);

    picture(&w.fx.path("a/2.jpg"), 2);
    w.settle();
    assert!(w.row("a/2.jpg").is_none(), "nothing watches");
    w.watch().focused();
    w.scanner.wait();
    assert!(w.row("a/2.jpg").is_some(), "the focus rescan found it");
    // Not tried again: the limit has not gone away.
    assert_eq!(w.watch().state(), WatchState::Failed);
}

fn errs_after_starting(
    _: &Path,
    mut handler: Box<dyn FnMut(notify::Result<notify::Event>) + Send>,
) -> notify::Result<Box<dyn std::any::Any + Send>> {
    // As inotify does when the limit is reached while adding the watches
    // of a recursive tree, or for a folder made later.
    handler(Err(notify::Error::new(notify::ErrorKind::MaxFilesWatch)));
    Ok(Box::new(()))
}

#[test]
fn a_watch_that_reports_an_error_later_is_dropped_and_not_tried_again() {
    let env = WatchEnv {
        start: errs_after_starting,
        ..quick()
    };
    let w = Watched::new(&["a/1.jpg"], env);
    eventually("the watch to be dropped", || {
        w.watch().state() == WatchState::Failed
    });
    assert!(!w.scanner.status().watching);
    w.settle();
    let before = w.scans_finished();
    // Focus within the 5 minutes: no new watch, no scan.
    w.watch().focused();
    w.watch().focused();
    w.scanner.wait();
    assert_eq!(w.watch().state(), WatchState::Failed);
    assert_eq!(w.scans_finished(), before);
}

#[test]
fn focus_rescans_only_when_the_last_scan_ended_long_enough_ago() {
    let env = WatchEnv {
        start: fails_to_start,
        ..quick()
    };
    let w = Watched::new(&["a/1.jpg"], env);
    picture(&w.fx.path("a/2.jpg"), 2);
    let before = w.scans_finished();
    w.watch().focused();
    w.scanner.wait();
    assert_eq!(
        w.scans_finished(),
        before,
        "the last scan ended moments ago"
    );
    assert!(w.row("a/2.jpg").is_none());
}

#[test]
fn focus_does_nothing_while_the_folder_is_watched() {
    let env = WatchEnv {
        focus_after: Duration::ZERO,
        ..quick()
    };
    let w = Watched::new(&["a/1.jpg"], env);
    assert_eq!(w.watch().state(), WatchState::Watching);
    w.settle();
    let before = w.scans_finished();
    w.watch().focused();
    w.scanner.wait();
    assert_eq!(w.scans_finished(), before);
}

#[test]
fn a_watched_folder_that_disappears_loses_no_rows_and_is_watched_again_when_back() {
    // A scoped open, so the watched folder can go away while the album
    // database (in the root) stays open.
    let w = Watched::scoped(&["s/a/1.jpg", "s/b/2.jpg"], Some("s"), quick());
    assert_eq!(w.watch().state(), WatchState::Watching);
    let away = w.fx._tmp.path().join("away");
    std::fs::rename(w.fx.path("s"), &away).unwrap();
    eventually("the watch to be dropped", || {
        w.watch().state() == WatchState::Lost
    });
    w.settle();
    assert!(!w.scanner.status().watching);
    assert!(!w.row("s/a/1.jpg").unwrap().1, "not missing");
    assert!(!w.row("s/b/2.jpg").unwrap().1, "not missing");

    // Still gone: focus leaves it lost.
    w.watch().focused();
    assert_eq!(w.watch().state(), WatchState::Lost);

    // Back, with a file added while it was away.
    picture(&away.join("a/3.jpg"), 3);
    std::fs::rename(&away, w.fx.path("s")).unwrap();
    w.watch().focused();
    eventually("the watch to start again", || {
        w.watch().state() == WatchState::Watching
    });
    eventually("the file added while away", || w.row("s/a/3.jpg").is_some());
    assert!(!w.row("s/a/1.jpg").unwrap().1);

    // And it watches again.
    picture(&w.fx.path("s/b/4.jpg"), 4);
    eventually("a file added after", || w.row("s/b/4.jpg").is_some());
}

/// The real inotify limit. Run inside a user namespace with a low limit,
/// which leaves the machine's own limit alone:
///
/// ```text
/// unshare -Ur sh -c 'echo 40 > /proc/sys/user/max_inotify_watches &&
///   cargo test -p owlbum-core --lib inotify_limit -- --ignored --nocapture'
/// ```
#[cfg(target_os = "linux")]
#[test]
#[ignore = "needs a low inotify limit; see the comment"]
fn inotify_limit_falls_back_without_losing_rows() {
    // Reached while adding the watches of the tree.
    let many: Vec<String> = (0..100).map(|i| format!("f{i}/x.jpg")).collect();
    let rels: Vec<&str> = many.iter().map(String::as_str).collect();
    let w = Watched::new(&rels, quick());
    w.settle();
    let state = w.watch().state();
    eprintln!("limit reached at start: {state:?}");
    assert_eq!(state, WatchState::Failed);
    assert!(!w.scanner.status().watching);
    assert!(!w.row("f0/x.jpg").unwrap().1);
    drop(w);

    // Reached later, by folders made after the watch started.
    let w = Watched::new(&["a/1.jpg"], quick());
    assert_eq!(w.watch().state(), WatchState::Watching);
    for i in 0..100 {
        std::fs::create_dir_all(w.fx.path(&format!("new{i}"))).unwrap();
    }
    picture(&w.fx.path("new0/y.jpg"), 1);
    eventually("the watch to be dropped", || {
        w.watch().state() != WatchState::Watching
    });
    w.settle();
    eprintln!("limit reached later: {:?}", w.watch().state());
    assert_eq!(w.watch().state(), WatchState::Failed);
    assert!(!w.row("a/1.jpg").unwrap().1);
}
