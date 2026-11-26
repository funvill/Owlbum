//! Tidy tests. Every album is a temporary folder; the only files are the
//! ones a test makes.

use serde_json::json;

use super::*;
use crate::album::OpenOptions;
use crate::album::undo::{UndoOutcome, undo};
use crate::index::AlbumIndex;
use crate::info::typed::{InfoPatch, set_info};

fn album(dir: &std::path::Path) -> Album {
    Album::open(dir, &OpenOptions::default()).unwrap()
}

/// Adds a row for `rel_path` (its file is not made) and answers its id.
/// `extra` sets more columns, as `col = value, ...`.
fn row(album: &Album, rel_path: &str, extra: &str) -> i64 {
    let rel_path = rel_path.to_owned();
    let extra = extra.to_owned();
    album
        .write_photos(move |tx, touched| {
            let (rel_dir, file_name) = rel_path.rsplit_once('/').unwrap_or(("", &rel_path));
            tx.execute(
                "INSERT INTO photos (rel_path, rel_dir, file_name, size, mtime_ms, file_key, thumb_key,
                                     kind, format, width, height, scan_version)
                 VALUES (?1, ?2, ?3, 1, 1, 'fk', 'tk', 'image', 'jpeg', 10, 10, 1)",
                [&rel_path, rel_dir, file_name],
            )
            .unwrap();
            let id = tx.last_insert_rowid();
            if !extra.is_empty() {
                tx.execute(&format!("UPDATE photos SET {extra} WHERE id = ?1"), [id])
                    .unwrap();
            }
            crate::album::search::reindex(tx, id).unwrap();
            touched.photo(id);
            Ok(id)
        })
        .unwrap()
}

fn set(album: &Album, ids: &[i64], v: serde_json::Value) {
    let patch: InfoPatch = serde_json::from_value(v).unwrap();
    set_info(album, ids.to_vec(), patch).unwrap();
}

fn mark_missing(album: &Album, ids: &[i64]) {
    let ids = ids.to_vec();
    album
        .write_photos(move |tx, touched| {
            for &id in &ids {
                tx.execute("UPDATE photos SET missing = 1 WHERE id = ?1", [id])
                    .unwrap();
            }
            touched.photos(ids);
            Ok(())
        })
        .unwrap();
}

fn count(album: &Album, sql: &'static str) -> i64 {
    album
        .db()
        .read(move |c| Ok(c.query_row(sql, [], |r| r.get(0)).unwrap()))
        .unwrap()
}

fn exists(album: &Album, id: i64) -> bool {
    album
        .db()
        .read(move |c| {
            Ok(
                c.query_row("SELECT count(*) FROM photos WHERE id = ?1", [id], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap()
                    == 1,
            )
        })
        .unwrap()
}

#[test]
fn the_list_groups_missing_rows_by_folder_with_what_they_would_lose() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("Day 2")).unwrap();
    let album = album(tmp.path());
    let top = row(&album, "lost.jpg", "missing = 1");
    let a10 = row(&album, "Day 2/IMG_10.jpg", "");
    let a9 = row(
        &album,
        "Day 2/IMG_9.jpg",
        "missing = 1, star = 1, notes = 'x', edit = '{}'",
    );
    let _live = row(&album, "Day 2/here.jpg", "");
    let scans = row(&album, "Scans/s.jpg", "missing = 1");
    set(&album, &[a10], json!({ "people": ["Ann", "Bob"] }));
    set(
        &album,
        &[a10],
        json!({ "date_override": "2020-01-02 03:04" }),
    );
    mark_missing(&album, &[a10]);

    let got = list(&album, false).unwrap();
    let shape: Vec<(&str, bool, Vec<i64>)> = got
        .iter()
        .map(|f| {
            (
                f.folder.as_str(),
                f.gone,
                f.files.iter().map(|x| x.id).collect(),
            )
        })
        .collect();
    assert_eq!(
        shape,
        [
            ("", false, vec![top]),
            ("Day 2", false, vec![a9, a10]),
            ("Scans", true, vec![scans]),
        ],
        "natural order; a folder not on disk is gone; live rows left out"
    );
    assert_eq!(got[1].files[0].keeps, ["starred", "notes", "edit"]);
    assert_eq!(got[1].files[1].keeps, ["2 people", "date"]);
    assert!(got[0].files[0].keeps.is_empty());
}

