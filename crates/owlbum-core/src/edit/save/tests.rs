//! Save tests. Every album is a temporary folder of generated pictures; the
//! bin is a stand-in that moves files into a folder of the test's.

use std::sync::atomic::AtomicU32;

use super::*;
use crate::album::OpenOptions;
use crate::album::undo::Step;
use crate::edit::IDENTITY;
use crate::edit::store::{self, EditStep};

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

/// `album` comes first so it closes its database before `tmp` deletes the
/// folder (fields drop in order).
struct Fixture {
    album: Album,
    tmp: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("album")).unwrap();
        std::fs::create_dir_all(tmp.path().join("bin")).unwrap();
        let album = Album::open(&tmp.path().join("album"), &OpenOptions::default()).unwrap();
        Self { album, tmp }
    }

    fn root(&self) -> PathBuf {
        self.album.location().root.clone()
    }

    fn renders(&self) -> PathBuf {
        self.tmp.path().join("renders")
    }

    fn bin_dir(&self) -> PathBuf {
        self.tmp.path().join("bin")
    }

    /// A 4 × 2 picture, left half red, right half blue, as `name` (PNG or
    /// WebP by extension), with its row, starred; answers the id.
    fn photo(&self, name: &str) -> i64 {
        let path = self.root().join(name);
        let img =
            image::RgbaImage::from_fn(4, 2, |x, _| image::Rgba(if x < 2 { RED } else { BLUE }));
        img.save(&path).unwrap();
        let format = crate::media::format_of(name).unwrap().id;
        let key = keys::sample(&path).unwrap().file_key;
        let name = name.to_owned();
        self.album
            .write_photos(move |tx, touched| {
                tx.execute(
                    "INSERT INTO photos (rel_path, rel_dir, file_name, size, mtime_ms, file_key, thumb_key,
                                         kind, format, width, height, scan_version, analysis, star)
                     VALUES (?1, '', ?1, 1, 1, ?2, 'tk', 'image', ?3, 4, 2, 1, '{}', 1)",
                    params![name, key, format],
                )
                .unwrap();
                let id = tx.last_insert_rowid();
                touched.photo(id);
                Ok(id)
            })
            .unwrap()
    }

    fn flip(&self, id: i64) {
        store::set(
            &self.album,
            id,
            Some(PhotoEdit {
                flip: true,
                ..IDENTITY
            }),
        )
        .unwrap();
    }

    fn column(&self, id: i64, col: &str) -> rusqlite::types::Value {
        let sql = format!("SELECT {col} FROM photos WHERE id = ?1");
        self.album
            .db()
            .read(move |c| Ok(c.query_row(&sql, [id], |r| r.get(0)).unwrap()))
            .unwrap()
    }

    fn edit(&self, id: i64) -> Option<String> {
        match self.column(id, "edit") {
            rusqlite::types::Value::Text(t) => Some(t),
            _ => None,
        }
    }

    fn drawn(&self, id: i64) -> Option<String> {
        match self.column(id, DRAWN_EDIT) {
            rusqlite::types::Value::Text(t) => Some(t),
            _ => None,
        }
    }

    fn records(&self) -> usize {
        records(&self.album).unwrap().len()
    }

    fn renders_left(&self) -> usize {
        std::fs::read_dir(self.renders()).map_or(0, Iterator::count)
    }
}

/// A stand-in bin that moves files into `dir`.
fn bin_into(dir: PathBuf) -> impl Fn(&Path) -> Result<(), String> + Sync {
    move |p: &Path| std::fs::rename(p, dir.join(p.file_name().unwrap())).map_err(|e| e.to_string())
}

fn env<'a>(f: &Fixture, bin: &'a (dyn Fn(&Path) -> Result<(), String> + Sync)) -> SaveEnv<'a> {
    SaveEnv {
        retry_wait: Duration::from_millis(1),
        ..SaveEnv::new(f.renders(), bin)
    }
}

fn pixel(path: &Path, x: u32, y: u32) -> [u8; 4] {
    image::open(path).unwrap().to_rgba8().get_pixel(x, y).0
}

