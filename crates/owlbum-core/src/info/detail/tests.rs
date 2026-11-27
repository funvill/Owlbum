use std::path::Path;
use std::sync::Mutex;

use super::*;
use crate::album::{Album, OpenOptions, sqlite_error};
use crate::error::ErrorCode;
use crate::media::MediaKind;
use crate::scan::memory::Overlay;
use crate::text;

fn album(dir: &Path) -> Album {
    Album::open(dir, &OpenOptions::default()).unwrap()
}

/// Runs SQL on the album through the writer, so the album index follows,
/// and answers the last inserted id.
fn write(album: &Album, sql: &str) -> i64 {
    let sql = sql.to_owned();
    album
        .write_photos(move |tx, touched| {
            tx.execute_batch(&sql)
                .map_err(sqlite_error(text::album::WRITING_DATABASE))?;
            let id = tx.last_insert_rowid();
            touched.photo(id);
            Ok(id)
        })
        .unwrap()
}

/// A JPEG in `2024/Trip` with a camera date, GPS in Paris, camera details
/// and an analysis.
const PARIS: &str = r#"
    INSERT INTO photos (rel_path, rel_dir, file_name, size, mtime_ms, file_key, thumb_key, kind, format,
                        width, height, scan_version, captured_at, gps_lat, gps_lon, camera, exif, analysis)
    VALUES ('2024/Trip/IMG_0042.jpg', '2024/Trip', 'IMG_0042.jpg', 3250585, 1, 'fk', 'tk42', 'image', 'jpeg',
            4032, 3024, 1, '2024-06-12T14:03:00', 48.85837, 2.29448, 'Owlbum Fixture One',
            '{"lens":"Fixture 24-70mm F2.8","focal_length":35.0,"focal_length_35":52.0,"aperture":5.6,"exposure_time":0.004,"iso":100,"exposure_bias":-0.7,"flash":false}',
            '{"sharpness":1.0,"mean_luma":0.5,"clipped_shadows":0.004,"clipped_highlights":0.023}');
"#;

fn detail(album: &Album, id: i64) -> AppResult<PhotoDetail> {
    photo_detail(album, &Mutex::new(Overlay::default()), id)
}

#[test]
fn a_photo_answers_its_facts_camera_and_clipping() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let id = write(&album, PARIS);
    let d = detail(&album, id).unwrap();

    assert_eq!(d.id, id);
    assert_eq!(d.rel_path, "2024/Trip/IMG_0042.jpg");
    assert_eq!(d.rel_dir, "2024/Trip");
    assert_eq!(d.file_name, "IMG_0042.jpg");
    assert_eq!(
        Path::new(&d.abs_path),
        tmp.path().join("2024").join("Trip").join("IMG_0042.jpg")
    );
    assert_eq!(d.kind, MediaKind::Image);
    assert_eq!(d.format, "jpeg");
    assert_eq!((d.width, d.height, d.size), (4032, 3024, 3_250_585));
    assert_eq!(d.duration_ms, None);
    assert_eq!(d.thumb_key, "tk42");

    assert_eq!(
        d.camera,
        Camera {
            camera: Some("Owlbum Fixture One".into()),
            lens: Some("Fixture 24-70mm F2.8".into()),
            focal_mm: Some(35.0),
            focal35_mm: Some(52.0),
            aperture: Some(5.6),
            shutter_s: Some(0.004),
            iso: Some(100),
            bias_ev: Some(-0.7),
            flash: Some(false),
        }
    );
    assert_eq!(
        d.clipping,
        Some(Clipping {
            shadows: 0.004,
            highlights: 0.023
        })
    );
    assert!(!d.star && !d.hidden);
    assert!(d.people.is_empty());
    assert_eq!((d.description.as_deref(), d.notes.as_deref()), (None, None));
    assert_eq!(d.edit_plan, None);
    assert_eq!(d.edit, None);
    assert_eq!(d.tags, None);
}

