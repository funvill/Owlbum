//! Targeted rescans: the scanner walking only the folders the watcher saw
//! change (docs/03-media-and-scanning.md, "Debounce").

use super::*;
use crate::album::ChangeReason;

fn folders(items: &[(&str, bool)]) -> Folders {
    let mut f = Folders::new();
    for (d, r) in items {
        f.add(d, *r);
    }
    f
}

impl Fx {
    /// The watcher's rescan of `items`.
    fn watch_scan(&self, items: &[(&str, bool)]) -> ScanSummary {
        self.cancel.store(false, Ordering::SeqCst);
        self.run(Request::watch(Some(folders(items))))
    }
}

/// An album of `rels`, each a small picture, scanned once.
fn album_of(rels: &[&str]) -> Fx {
    let fx = Fx::empty();
    for (i, rel) in rels.iter().enumerate() {
        picture(&fx.path(rel), u8::try_from(i).unwrap());
    }
    let first = fx.scan();
    assert!(first.error.is_none(), "{:?}", first.error);
    fx
}

#[test]
fn a_targeted_scan_touches_no_row_outside_its_folders() {
    let fx = album_of(&["a/1.jpg", "b/2.jpg", "c/3.jpg"]);
    std::fs::remove_file(fx.path("b/2.jpg")).unwrap();
    touch_later(&fx.path("c/3.jpg"));
    picture(&fx.path("a/4.jpg"), 40);
    fx.reads.take();

    let summary = fx.watch_scan(&[("a", false)]);
    assert!(summary.error.is_none(), "{:?}", summary.error);
    assert_eq!((summary.added, summary.missing), (1, 0));
    assert!(fx.photo("a/4.jpg").is_some());
    assert!(!fx.photo("b/2.jpg").unwrap().missing, "b was not looked at");
    // c changed but was not in the set: not read.
    assert_eq!(fx.read("metadata"), ["a/4.jpg"]);
}

#[test]
fn a_folder_that_is_not_recursive_leaves_its_subfolders_alone() {
    let fx = album_of(&["a/1.jpg", "a/sub/2.jpg"]);
    std::fs::remove_file(fx.path("a/sub/2.jpg")).unwrap();
    fx.watch_scan(&[("a", false)]);
    assert!(!fx.photo("a/sub/2.jpg").unwrap().missing);
    fx.watch_scan(&[("a", true)]);
    assert!(fx.photo("a/sub/2.jpg").unwrap().missing);
    assert!(!fx.photo("a/1.jpg").unwrap().missing);
}

#[test]
fn a_file_moved_between_two_folders_of_the_set_keeps_its_row() {
    let fx = album_of(&["a/1.jpg", "b/2.jpg"]);
    let before = fx.photo("a/1.jpg").unwrap();
    fx.star(before.id);
    rename(&fx, "a/1.jpg", "b/1.jpg");
    let summary = fx.watch_scan(&[("a", false), ("b", false)]);
    assert_eq!(summary.moved, 1);
    let after = fx.photo("b/1.jpg").unwrap();
    assert_eq!((after.id, after.star, after.missing), (before.id, 1, false));
    assert!(fx.photo("a/1.jpg").is_none());
}

#[test]
fn a_delete_and_the_create_seen_by_two_targeted_scans_keep_the_row() {
    let fx = album_of(&["a/1.jpg", "b/2.jpg"]);
    let before = fx.photo("a/1.jpg").unwrap();
    let parked = fx._tmp.path().join("parked.jpg");
    std::fs::rename(fx.path("a/1.jpg"), &parked).unwrap();
    fx.watch_scan(&[("a", false)]);
    assert!(fx.by_id(before.id).missing);

    // It comes back in another folder, which is all the next scan sees.
    std::fs::create_dir_all(fx.path("c")).unwrap();
    std::fs::rename(&parked, fx.path("c/1.jpg")).unwrap();
    let summary = fx.watch_scan(&[("c", true)]);
    assert_eq!((summary.moved, summary.added), (1, 0));
    let after = fx.by_id(before.id);
    assert_eq!((after.rel_path.as_str(), after.missing), ("c/1.jpg", false));
    assert!(!fx.photo("b/2.jpg").unwrap().missing);
}

#[test]
fn a_folder_that_is_gone_loses_its_rows_but_an_unplugged_scope_loses_none() {
    let fx = album_of(&["a/1.jpg", "a/sub/2.jpg", "b/3.jpg"]);
    std::fs::remove_dir_all(fx.path("a")).unwrap();
    // The watcher asks for the parent of a removed folder, recursive.
    let summary = fx.watch_scan(&[("", true)]);
    assert!(summary.error.is_none());
    assert!(fx.photo("a/1.jpg").unwrap().missing);
    assert!(fx.photo("a/sub/2.jpg").unwrap().missing);
    assert!(!fx.photo("b/3.jpg").unwrap().missing);
}