#[test]
fn save_into_the_file_replaces_it_and_clears_the_edit() {
    let f = Fixture::new();
    let id = f.photo("a.png");
    f.flip(id);
    let key_before = f.column(id, "file_key");
    let bin = bin_into(f.bin_dir());
    let out = save_to_file(&f.album, &env(&f, &bin), &[id], &|_| {}).unwrap();
    assert_eq!(out.saved, vec![id]);
    assert!(out.failed.is_empty());
    // The original is in the bin, unchanged; the path holds the render.
    assert_eq!(pixel(&f.bin_dir().join("a.png"), 0, 0), RED);
    assert_eq!(pixel(&f.root().join("a.png"), 0, 0), BLUE);
    assert_eq!(pixel(&f.root().join("a.png"), 3, 0), RED);
    assert_eq!(f.edit(id), None);
    assert_ne!(f.column(id, "file_key"), key_before);
    assert_eq!(f.column(id, "star"), rusqlite::types::Value::Integer(1));
    assert_eq!(f.column(id, "analysis"), rusqlite::types::Value::Null);
    assert_eq!(f.records(), 0);
    assert_eq!(f.renders_left(), 0);
    // The edit step that held this photo's edit is gone from undo.
    assert!(f.album.undo().is_empty());
    assert_eq!(f.album.saves().count(), 0);
}