#[test]
fn a_scoped_open_lists_and_removes_only_rows_under_the_scope() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("in/deep")).unwrap();
    let (inside, outside, prefix) = {
        let album = album(tmp.path());
        let inside = row(&album, "in/deep/a.jpg", "missing = 1");
        let outside = row(&album, "out/b.jpg", "missing = 1");
        // Shares the scope's letters but is not under it.
        let prefix = row(&album, "in2/c.jpg", "missing = 1");
        (inside, outside, prefix)
    };
    let album = album(&tmp.path().join("in"));
    assert!(album.location().is_scoped());
    let got = list(&album, false).unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].folder, "in/deep");
    let r = remove(&album, vec![inside, outside, prefix], false).unwrap();
    assert_eq!((r.removed, r.kept), (1, 0));
    assert!(!exists(&album, inside));
    assert!(exists(&album, outside) && exists(&album, prefix));
}

#[test]
fn removing_deletes_the_rows_and_everything_hung_on_them_but_no_file() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("keep.jpg"), b"not touched").unwrap();
    let album = album(tmp.path());
    let gone = row(&album, "gone.jpg", "description = 'tower'");
    let live = row(&album, "keep.jpg", "");
    set(&album, &[gone], json!({ "people": ["Ann", "Bob"] }));
    set(&album, &[live], json!({ "people": ["Bob"] }));
    album
        .write_photos(move |tx, _| {
            tx.execute(
                "INSERT INTO tags (photo_id, part, model_id, status, made_at) VALUES (?1, 'describe', 'm', 'done', 1)",
                [gone],
            )
            .unwrap();
            tx.execute(
                "INSERT INTO photo_labels (kind, label, photo_id) VALUES ('keyword', 'tower', ?1)",
                [gone],
            )
            .unwrap();
            tx.execute(
                "INSERT INTO vectors (photo_id, model_id, dim, vector) VALUES (?1, 'm', 1, x'00000000')",
                [gone],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
    mark_missing(&album, &[gone]);
    let rev = album.revision();

    let r = remove(&album, vec![gone], false).unwrap();
    assert_eq!((r.removed, r.kept), (1, 0));
    assert!(r.revision > rev);
    assert!(!exists(&album, gone));
    assert!(exists(&album, live));
    assert_eq!(count(&album, "SELECT count(*) FROM tags"), 0);
    assert_eq!(count(&album, "SELECT count(*) FROM photo_labels"), 0);
    assert_eq!(count(&album, "SELECT count(*) FROM vectors"), 0);
    assert_eq!(count(&album, "SELECT count(*) FROM photo_people"), 1);
    assert_eq!(
        count(&album, "SELECT count(*) FROM people WHERE name = 'Ann'"),
        0,
        "a name left on no photo is forgotten"
    );
    assert_eq!(
        count(&album, "SELECT count(*) FROM people WHERE name = 'Bob'"),
        1
    );
    assert_eq!(
        count(
            &album,
            "SELECT count(*) FROM search WHERE search MATCH 'tower'"
        ),
        0,
        "the row left the search table"
    );
    assert_eq!(
        std::fs::read(tmp.path().join("keep.jpg")).unwrap(),
        b"not touched"
    );
    assert!(list(&album, false).unwrap().is_empty());
}

#[test]
fn a_row_that_is_not_missing_is_refused_and_a_file_found_again_is_kept() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let live = row(&album, "live.jpg", "");
    let back = row(&album, "back.jpg", "missing = 1, star = 1");
    let gone = row(&album, "gone.jpg", "missing = 1");
    std::fs::write(tmp.path().join("back.jpg"), b"x").unwrap();

    let r = remove(&album, vec![live, back, gone, 999], false).unwrap();
    assert_eq!((r.removed, r.kept), (1, 1));
    assert!(exists(&album, live), "a live row is never deleted");
    assert!(exists(&album, back));
    assert!(!exists(&album, gone));
    assert_eq!(
        count(
            &album,
            "SELECT missing FROM photos WHERE rel_path = 'back.jpg'"
        ),
        0,
        "found again: no longer missing"
    );
    assert!(
        album.index().read(|v| v.get(back as u32).is_some()),
        "and back in the album index"
    );
}

#[test]
fn nothing_to_remove_writes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let live = row(&album, "live.jpg", "");
    let rev = album.revision();
    let r = remove(&album, vec![live], false).unwrap();
    assert_eq!((r.removed, r.kept, r.revision), (0, 0, rev));
}