#[test]
fn nothing_is_marked_missing_when_a_targeted_scope_cannot_be_read() {
    let mut fx = album_of(&["s/a/1.jpg", "s/b/2.jpg"]);
    let scoped = fx.path("s");
    fx.reopen(&scoped, &OpenOptions::default());
    // The scope folder goes away (as an unplugged drive would).
    std::fs::rename(&scoped, fx.path("t")).unwrap();
    let summary = fx.watch_scan(&[("s/a", false)]);
    assert_eq!(summary.error.map(|e| e.code), Some(ErrorCode::NotAFolder));
    assert!(fx.photos().iter().all(|p| !p.missing), "{:#?}", fx.photos());
}

#[test]
fn a_watch_scan_says_watch_and_any_other_says_scan() {
    let fx = album_of(&["a/1.jpg"]);
    fx.events.clear();
    picture(&fx.path("a/2.jpg"), 9);
    fx.watch_scan(&[("a", false)]);
    let reasons: Vec<ChangeReason> = fx.events.changes().iter().map(|(c, _)| c.reason).collect();
    assert!(!reasons.is_empty());
    assert!(
        reasons.iter().all(|r| *r == ChangeReason::Watch),
        "{reasons:?}"
    );

    fx.events.clear();
    picture(&fx.path("a/3.jpg"), 10);
    fx.scan();
    let reasons: Vec<ChangeReason> = fx.events.changes().iter().map(|(c, _)| c.reason).collect();
    assert!(!reasons.is_empty());
    assert!(
        reasons.iter().all(|r| *r == ChangeReason::Scan),
        "{reasons:?}"
    );
}

#[test]
fn a_targeted_scan_keeps_the_report_of_the_last_whole_walk() {
    let mut fx = Fx::empty();
    picture(&fx.path("a/1.jpg"), 1);
    std::fs::write(fx.path("a/x.heic"), b"heic").unwrap();
    std::fs::create_dir_all(fx.path("b")).unwrap();
    let scanner = Scanner::new(fx.take_album(), fx.env.clone());
    scanner.scan_and_wait(Request::default());
    assert_eq!(scanner.status().unsupported, 1);

    picture(&fx.path("b/2.jpg"), 2);
    let summary = scanner.scan_and_wait(Request::watch(Some(folders(&[("b", false)]))));
    assert_eq!(summary.added, 1);
    assert_eq!(
        scanner.status().unsupported,
        1,
        "the report is still the whole walk's"
    );
    assert_eq!(scanner.report().unsupported.len(), 1);
}

#[test]
fn a_targeted_folder_inside_a_nested_album_is_not_scanned() {
    let fx = album_of(&["a/1.jpg"]);
    picture(&fx.path("n/2.jpg"), 2);
    picture(&fx.path("n/inner/3.jpg"), 3);
    std::fs::create_dir_all(fx.path("n/.owlbum")).unwrap();
    std::fs::write(fx.path("n/.owlbum/owlbum.sqlite"), b"").unwrap();
    let summary = fx.watch_scan(&[("n", false), ("n/inner", true)]);
    assert_eq!(summary.added, 0);
    assert_eq!(summary.walk.nested, ["n"]);
}

// ---- Requests ----

#[test]
fn requests_merge_their_folders_or_take_the_whole_scope() {
    let a = Request::watch(Some(folders(&[("a", false)])));
    let b = Request::watch(Some(folders(&[("b", true)])));
    let both = a.clone().merge(b);
    assert!(both.watch);
    assert_eq!(
        both.folders.unwrap().normalised(),
        [("a".to_owned(), false), ("b".to_owned(), true)]
    );

    // A whole-scope request takes in a targeted one; the scan is a watch
    // scan only when both asked as the watcher.
    let whole = a.clone().merge(Request::default());
    assert_eq!((whole.folders, whole.watch), (None, false));
    let whole = Request::watch(None).merge(a.clone());
    assert_eq!((whole.folders, whole.watch), (None, true));
    assert!(a.clone().merge(Request::manual()).manual);

    // Over 500 folders together: the whole scope.
    let mut many = Folders::new();
    for i in 0..400 {
        many.add(&format!("x{i}"), false);
    }
    let mut more = Folders::new();
    for i in 0..101 {
        more.add(&format!("y{i}"), false);
    }
    let merged = Request::watch(Some(many)).merge(Request::watch(Some(more)));
    assert_eq!(merged.folders, None);
}

#[test]
fn a_scanner_merges_a_waiting_targeted_request() {
    let mut fx = Fx::large(40);
    let scanner = Scanner::new(fx.take_album(), fx.env.clone());
    scanner.request(Request::default());
    scanner.request(Request::watch(Some(folders(&[("a", false)]))));
    scanner.request(Request::watch(Some(folders(&[("b", false)]))));
    scanner.wait();
    let last = scanner.last().unwrap();
    assert!(last.error.is_none());
}