#[test]
fn save_keeps_other_photos_in_an_edit_step() {
    let f = Fixture::new();
    let a = f.photo("a.png");
    let b = f.photo("b.png");
    store::turn(&f.album, vec![a, b], crate::edit::TurnOp::Flip).unwrap();
    let bin = bin_into(f.bin_dir());
    save_to_file(&f.album, &env(&f, &bin), &[a], &|_| {}).unwrap();
    assert_eq!(f.album.undo().len(), 1);
    match f.album.undo().pop().unwrap() {
        Step::Edit(EditStep { before, .. }) => assert_eq!(before, vec![(b, None)]),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_bin_that_refuses_changes_nothing() {
    let f = Fixture::new();
    let id = f.photo("a.png");
    f.flip(id);
    let refuse = |_: &Path| Err("no bin here".to_owned());
    let e = save_one(&f.album, &env(&f, &refuse), id).unwrap_err();
    assert_eq!(e.code, ErrorCode::EditOriginalNotMoved);
    assert!(
        e.message.contains("so nothing was changed: no bin here"),
        "{}",
        e.message
    );
    assert_eq!(pixel(&f.root().join("a.png"), 0, 0), RED);
    assert!(f.edit(id).is_some());
    assert_eq!(f.records(), 0);
    assert_eq!(f.renders_left(), 0);
    assert_eq!(f.album.saves().count(), 0);
}

#[test]
fn a_render_that_fails_changes_nothing() {
    let f = Fixture::new();
    let id = f.photo("a.png");
    f.flip(id);
    let bin = bin_into(f.bin_dir());
    let fail = |_: &Path, _: &MediaFormat, _: &PhotoEdit, _: SaveFormat, _: &Path| {
        Err(AppError::new(
            ErrorCode::EditRenderFailed,
            text::edit::render_failed("broken"),
        ))
    };
    let e = save_one(
        &f.album,
        &SaveEnv {
            render: &fail,
            ..env(&f, &bin)
        },
        id,
    )
    .unwrap_err();
    assert_eq!(e.message, "Couldn't make the edited photo: broken");
    assert!(f.edit(id).is_some());
    assert!(f.root().join("a.png").exists());
    assert_eq!(f.records(), 0);
}

#[test]
fn a_move_that_fails_keeps_the_render_and_the_record_and_the_next_open_finishes() {
    let f = Fixture::new();
    let id = f.photo("a.png");
    f.flip(id);
    let bin = bin_into(f.bin_dir());
    let refuse = |_: &Path, _: &Path| Err(io::Error::other("disk full"));
    let e = save_one(
        &f.album,
        &SaveEnv {
            rename: &refuse,
            ..env(&f, &bin)
        },
        id,
    )
    .unwrap_err();
    assert_eq!(e.code, ErrorCode::EditNotPlaced);
    assert!(e.message.contains("Restore the original"), "{}", e.message);
    assert_eq!(f.records(), 1);
    assert_eq!(f.renders_left(), 1);
    // While the record is there the edit is kept but not drawn.
    assert!(f.edit(id).is_some());
    assert_eq!(f.drawn(id), None);

    let notices = recover(&f.album, &env(&f, &bin));
    assert!(notices.is_empty());
    assert_eq!(pixel(&f.root().join("a.png"), 0, 0), BLUE);
    assert_eq!(f.edit(id), None);
    assert_eq!(f.records(), 0);
}

#[test]
fn a_busy_target_is_tried_three_times() {
    let f = Fixture::new();
    let bin = bin_into(f.bin_dir());
    let tries = AtomicU32::new(0);
    let busy_twice = |a: &Path, b: &Path| {
        if tries.fetch_add(1, Ordering::SeqCst) < 2 {
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        } else {
            std::fs::rename(a, b)
        }
    };
    let from = f.tmp.path().join("from");
    let to = f.tmp.path().join("to");
    std::fs::write(&from, b"x").unwrap();
    move_into_place(
        &from,
        &to,
        &SaveEnv {
            rename: &busy_twice,
            ..env(&f, &bin)
        },
    )
    .unwrap();
    assert!(to.exists());

    let count = AtomicU32::new(0);
    let always = |_: &Path, _: &Path| {
        count.fetch_add(1, Ordering::SeqCst);
        Err(io::Error::from(io::ErrorKind::PermissionDenied))
    };
    std::fs::write(&from, b"x").unwrap();
    assert!(
        move_into_place(
            &from,
            &to,
            &SaveEnv {
                rename: &always,
                ..env(&f, &bin)
            }
        )
        .is_err()
    );
    assert_eq!(count.load(Ordering::SeqCst), MOVE_TRIES);
}

#[test]
fn another_drive_copies_through_a_part_file() {
    let f = Fixture::new();
    let bin = bin_into(f.bin_dir());
    // The direct rename is refused as across drives; the part file's
    // rename beside the target works.
    let across = |a: &Path, b: &Path| {
        if a.extension().is_some_and(|e| e == "part") {
            std::fs::rename(a, b)
        } else {
            Err(io::Error::from(io::ErrorKind::CrossesDevices))
        }
    };
    let from = f.tmp.path().join("from.png");
    let dir = f.tmp.path().join("other");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&from, b"pixels").unwrap();
    let env = SaveEnv {
        rename: &across,
        ..env(&f, &bin)
    };
    move_into_place(&from, &dir.join("to.png"), &env).unwrap();
    assert!(!from.exists());
    assert_eq!(std::fs::read(dir.join("to.png")).unwrap(), b"pixels");
    assert_eq!(
        std::fs::read_dir(&dir).unwrap().count(),
        1,
        "no .part file is left"
    );

    // A failure removes the part file and leaves the source.
    let refuse = |_: &Path, _: &Path| Err(io::Error::from(io::ErrorKind::CrossesDevices));
    std::fs::write(&from, b"pixels").unwrap();
    let env = SaveEnv {
        rename: &refuse,
        ..env
    };
    assert!(move_into_place(&from, &dir.join("again.png"), &env).is_err());
    assert!(from.exists());
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
}

#[test]
fn the_checks_refuse_with_their_messages() {
    let f = Fixture::new();
    let bin = bin_into(f.bin_dir());
    let env = env(&f, &bin);
    let plain = f.photo("plain.png");
    let e = save_one(&f.album, &env, plain).unwrap_err();
    assert_eq!(
        (e.code, e.message.as_str()),
        (ErrorCode::EditNone, "plain.png has no edit to save.")
    );

    let webp = f.photo("c.webp");
    f.flip(webp);
    let e = save_one(&f.album, &env, webp).unwrap_err();
    assert_eq!(e.code, ErrorCode::EditCopyOnly);
    assert_eq!(e.message, "WebP photos can only be saved as a copy.");

    let e = save_one(&f.album, &env, 999).unwrap_err();
    assert_eq!(e.message, "That photo is not in the album any more.");

    let busy = f.photo("busy.png");
    f.flip(busy);
    let claim = f.album.saves().claim(busy, "busy.png").unwrap();
    assert!(f.album.saves().skips("busy.png"));
    let e = save_one(&f.album, &env, busy).unwrap_err();
    assert_eq!(e.code, ErrorCode::EditBusy);
    drop(claim);
    assert!(!f.album.saves().skips("busy.png"));
}

#[test]
fn a_later_change_is_kept_and_reported() {
    let f = Fixture::new();
    let id = f.photo("a.png");
    f.flip(id);
    let bin = bin_into(f.bin_dir());
    let album = &f.album;
    // The edit changes while the render runs.
    let render = |s: &Path, fmt: &MediaFormat, e: &PhotoEdit, t: SaveFormat, o: &Path| {
        let out = render_edit(s, fmt, e, t, o);
        store::turn(album, vec![id], crate::edit::TurnOp::RotateRight).unwrap();
        out
    };
    let env = SaveEnv {
        render: &render,
        ..env(&f, &bin)
    };
    let out = save_to_file(album, &env, &[id], &|_| {}).unwrap();
    assert_eq!(
        out.warnings,
        vec!["A later change to a.png was not saved".to_owned()]
    );
    assert!(f.edit(id).is_some());
}

#[test]
fn save_all_skips_copy_only_photos_and_reports_progress() {
    let f = Fixture::new();
    let a = f.photo("a.png");
    let b = f.photo("b.png");
    let c = f.photo("c.webp");
    f.photo("plain.png");
    for id in [a, b, c] {
        f.flip(id);
    }
    assert_eq!(
        status(&f.album, EditTarget::File).unwrap(),
        EditStatus {
            edited: 3,
            waiting: 3,
            saving: 0
        }
    );
    assert_eq!(status(&f.album, EditTarget::Album).unwrap().waiting, 0);
    let bin = bin_into(f.bin_dir());
    let seen = Mutex::new(Vec::new());
    let out = save_all(&f.album, &env(&f, &bin), &|p| seen.lock().unwrap().push(p)).unwrap();
    assert_eq!(out.saved, vec![a, b]);
    assert_eq!(out.skipped, 1);
    let seen = seen.into_inner().unwrap();
    assert_eq!(seen.len(), 3);
    assert_eq!(
        seen[0],
        SaveProgress {
            done: 0,
            total: 2,
            current: Some("a.png".into())
        }
    );
    assert_eq!(
        seen[2],
        SaveProgress {
            done: 2,
            total: 2,
            current: None
        }
    );
    assert_eq!(status(&f.album, EditTarget::File).unwrap().edited, 1);
}

#[test]
fn save_all_stops_when_asked() {
    let f = Fixture::new();
    let a = f.photo("a.png");
    f.flip(a);
    f.album.saves().stop();
    let bin = bin_into(f.bin_dir());
    let out = save_all(&f.album, &env(&f, &bin), &|_| {}).unwrap();
    assert!(out.saved.is_empty());
}

#[test]
fn recovery_settles_each_row_of_the_table() {
    let f = Fixture::new();
    let bin = bin_into(f.bin_dir());
    let env = env(&f, &bin);
    let record = |id: i64, file_key: &str| {
        write_record(
            &f.album,
            id,
            &SaveRecord {
                edit: serde_json::json!({"rotate": 0, "flip": true}),
                file_key: file_key.to_owned(),
                render: "gone.png".to_owned(),
                started_at: 1,
            },
        )
        .unwrap();
    };
    // Same file key: the save never replaced the file.
    let same = f.photo("same.png");
    f.flip(same);
    let key = keys::sample(&f.root().join("same.png")).unwrap().file_key;
    record(same, &key);
    // Another key: the save replaced it.
    let replaced = f.photo("replaced.png");
    f.flip(replaced);
    record(replaced, "fk1:1:old");
    // No file, no render.
    let lost = f.photo("lost.png");
    f.flip(lost);
    record(lost, "fk1:1:old");
    std::fs::remove_file(f.root().join("lost.png")).unwrap();

    let notices = recover(&f.album, &env);
    assert_eq!(f.records(), 0);
    assert!(f.edit(same).is_some(), "the edit stays");
    assert_eq!(f.edit(replaced), None, "the edit is in the file now");
    assert!(f.edit(lost).is_some());
    assert_eq!(notices.len(), 1);
    assert!(notices[0].starts_with("lost.png was being saved when Owlbum stopped."));
}

#[test]
fn the_scanner_settles_a_record_whose_file_is_there_and_leaves_the_rest() {
    let f = Fixture::new();
    let record = |id: i64| {
        write_record(
            &f.album,
            id,
            &SaveRecord {
                edit: serde_json::json!({"rotate": 0, "flip": true}),
                file_key: "fk1:1:old".to_owned(),
                render: "gone.png".to_owned(),
                started_at: 1,
            },
        )
        .unwrap();
    };
    // Step 7 failed after the file was replaced: the scan finishes it.
    let replaced = f.photo("replaced.png");
    f.flip(replaced);
    record(replaced);
    // No file: left for the next open.
    let lost = f.photo("lost.png");
    f.flip(lost);
    record(lost);
    std::fs::remove_file(f.root().join("lost.png")).unwrap();
    // Being saved now: left to its save.
    let busy = f.photo("busy.png");
    f.flip(busy);
    record(busy);
    let claim = f.album.saves().claim(busy, "busy.png").unwrap();

    assert_eq!(settle_found(&f.album), 1);
    assert_eq!(f.edit(replaced), None, "the edit is in the file now");
    assert!(f.edit(lost).is_some());
    assert!(f.edit(busy).is_some());
    assert_eq!(f.records(), 2);
    drop(claim);
}

#[test]
fn old_renders_are_cleaned_unless_a_record_names_them() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("old.png"), b"x").unwrap();
    std::fs::write(tmp.path().join("kept.png"), b"x").unwrap();
    clean_renders(
        tmp.path(),
        &HashSet::from(["kept.png".to_owned()]),
        Duration::ZERO,
    );
    assert!(!tmp.path().join("old.png").exists());
    assert!(tmp.path().join("kept.png").exists());
    std::fs::write(tmp.path().join("new.png"), b"x").unwrap();
    clean_renders(tmp.path(), &HashSet::new(), RENDER_KEPT);
    assert!(tmp.path().join("new.png").exists());
}

