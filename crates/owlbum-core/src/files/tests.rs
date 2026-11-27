//! File action tests. Every album is a temporary folder; the bin is a
//! stand-in that moves files into a folder of the test's.

use std::cell::RefCell;
use std::path::PathBuf;

use super::*;
use crate::album::OpenOptions;

fn album(dir: &Path) -> Album {
    Album::open(dir, &OpenOptions::default()).unwrap()
}

/// Adds a row and its file for `rel_path` and answers its id. `extra`
/// sets more columns, as `col = value, ...`.
fn row(album: &Album, rel_path: &str, extra: &str) -> i64 {
    let file = album.location().root.join(rel_path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, b"x").unwrap();
    let rel_path = rel_path.to_owned();
    let extra = extra.to_owned();
    album
        .write_photos(move |tx, touched| {
            let (rel_dir, file_name) = rel_path.rsplit_once('/').unwrap_or(("", &rel_path));
            tx.execute(
                "INSERT INTO photos (rel_path, rel_dir, file_name, size, mtime_ms, file_key, thumb_key,
                                     kind, format, width, height, scan_version, analysis)
                 VALUES (?1, ?2, ?3, 1, 1, 'fk', 'tk', 'image', 'jpeg', 10, 10, 1, '{}')",
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

fn missing(album: &Album, id: i64) -> bool {
    album
        .db()
        .read(move |c| {
            Ok(
                c.query_row("SELECT missing FROM photos WHERE id = ?1", [id], |r| {
                    r.get::<_, bool>(0)
                })
                .unwrap(),
            )
        })
        .unwrap()
}

fn on_disk(p: &Path) -> bool {
    p.exists()
}

#[test]
fn resolve_keeps_order_drops_duplicates_and_missing_files() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let overlay = Mutex::new(Overlay::default());
    let a = row(&album, "a.jpg", "");
    let b = row(&album, "sub/b.jpg", "edit = '{\"rotate\":90}'");
    let gone = row(&album, "gone.jpg", "");
    std::fs::remove_file(tmp.path().join("gone.jpg")).unwrap();

    let t = resolve(&album, &overlay, &[b, a, b, gone, 9999], LIMIT, &on_disk).unwrap();
    let ids: Vec<i64> = t.photos.iter().map(|t| t.photo.id).collect();
    assert_eq!(ids, vec![b, a]);
    assert_eq!(t.photos[0].name, "b.jpg");
    assert!(t.photos[0].edited);
    assert!(!t.photos[1].edited);
    assert!(t.with_edits());
    assert_eq!((t.missing, t.dropped), (1, 0));
}

#[test]
fn resolve_answers_not_found_when_no_id_is_a_photo() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let overlay = Mutex::new(Overlay::default());
    let e = resolve(&album, &overlay, &[41, 42], LIMIT, &on_disk).unwrap_err();
    assert_eq!(e.code, ErrorCode::PhotoNotFound);
    assert_eq!(e.message, text::files::NOT_FOUND_MANY);
    let e = resolve_one(&album, &overlay, 41, &on_disk).unwrap_err();
    assert_eq!(e.message, text::files::NOT_FOUND_ONE);
}

#[test]
fn resolve_one_answers_file_missing_for_a_gone_file() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let overlay = Mutex::new(Overlay::default());
    let a = row(&album, "a.jpg", "");
    assert_eq!(
        resolve_one(&album, &overlay, a, &on_disk).unwrap().photo.id,
        a
    );
    let e = resolve_one(&album, &overlay, a, &|_| false).unwrap_err();
    assert_eq!(e.code, ErrorCode::FileMissing);
}

#[test]
fn limit_keeps_the_first_and_counts_the_rest() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let overlay = Mutex::new(Overlay::default());
    let ids: Vec<i64> = (0..12)
        .map(|i| row(&album, &format!("p{i}.jpg"), ""))
        .collect();
    let mut sent = ids.clone();
    sent.push(ids[0]); // a duplicate counts once
    let t = resolve(&album, &overlay, &sent, 10, &on_disk).unwrap();
    assert_eq!(t.photos.len(), 10);
    assert_eq!(t.dropped, 2);
    assert_eq!(t.photos[9].photo.id, ids[9]);
}

#[test]
fn trash_marks_moved_rows_missing_and_reports_refused_files() {
    let tmp = tempfile::tempdir().unwrap();
    let bin_dir = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let overlay = Mutex::new(Overlay::default());
    let a = row(&album, "a.jpg", "notes = 'kept', star = 1");
    let b = row(&album, "b.jpg", "");
    let t = resolve(&album, &overlay, &[a, b], LIMIT, &on_disk).unwrap();
    let before = album.revision();

    let asked = RefCell::new(Vec::<PathBuf>::new());
    let bin = |p: &Path| -> Result<(), String> {
        asked.borrow_mut().push(p.to_owned());
        if p.ends_with("b.jpg") {
            return Err("in use".into());
        }
        std::fs::rename(p, bin_dir.path().join(p.file_name().unwrap())).map_err(|e| e.to_string())
    };
    let out = trash(&album, &t.photos, &bin).unwrap();

    assert_eq!(asked.borrow().len(), 2, "each file is tried once");
    assert_eq!(out.trashed, vec![a]);
    assert_eq!(
        out.failed,
        vec![FileFailure {
            id: b,
            name: "b.jpg".into(),
            message: "in use".into()
        }]
    );
    assert!(!out.cancelled);
    assert!(out.revision > before);
    assert!(missing(&album, a));
    assert!(!missing(&album, b));
    assert!(tmp.path().join("b.jpg").exists());
    // The row keeps its typed info.
    let notes: String = album
        .db()
        .read(move |c| {
            Ok(
                c.query_row("SELECT notes FROM photos WHERE id = ?1", [a], |r| r.get(0))
                    .unwrap(),
            )
        })
        .unwrap();
    assert_eq!(notes, "kept");
    // It no longer resolves.
    assert!(resolve(&album, &overlay, &[a], LIMIT, &on_disk).is_err());
}

