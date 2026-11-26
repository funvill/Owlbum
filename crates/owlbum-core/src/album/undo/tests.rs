use serde_json::json;

use super::*;
use crate::album::{OpenOptions, sqlite_error};
use crate::info::typed::{InfoPatch, set_info};
use crate::organise::nudge::nudge;
use crate::text;

fn album(dir: &std::path::Path) -> Album {
    Album::open(dir, &OpenOptions::default()).unwrap()
}

/// Adds a photo with a camera date and GPS, and answers its id.
fn photo(album: &Album, name: &str) -> i64 {
    let name = name.to_owned();
    album
        .write_photos(move |tx, touched| {
            tx.execute(
                "INSERT INTO photos (rel_path, rel_dir, file_name, size, mtime_ms, file_key, thumb_key,
                                     kind, format, width, height, scan_version, captured_at, gps_lat, gps_lon)
                 VALUES (?1, '', ?1, 1, 1, 'fk', 'tk', 'image', 'jpeg', 10, 10, 1,
                         '2024-06-12T14:03:00', 48.85837, 2.29448)",
                [&name],
            )
            .map_err(sqlite_error(text::album::WRITING_DATABASE))?;
            let id = tx.last_insert_rowid();
            touched.photo(id);
            Ok(id)
        })
        .unwrap()
}

fn set(album: &Album, ids: &[i64], v: serde_json::Value) {
    let patch: InfoPatch = serde_json::from_value(v).unwrap();
    set_info(album, ids.to_vec(), patch).unwrap();
}

/// One column of photo `id`, as text (NULL as "NULL").
fn col(album: &Album, id: i64, column: &'static str) -> String {
    album
        .db()
        .read(move |c| {
            Ok(c.query_row(
                &format!(
                    "SELECT coalesce(CAST({column} AS TEXT), 'NULL') FROM photos WHERE id = ?1"
                ),
                [id],
                |r| r.get(0),
            )
            .unwrap())
        })
        .unwrap()
}

fn people(album: &Album, id: i64) -> Vec<String> {
    album
        .db()
        .read(move |c| {
            let mut st = c
                .prepare(
                    "SELECT p.name FROM photo_people pp JOIN people p ON p.id = pp.person_id
                      WHERE pp.photo_id = ?1 ORDER BY p.name",
                )
                .unwrap();
            Ok(st
                .query_map([id], |r| r.get(0))
                .unwrap()
                .collect::<Result<Vec<String>, _>>()
                .unwrap())
        })
        .unwrap()
}

fn mark_missing(album: &Album, id: i64) {
    album
        .write_photos(move |tx, touched| {
            tx.execute("UPDATE photos SET missing = 1 WHERE id = ?1", [id])
                .unwrap();
            touched.photo(id);
            Ok(())
        })
        .unwrap();
}

#[test]
fn nothing_to_undo() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let rev = album.revision();
    let r = undo(&album).unwrap();
    assert_eq!(r.outcome, UndoOutcome::Nothing);
    assert_eq!(r.label, None);
    assert!(r.ids.is_empty());
    assert_eq!(r.revision, rev);
}