#[test]
fn save_as_names_forces_and_refuses() {
    let f = Fixture::new();
    let id = f.photo("trip.png");
    f.flip(id);
    let d = save_as_dialog(&f.album, id).unwrap();
    assert_eq!(d.default_name, "trip-edited.png");
    assert_eq!(d.filter, "PNG photo");
    assert_eq!(d.extensions, vec!["png".to_owned()]);
    let webp = f.photo("w.webp");
    f.flip(webp);
    assert_eq!(
        save_as_dialog(&f.album, webp).unwrap().default_name,
        "w-edited.webp"
    );

    assert_eq!(
        force_extension(Path::new("x/a.JPEG"), &[".jpg", ".jpeg"]),
        (PathBuf::from("x/a.JPEG"), false)
    );
    assert_eq!(
        force_extension(Path::new("x/a.gif"), &[".png"]),
        (PathBuf::from("x/a.png"), true)
    );
    assert_eq!(
        force_extension(Path::new("x/a"), &[".png"]),
        (PathBuf::from("x/a.png"), true)
    );

    let bin = bin_into(f.bin_dir());
    let env = env(&f, &bin);
    let e = save_as(&f.album, &env, id, &f.root().join("trip.png"), &|_| {}).unwrap_err();
    assert_eq!(e.code, ErrorCode::SaveAsSameFile);
    if cfg!(windows) {
        let e = save_as(&f.album, &env, id, &f.root().join("TRIP.PNG"), &|_| {}).unwrap_err();
        assert_eq!(e.code, ErrorCode::SaveAsSameFile);
    }
    // Forcing names a file that exists, which the dialog did not ask about.
    let e = save_as(&f.album, &env, id, &f.root().join("trip.gif"), &|_| {}).unwrap_err();
    assert_eq!(e.code, ErrorCode::SaveAsExists);
}

