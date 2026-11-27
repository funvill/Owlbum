use std::collections::HashSet;

use super::*;
use crate::album::OpenOptions;
use crate::album::undo::{UndoOutcome, undo};
use crate::edit::model::{CropRect, rotate_right};

fn album(dir: &std::path::Path) -> Album {
    Album::open(dir, &OpenOptions::default()).unwrap()
}

/// Adds a 3000 × 2000 photo of `kind` (with `duration` for an animated
/// picture) and answers its id.
fn photo_of(album: &Album, name: &str, kind: &'static str, duration: Option<i64>) -> i64 {
    let name = name.to_owned();
    album
        .write_photos(move |tx, touched| {
            tx.execute(
                "INSERT INTO photos (rel_path, rel_dir, file_name, size, mtime_ms, file_key, thumb_key,
                                     kind, format, width, height, duration_ms, scan_version)
                 VALUES (?1, '', ?1, 1, 1, 'fk', 'tk', ?2, 'jpeg', 3000, 2000, ?3, 1)",
                params![name, kind, duration],
            )
            .map_err(sqlite_error(text::album::WRITING_DATABASE))?;
            let id = tx.last_insert_rowid();
            touched.photo(id);
            Ok(id)
        })
        .unwrap()
}

fn photo(album: &Album, name: &str) -> i64 {
    photo_of(album, name, "image", None)
}

fn stored(album: &Album, id: i64) -> Option<String> {
    album
        .db()
        .read(move |conn| {
            Ok(conn
                .query_row("SELECT edit FROM photos WHERE id = ?1", [id], |r| r.get(0))
                .unwrap())
        })
        .unwrap()
}

fn edit_of(album: &Album, id: i64) -> PhotoEdit {
    stored(album, id).map_or(IDENTITY, |t| parse_stored(&t).unwrap())
}

fn cropped() -> PhotoEdit {
    PhotoEdit {
        crop: CropRect {
            x: 0.0,
            y: 0.0,
            w: 0.5,
            h: 0.25,
        },
        ..IDENTITY
    }
}

#[test]
fn set_stores_the_edit_and_identity_as_null() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let a = photo(&album, "a.jpg");
    let rev = album.revision();
    let r = set(&album, a, Some(cropped())).unwrap();
    assert!(r.changed && r.revision > rev);
    assert_eq!(
        r.plan.unwrap().out,
        crate::edit::plan::PlanSize { w: 1500, h: 500 }
    );
    assert!(same_edit(&edit_of(&album, a), &cropped()));
    assert!(album.index().read(|v| v.get(a as u32).unwrap().has_edit()));

    let r = set(&album, a, Some(cropped())).unwrap();
    assert!(!r.changed, "the same edit writes nothing");
    assert_eq!(album.undo().len(), 1);

    let r = set(&album, a, Some(IDENTITY)).unwrap();
    assert!(r.changed && r.plan.is_none());
    assert_eq!(stored(&album, a), None);
    assert!(!album.index().read(|v| v.get(a as u32).unwrap().has_edit()));
}

#[test]
fn set_and_turn_refuse_what_cannot_be_edited() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let v = photo_of(&album, "v.mp4", "video", Some(5000));
    let g = photo_of(&album, "g.gif", "image", Some(800));
    let a = photo(&album, "a.jpg");
    let e = set(&album, v, Some(cropped())).unwrap_err();
    assert_eq!(
        (e.code, e.message.as_str()),
        (ErrorCode::EditNotEditable, text::edit::VIDEO)
    );
    let e = set(&album, g, Some(cropped())).unwrap_err();
    assert_eq!(e.message, text::edit::ANIMATED);
    let t = turn(&album, vec![v, a, g], TurnOp::RotateRight).unwrap();
    assert_eq!((t.changed, t.skipped), (vec![a], 2));
    let rev = album.revision();
    let t = turn(&album, vec![v], TurnOp::RotateRight).unwrap();
    assert!(t.changed.is_empty());
    assert_eq!(t.revision, rev);
    assert_eq!(album.undo().len(), 1, "nothing turned, nothing recorded");
    assert_eq!(
        turn(&album, vec![999], TurnOp::Flip).unwrap_err().code,
        ErrorCode::PhotoNotFound
    );
}