#[test]
fn nothing_moved_changes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let overlay = Mutex::new(Overlay::default());
    let a = row(&album, "a.jpg", "");
    let t = resolve(&album, &overlay, &[a], LIMIT, &on_disk).unwrap();
    let before = album.revision();
    let out = trash(&album, &t.photos, &|_| Err("no".into())).unwrap();
    assert!(out.trashed.is_empty());
    assert_eq!(out.revision, before);
    assert!(!missing(&album, a));
}

#[test]
fn trash_refuses_a_view_only_album_before_touching_files() {
    let tmp = tempfile::tempdir().unwrap();
    {
        let album = album(tmp.path());
        row(&album, "a.jpg", "");
    }
    let album = Album::open(
        tmp.path(),
        &OpenOptions {
            view_only: true,
            ..OpenOptions::default()
        },
    )
    .unwrap();
    let overlay = Mutex::new(Overlay::default());
    let t = resolve(&album, &overlay, &[1], LIMIT, &on_disk).unwrap();
    let e = trash(&album, &t.photos, &|_| panic!("no file is touched")).unwrap_err();
    assert_eq!(e.code, ErrorCode::ViewOnly);
}

#[test]
fn confirm_wording_says_it_for_one_and_them_for_several() {
    let bin = text::files::bin();
    assert_eq!(
        text::files::trash_message(1, "x.jpg"),
        format!("Move \"x.jpg\" to the {bin}?")
    );
    assert_eq!(
        text::files::trash_message(3, "x.jpg"),
        format!("Move 3 photos to the {bin}?")
    );
    assert_eq!(
        text::files::trash_detail(1),
        format!("You can restore it from the {bin}.")
    );
    assert_eq!(
        text::files::trash_detail(2),
        format!("You can restore them from the {bin}.")
    );
}

#[cfg(windows)]
#[test]
fn a_network_share_has_no_bin() {
    assert!(no_bin_reason(Path::new(r"\\server\share\a.jpg")).is_some());
    let here = std::env::temp_dir().join("a.jpg");
    assert!(
        no_bin_reason(&here).is_none(),
        "the temp folder is on a fixed drive"
    );
}

#[test]
fn fit_pixels_keeps_the_shape_under_the_limit() {
    assert_eq!(fit_pixels(4000, 3000, MAX_COPY_PIXELS), None);
    let (w, h) = fit_pixels(10_000, 8_000, MAX_COPY_PIXELS).unwrap();
    assert!(u64::from(w) * u64::from(h) <= MAX_COPY_PIXELS);
    assert!((f64::from(w) / f64::from(h) - 1.25).abs() < 0.001);
}

#[test]
fn a_prepared_drag_is_used_for_the_same_ids_in_order_within_the_time() {
    let cache = DragCache::<u32>::default();
    let t0 = Instant::now();
    cache.put(vec![1, 2], t0, 7);
    assert_eq!(cache.take(&[2, 1], t0), None, "another order");
    assert_eq!(cache.take(&[1, 2], t0 + Duration::from_secs(10)), Some(7));
    assert_eq!(cache.take(&[1, 2], t0), None, "used once");
    cache.put(vec![1, 2], t0, 8);
    assert_eq!(
        cache.take(&[1, 2], t0 + Duration::from_millis(10_001)),
        None,
        "too old"
    );
}

#[test]
fn capabilities_name_what_this_platform_does() {
    let c = Capabilities::current();
    if cfg!(windows) {
        assert!(c.drag_out);
        assert_eq!(c.copy_files, CopyAs::Files);
    } else {
        assert!(!c.drag_out);
        assert_eq!(c.copy_files, CopyAs::Paths);
    }
    assert_eq!(
        serde_json::to_value(CopiedFiles {
            copied: 1,
            dropped: 0,
            copied_as: CopyAs::Paths,
            with_edits: false
        })
        .unwrap()["as"],
        "paths"
    );
}

#[test]
fn drag_icon_is_at_most_128_and_never_enlarged() {
    let encode = |w, h| {
        let mut out = std::io::Cursor::new(Vec::new());
        image::RgbImage::new(w, h)
            .write_to(&mut out, image::ImageFormat::Jpeg)
            .unwrap();
        out.into_inner()
    };
    let size = |png: Vec<u8>| {
        let img = image::load_from_memory(&png).unwrap();
        (img.width(), img.height())
    };
    assert_eq!(size(drag_icon(Some(&encode(512, 256)))), (128, 64));
    assert_eq!(size(drag_icon(Some(&encode(100, 50)))), (100, 50));
    assert_eq!(size(drag_icon(None)), (48, 48));
    assert_eq!(size(drag_icon(Some(b"not a picture"))), (48, 48));
}