#[test]
fn save_as_outside_the_album_writes_a_copy_and_puts_a_replaced_file_in_the_bin() {
    let f = Fixture::new();
    let id = f.photo("trip.png");
    f.flip(id);
    let out_dir = f.tmp.path().join("out");
    std::fs::create_dir_all(&out_dir).unwrap();
    std::fs::write(out_dir.join("copy.png"), b"old").unwrap();
    let bin = bin_into(f.bin_dir());
    let out = save_as(
        &f.album,
        &env(&f, &bin),
        id,
        &out_dir.join("copy.png"),
        &|_| panic!("no scan outside the album"),
    )
    .unwrap();
    assert_eq!(out.saved_name.as_deref(), Some("copy.png"));
    assert!(!out.in_album);
    assert_eq!(std::fs::read(f.bin_dir().join("copy.png")).unwrap(), b"old");
    assert_eq!(pixel(&out_dir.join("copy.png"), 0, 0), BLUE);
    // The original and its edit are as they were.
    assert_eq!(pixel(&f.root().join("trip.png"), 0, 0), RED);
    assert!(f.edit(id).is_some());
}

#[test]
fn save_as_inside_the_album_copies_typed_info_but_not_the_edit() {
    let f = Fixture::new();
    let id = f.photo("trip.png");
    f.flip(id);
    f.album
        .write_photos(move |tx, t| {
            tx.execute(
                "UPDATE photos SET description = 'beach', hidden = 1, score_adjust = 3 WHERE id = ?1",
                [id],
            )
            .unwrap();
            t.photo(id);
            Ok(())
        })
        .unwrap();
    let bin = bin_into(f.bin_dir());
    let album = &f.album;
    // Stands in for the scan of the folder: gives the copy a row.
    let rescan = |dir: &str| {
        assert_eq!(dir, "");
        album
            .write_photos(|tx, t| {
                tx.execute(
                    "INSERT INTO photos (rel_path, rel_dir, file_name, size, mtime_ms, file_key, thumb_key,
                                         kind, format, width, height, scan_version)
                     VALUES ('trip-edited.png', '', 'trip-edited.png', 1, 1, 'k', 't', 'image', 'png', 4, 2, 1)",
                    [],
                )
                .unwrap();
                t.photo(tx.last_insert_rowid());
                Ok(())
            })
            .unwrap();
    };
    let out = save_as(
        album,
        &env(&f, &bin),
        id,
        &f.root().join("trip-edited.png"),
        &rescan,
    )
    .unwrap();
    assert!(out.in_album);
    assert_eq!(out.warning, None);
    let copy = out.photo_id.unwrap();
    assert_eq!(f.column(copy, "star"), rusqlite::types::Value::Integer(1));
    assert_eq!(
        f.column(copy, "description"),
        rusqlite::types::Value::Text("beach".into())
    );
    assert_eq!(f.column(copy, "hidden"), rusqlite::types::Value::Integer(0));
    assert_eq!(
        f.column(copy, "score_adjust"),
        rusqlite::types::Value::Integer(0)
    );
    assert_eq!(f.edit(copy), None);
}

#[test]
fn wait_idle_answers_when_the_last_claim_goes() {
    let saves = Saves::default();
    assert!(saves.wait_idle(Duration::ZERO));
    let claim = saves.claim(1, "a").unwrap();
    assert!(!saves.wait_idle(Duration::from_millis(1)));
    drop(claim);
    assert!(saves.wait_idle(Duration::ZERO));
}