#[test]
fn turning_many_is_one_step_and_a_crop_turns_with_the_photo() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let a = photo(&album, "a.jpg");
    let b = photo(&album, "b.jpg");
    let c = photo(&album, "c.jpg");
    set(&album, a, Some(cropped())).unwrap();
    let steps = album.undo().len();
    let t = turn(&album, vec![a, b, c, b], TurnOp::RotateRight).unwrap();
    assert_eq!(t.changed, [a, b, c]);
    assert_eq!(album.undo().len(), steps + 1);
    assert!(same_edit(&edit_of(&album, a), &rotate_right(&cropped())));
    assert_eq!(edit_of(&album, b).rotate, 90);

    // Four turns give identity, stored as NULL.
    for _ in 0..3 {
        turn(&album, vec![b], TurnOp::RotateRight).unwrap();
    }
    assert_eq!(stored(&album, b), None);

    let u = undo(&album).unwrap();
    assert_eq!(
        (u.outcome, u.label.as_deref()),
        (UndoOutcome::Done, Some("rotate right"))
    );
    assert_eq!(edit_of(&album, b).rotate, 270);
}

#[test]
fn undo_puts_each_edit_back() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let a = photo(&album, "a.jpg");
    let b = photo(&album, "b.jpg");
    set(&album, a, Some(cropped())).unwrap();
    turn(&album, vec![a, b], TurnOp::Flip).unwrap();
    revert(&album, vec![a, b]).unwrap();
    assert_eq!((stored(&album, a), stored(&album, b)), (None, None));

    let u = undo(&album).unwrap();
    assert_eq!(u.label.as_deref(), Some("revert to original"));
    assert_eq!(u.ids, [a, b]);
    assert!(edit_of(&album, b).flip);
    assert_eq!(undo(&album).unwrap().label.as_deref(), Some("flip"));
    assert!(same_edit(&edit_of(&album, a), &cropped()));
    assert_eq!(stored(&album, b), None);
    assert_eq!(undo(&album).unwrap().label.as_deref(), Some("edit"));
    assert_eq!(stored(&album, a), None);
}

#[test]
fn revert_records_nothing_when_no_photo_has_an_edit() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let a = photo(&album, "a.jpg");
    let r = revert(&album, vec![a]).unwrap();
    assert!(r.changed.is_empty());
    assert!(album.undo().is_empty());
}

#[test]
fn dropping_a_saved_photos_edits_keeps_other_steps() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let a = photo(&album, "a.jpg");
    let b = photo(&album, "b.jpg");
    set(&album, a, Some(cropped())).unwrap();
    turn(&album, vec![a, b], TurnOp::RotateLeft).unwrap();
    album.undo().drop_edits_of(&HashSet::from([a]));
    assert_eq!(
        album.undo().len(),
        1,
        "a's own step is gone; the turn keeps b"
    );
    let u = undo(&album).unwrap();
    assert_eq!(u.ids, [b]);
    assert!(edit_of(&album, a).rotate == 270, "a's edit is left alone");
}

#[test]
fn an_unreadable_stored_edit_counts_as_none() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let a = photo(&album, "a.jpg");
    album
        .write_photos(move |tx, touched| {
            tx.execute(
                "UPDATE photos SET edit = '{\"rotate\":45}' WHERE id = ?1",
                [a],
            )
            .unwrap();
            touched.photo(a);
            Ok(())
        })
        .unwrap();
    assert_eq!(
        stored_plan(a, 3000, 2000, stored(&album, a).as_deref()),
        None
    );
    let r = set(&album, a, None).unwrap();
    assert!(r.changed, "the unreadable text is replaced by NULL");
    assert_eq!(stored(&album, a), None);
    turn(&album, vec![a], TurnOp::RotateLeft).unwrap();
    assert_eq!(edit_of(&album, a).rotate, 270);
}

#[test]
fn a_view_only_album_changes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let a = {
        let album = album(tmp.path());
        photo(&album, "a.jpg")
    };
    let album = Album::open(
        tmp.path(),
        &OpenOptions {
            view_only: true,
            ..OpenOptions::default()
        },
    );
    let Ok(album) = album else { return };
    for e in [
        set(&album, a, Some(cropped())).unwrap_err(),
        turn(&album, vec![a], TurnOp::Flip).unwrap_err(),
        revert(&album, vec![a]).unwrap_err(),
    ] {
        assert_eq!(e.code, ErrorCode::ViewOnly);
    }
}