#[test]
fn a_stored_edit_comes_with_its_plan() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let id = write(&album, PARIS);
    let edit = crate::edit::PhotoEdit {
        rotate: 90,
        ..crate::edit::IDENTITY
    };
    crate::edit::store::set(&album, id, Some(edit)).unwrap();
    let d = detail(&album, id).unwrap();
    assert_eq!(d.edit, Some(edit));
    assert_eq!(d.edit_plan.map(|p| p.out.w), Some(3024));
}

#[test]
fn the_revision_is_the_albums_when_the_detail_was_read() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let id = write(&album, PARIS);
    let before = album.revision();
    let d = detail(&album, id).unwrap();
    assert_eq!(d.revision, before);
    // A change raises the revision when it tells the page.
    write(
        &album,
        &format!("UPDATE photos SET star = 1 WHERE id = {id};"),
    );
    let next = album.next_revision();
    let d = detail(&album, id).unwrap();
    assert_eq!(d.revision, next);
    assert!(next > before);
    assert!(d.star);
}

#[test]
fn the_camera_date_is_effective_until_a_date_is_typed() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let id = write(&album, PARIS);
    let d = detail(&album, id).unwrap();
    assert_eq!(
        d.date,
        DateInfo {
            effective: Some("2024-06-12T14:03:00".into()),
            camera: Some("2024-06-12T14:03:00".into()),
            typed: None,
        }
    );

    write(
        &album,
        &format!("UPDATE photos SET date_override = '1999-12-31T23:59:00' WHERE id = {id};"),
    );
    let d = detail(&album, id).unwrap();
    assert_eq!(
        d.date,
        DateInfo {
            effective: Some("1999-12-31T23:59:00".into()),
            camera: Some("2024-06-12T14:03:00".into()),
            typed: Some("1999-12-31T23:59:00".into()),
        }
    );
}

#[test]
fn the_gps_place_is_effective_until_a_place_is_typed() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let id = write(&album, PARIS);
    let d = detail(&album, id).unwrap();
    assert_eq!(
        d.place,
        Place {
            lat: Some(48.85837),
            lon: Some(2.29448),
            gps: Some(LatLon {
                lat: 48.85837,
                lon: 2.29448
            }),
            typed: false,
            name: None,
            country: Some("France".into()),
        }
    );

    // A typed place in Rome wins over the camera's.
    write(
        &album,
        &format!(
            "UPDATE photos SET loc_lat = 41.9029, loc_lon = 12.4534, loc_name = 'Rome' WHERE id = {id};"
        ),
    );
    let d = detail(&album, id).unwrap();
    assert_eq!((d.place.lat, d.place.lon), (Some(41.9029), Some(12.4534)));
    assert!(d.place.typed);
    assert_eq!(d.place.name.as_deref(), Some("Rome"));
    assert_eq!(d.place.country.as_deref(), Some("Vatican City"));
    assert_eq!(d.place.gps.map(|g| g.lat), Some(48.85837));

    // Mid-Atlantic: no country.
    write(
        &album,
        &format!(
            "UPDATE photos SET loc_lat = 30.0, loc_lon = -40.0, loc_name = NULL WHERE id = {id};"
        ),
    );
    let d = detail(&album, id).unwrap();
    assert_eq!(d.place.country, None);
}

#[test]
fn a_photo_with_no_place_has_none() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let id = write(
        &album,
        "INSERT INTO photos (rel_path, rel_dir, file_name, size, mtime_ms, file_key, thumb_key, kind, format, width, height, scan_version)
         VALUES ('a.png', '', 'a.png', 10, 1, 'fk', 'tka', 'image', 'png', 10, 20, 1);",
    );
    let d = detail(&album, id).unwrap();
    assert_eq!(d.rel_dir, "");
    assert_eq!(
        d.place,
        Place {
            lat: None,
            lon: None,
            gps: None,
            typed: false,
            name: None,
            country: None,
        }
    );
    assert_eq!(d.date.effective, None);
    assert_eq!(d.camera, Camera::default());
    // Waiting for the pixels pass: no clipping yet.
    assert_eq!(d.clipping, None);
}