#[test]
fn each_label_is_undone_and_only_its_columns_go_back() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let a = photo(&album, "a.jpg");
    set(&album, &[a], json!({ "star": true }));
    set(&album, &[a], json!({ "hidden": true }));
    set(&album, &[a], json!({ "people": ["Ann", "Bob"] }));
    set(&album, &[a], json!({ "description": "At the tower" }));
    set(&album, &[a], json!({ "notes": "windy" }));
    set(&album, &[a], json!({ "date_override": "2020-01-02 03:04" }));
    set(
        &album,
        &[a],
        json!({ "location": { "lat": 1.5, "lon": 2.5, "name": "Home" } }),
    );
    nudge(&album, vec![a], 1).unwrap();
    assert_eq!(album.undo().len(), 8);

    let step = |label: &str| {
        let r = undo(&album).unwrap();
        assert_eq!(r.outcome, UndoOutcome::Done, "{label}");
        assert_eq!(r.label.as_deref(), Some(label));
        assert_eq!(r.ids, [a]);
    };
    step("score");
    assert_eq!(col(&album, a, "score_adjust"), "0");
    step("place");
    assert_eq!(col(&album, a, "loc_lat"), "NULL");
    assert_eq!(col(&album, a, "loc_name"), "NULL");
    assert_eq!(col(&album, a, "notes"), "windy", "other fields stay");
    step("date");
    assert_eq!(col(&album, a, "date_override"), "NULL");
    step("notes");
    assert_eq!(col(&album, a, "notes"), "NULL");
    step("description");
    assert_eq!(col(&album, a, "description"), "NULL");
    assert_eq!(people(&album, a), ["Ann", "Bob"]);
    step("people");
    assert!(people(&album, a).is_empty());
    let names: i64 = album
        .db()
        .read(|c| {
            Ok(c.query_row("SELECT count(*) FROM people", [], |r| r.get(0))
                .unwrap())
        })
        .unwrap();
    assert_eq!(names, 0, "names no photo uses are forgotten");
    step("hide");
    assert_eq!(col(&album, a, "hidden"), "0");
    assert_eq!(col(&album, a, "star"), "1");
    step("star");
    assert_eq!(col(&album, a, "star"), "0");
    assert_eq!(undo(&album).unwrap().outcome, UndoOutcome::Nothing);
}

#[test]
fn one_step_for_many_photos_and_an_undo_records_no_step() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let a = photo(&album, "a.jpg");
    let b = photo(&album, "b.jpg");
    let c = photo(&album, "c.jpg");
    set(&album, &[b], json!({ "star": true }));
    set(&album, &[a, b, c], json!({ "star": true }));
    assert_eq!(album.undo().len(), 2);
    let rev = album.revision();
    let r = undo(&album).unwrap();
    assert_eq!(r.outcome, UndoOutcome::Done);
    assert_eq!(r.ids, [a, c], "b already had the value");
    assert!(r.revision > rev);
    assert_eq!(album.undo().len(), 1, "an undo records no new step");
    assert_eq!(col(&album, a, "star"), "0");
    assert_eq!(col(&album, b, "star"), "1");
    let row = album.index().read(|v| v.get(a as u32).unwrap().star());
    assert!(!row, "the album index follows");
}

#[test]
fn a_change_that_changes_nothing_records_no_step() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let a = photo(&album, "a.jpg");
    set(&album, &[a], json!({ "star": false }));
    assert!(album.undo().is_empty());
}

#[test]
fn the_list_holds_a_hundred_steps() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let a = photo(&album, "a.jpg");
    for i in 0..(DEPTH + 5) {
        set(&album, &[a], json!({ "star": i % 2 == 0 }));
    }
    assert_eq!(album.undo().len(), DEPTH);
    for _ in 0..DEPTH {
        assert_eq!(undo(&album).unwrap().outcome, UndoOutcome::Done);
    }
    assert_eq!(undo(&album).unwrap().outcome, UndoOutcome::Nothing);
    // The five oldest were dropped: the star is as after the fifth press
    // (i = 4 starred it).
    assert_eq!(col(&album, a, "star"), "1");
}

#[test]
fn photos_that_are_gone_give_partial_or_gone() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let a = photo(&album, "a.jpg");
    let b = photo(&album, "b.jpg");
    set(&album, &[a, b], json!({ "hidden": true }));
    nudge(&album, vec![a], -1).unwrap();
    mark_missing(&album, a);

    let rev = album.revision();
    let r = undo(&album).unwrap();
    assert_eq!(r.outcome, UndoOutcome::Gone);
    assert_eq!(r.label.as_deref(), Some("score"));
    assert!(r.ids.is_empty());
    assert_eq!(r.revision, rev, "nothing written");

    let r = undo(&album).unwrap();
    assert_eq!(r.outcome, UndoOutcome::Partial);
    assert_eq!(r.label.as_deref(), Some("hide"));
    assert_eq!(r.ids, [b]);
    assert_eq!(col(&album, a, "hidden"), "1");
    assert_eq!(col(&album, b, "hidden"), "0");
    assert!(album.undo().is_empty());
}