#[test]
fn a_running_scan_or_a_view_only_album_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let gone = {
        let album = album(tmp.path());
        let gone = row(&album, "gone.jpg", "missing = 1");
        let e = list(&album, true).unwrap_err();
        assert_eq!(e.code, ErrorCode::ScanRunning);
        let e = remove(&album, vec![gone], true).unwrap_err();
        assert_eq!(e.code, ErrorCode::ScanRunning);
        assert!(exists(&album, gone));
        gone
    };
    let options = OpenOptions {
        view_only: true,
        ..OpenOptions::default()
    };
    let album = Album::open(tmp.path(), &options).unwrap();
    assert!(album.is_view_only());
    let e = remove(&album, vec![gone], false).unwrap_err();
    assert_eq!(e.code, ErrorCode::ViewOnly);
    assert!(exists(&album, gone));
}

#[test]
fn undo_steps_go_only_when_every_photo_in_them_was_removed() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let a = row(&album, "a.jpg", "");
    let b = row(&album, "b.jpg", "");
    set(&album, &[a, b], json!({ "star": true })); // both
    set(&album, &[a], json!({ "hidden": true })); // a only
    set(&album, &[b], json!({ "notes": "kept" })); // b only
    mark_missing(&album, &[a]);
    assert_eq!(album.undo().len(), 3);

    let r = remove(&album, vec![a], false).unwrap();
    assert_eq!(r.removed, 1);
    assert_eq!(album.undo().len(), 2, "the hide of a alone is dropped");
    let u = undo(&album).unwrap();
    assert_eq!(u.label.as_deref(), Some("notes"));
    let u = undo(&album).unwrap();
    assert_eq!(u.label.as_deref(), Some("star"));
    assert_eq!(u.outcome, UndoOutcome::Partial, "b is put back, a is gone");
    assert_eq!(undo(&album).unwrap().outcome, UndoOutcome::Nothing);
}

/// What the album index holds of each photo's burst, keeper and score.
fn bursts_of(index: &AlbumIndex) -> Vec<(u32, Option<u32>, bool, Option<i32>)> {
    index.read(|v| {
        v.photos()
            .iter()
            .map(|p| {
                (
                    p.id,
                    p.derived.burst.map(|b| b.get()),
                    p.derived.keeper,
                    p.derived.score,
                )
            })
            .collect()
    })
}

fn fresh(album: &Album) -> AlbumIndex {
    album
        .db()
        .read(|c| AlbumIndex::load(c, &album.location().scope))
        .unwrap()
}

/// Three shots of one burst, the last the sharpest.
fn burst(album: &Album) -> [i64; 3] {
    [1, 2, 3].map(|n| {
        row(
            album,
            &format!("day/IMG_{n}.jpg"),
            &format!(
                "captured_at = '2024-06-01T10:0{n}:00', analysis = '{{\"sharpness\":{},\"mean_luma\":0.5,\"clipped_shadows\":0,\"clipped_highlights\":0,\"saliency_x\":0.5,\"saliency_y\":0.5,\"dhash\":\"8f0c3c3e1e0e0603\",\"avg_colour\":[90,90,90]}}'",
                n * 10
            ),
        )
    })
}

#[test]
fn removing_and_finding_again_keep_bursts_keepers_and_scores_as_a_fresh_load() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("day")).unwrap();
    let album = album(tmp.path());
    let [a, b, c] = burst(&album);
    album.index().rebuild();
    // The keeper's file goes, then comes back before Tidy; a's goes for
    // good.
    mark_missing(&album, &[a, c]);
    album.index().rebuild();
    assert_eq!(bursts_of(album.index()), bursts_of(&fresh(&album)));
    std::fs::write(tmp.path().join("day/IMG_3.jpg"), b"x").unwrap();

    let r = remove(&album, vec![a, c], false).unwrap();
    assert_eq!((r.removed, r.kept), (1, 1));
    let got = bursts_of(album.index());
    assert_eq!(got, bursts_of(&fresh(&album)));
    let ids: Vec<u32> = got.iter().map(|g| g.0).collect();
    assert_eq!(ids, [b as u32, c as u32]);
    assert_eq!(got[0].1, Some(b as u32), "one burst again");
    assert_eq!(got[1].1, Some(b as u32));
    assert!(got[1].2, "the sharpest, found again, is the keeper");
    assert!(!got[0].2);
}