#[test]
fn a_video_has_its_length() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let id = write(
        &album,
        "INSERT INTO photos (rel_path, rel_dir, file_name, size, mtime_ms, file_key, thumb_key, kind, format, width, height, duration_ms, scan_version)
         VALUES ('clip.mp4', '', 'clip.mp4', 10, 1, 'fk', 'tkv', 'video', 'mp4', 1920, 1080, 83500, 1);",
    );
    let d = detail(&album, id).unwrap();
    assert_eq!(d.kind, MediaKind::Video);
    assert_eq!(d.format, "mp4");
    assert_eq!(d.duration_ms, Some(83_500));
}

#[test]
fn typed_info_and_people_are_read() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let id = write(&album, PARIS);
    write(
        &album,
        &format!(
            "UPDATE photos SET star = 1, hidden = 1, description = 'Tower', notes = 'Line one\nLine two' WHERE id = {id};
             INSERT INTO people (name) VALUES ('bob'), ('Ann');
             INSERT INTO photo_people (photo_id, person_id) SELECT {id}, id FROM people;"
        ),
    );
    let d = detail(&album, id).unwrap();
    assert!(d.star && d.hidden);
    assert_eq!(d.description.as_deref(), Some("Tower"));
    assert_eq!(d.notes.as_deref(), Some("Line one\nLine two"));
    // In name order, ignoring case.
    assert_eq!(d.people, ["Ann", "bob"]);
}

#[test]
fn photos_that_are_not_shown_answer_photo_not_found() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let id = write(&album, PARIS);
    assert_eq!(
        detail(&album, 987_654).unwrap_err().code,
        ErrorCode::PhotoNotFound
    );

    let broken = write(
        &album,
        "INSERT INTO photos (rel_path, rel_dir, file_name, size, mtime_ms, file_key, thumb_key, kind, format, width, height, scan_version, scan_error)
         VALUES ('bad.jpg', '', 'bad.jpg', 10, 1, 'fk', 'tkb', 'image', 'jpeg', 0, 0, 1, 'decode: This file could not be read.');",
    );
    assert_eq!(
        detail(&album, broken).unwrap_err().code,
        ErrorCode::PhotoNotFound
    );

    write(
        &album,
        &format!("UPDATE photos SET missing = 1 WHERE id = {id};"),
    );
    let err = detail(&album, id).unwrap_err();
    assert_eq!(err.code, ErrorCode::PhotoNotFound);
    assert_eq!(err.message, text::photo::NOT_FOUND);
}

#[test]
fn a_photo_outside_the_scope_answers_photo_not_found() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("2024").join("Trip")).unwrap();
    std::fs::create_dir_all(tmp.path().join("2023")).unwrap();
    let root = album(tmp.path());
    let inside = write(&root, PARIS);
    let outside = write(
        &root,
        "INSERT INTO photos (rel_path, rel_dir, file_name, size, mtime_ms, file_key, thumb_key, kind, format, width, height, scan_version)
         VALUES ('2023/b.jpg', '2023', 'b.jpg', 10, 1, 'fk', 'tkb', 'image', 'jpeg', 10, 10, 1);",
    );
    drop(root);
    let scoped = Album::open(&tmp.path().join("2024"), &OpenOptions::default()).unwrap();
    assert_eq!(scoped.location().scope, "2024");
    assert!(detail(&scoped, inside).is_ok());
    assert_eq!(
        detail(&scoped, outside).unwrap_err().code,
        ErrorCode::PhotoNotFound
    );
}

#[test]
fn camera_details_ignore_an_exif_json_that_does_not_parse() {
    assert_eq!(camera(None, Some("not json")), Camera::default());
    assert_eq!(
        camera(Some("Cam".into()), Some(r#"{"flash":true,"iso":3200}"#)),
        Camera {
            camera: Some("Cam".into()),
            iso: Some(3200),
            flash: Some(true),
            ..Camera::default()
        }
    );
    assert_eq!(clipping(Some("{}")), None);
    assert_eq!(clipping(Some("nonsense")), None);
}
