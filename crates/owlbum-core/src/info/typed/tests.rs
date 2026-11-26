use std::path::Path;
use std::sync::Mutex;

use serde_json::json;

use super::*;
use crate::album::{Album, OpenOptions};
use crate::info::detail::{PhotoDetail, photo_detail};
use crate::scan::memory::Overlay;

fn album(dir: &Path) -> Album {
    Album::open(dir, &OpenOptions::default()).unwrap()
}

/// Adds a photo named `name` with a camera date and GPS, and answers its id.
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
            .map_err(failed())?;
            let id = tx.last_insert_rowid();
            touched.photo(id);
            Ok(id)
        })
        .unwrap()
}

fn detail(album: &Album, id: i64) -> PhotoDetail {
    photo_detail(album, &Mutex::new(Overlay::default()), id).unwrap()
}

fn patch(v: serde_json::Value) -> InfoPatch {
    serde_json::from_value(v).unwrap()
}

fn set(album: &Album, id: i64, v: serde_json::Value) -> AppResult<InfoChanged> {
    set_info(album, vec![id], patch(v))
}

#[test]
fn a_null_clears_and_an_absent_field_is_left_alone() {
    assert_eq!(patch(json!({})), InfoPatch::default());
    assert_eq!(patch(json!({ "notes": null })).notes, Some(None));
    assert_eq!(
        patch(json!({ "notes": "x" })).notes,
        Some(Some("x".to_owned()))
    );
}

#[test]
fn description_and_notes_are_trimmed_and_empty_text_is_null() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let id = photo(&album, "a.jpg");
    let r = set(
        &album,
        id,
        json!({ "description": "  At the tower ", "notes": "line 1\nline 2\n" }),
    )
    .unwrap();
    assert_eq!(r.changed, [id]);
    let d = detail(&album, id);
    assert_eq!(d.description.as_deref(), Some("At the tower"));
    assert_eq!(d.notes.as_deref(), Some("line 1\nline 2"));

    set(&album, id, json!({ "description": "   " })).unwrap();
    let stored: Option<String> = album
        .db()
        .read(move |c| {
            Ok(
                c.query_row("SELECT description FROM photos WHERE id = ?1", [id], |r| {
                    r.get(0)
                })
                .unwrap(),
            )
        })
        .unwrap();
    assert_eq!(stored, None, "never ''");
    assert_eq!(detail(&album, id).notes.as_deref(), Some("line 1\nline 2"));
}

#[test]
fn a_change_raises_the_revision_and_no_change_writes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let id = photo(&album, "a.jpg");
    let before = album.revision();
    let r = set(&album, id, json!({ "notes": "x" })).unwrap();
    assert!(r.revision > before);
    let again = set(&album, id, json!({ "notes": " x " })).unwrap();
    assert_eq!(again.changed, Vec::<i64>::new());
    assert_eq!(again.revision, r.revision);
}

#[test]
fn typed_text_is_found_by_search() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let id = photo(&album, "a.jpg");
    set(
        &album,
        id,
        json!({ "notes": "giraffe", "people": ["Zelda"] }),
    )
    .unwrap();
    let hits: i64 = album
        .db()
        .read(|c| {
            Ok(c.query_row(
                "SELECT count(*) FROM search WHERE search MATCH 'giraffe' AND search MATCH 'people:zelda'",
                [],
                |r| r.get(0),
            )
            .unwrap())
        })
        .unwrap();
    assert_eq!(hits, 1);
}

#[test]
fn each_date_form_is_accepted_or_refused() {
    let ok = [
        ("2024-06-12 14:03:05", "2024-06-12T14:03:05"),
        ("2024-06-12T14:03:05", "2024-06-12T14:03:05"),
        ("2024-06-12 14:03", "2024-06-12T14:03:00"),
        ("2024-06-12T14:03", "2024-06-12T14:03:00"),
        ("2024-06-12", "2024-06-12T12:00:00"),
        (" 2024-02-29 ", "2024-02-29T12:00:00"),
        ("1800-01-01", "1800-01-01T12:00:00"),
        ("2200-12-31 23:59:59", "2200-12-31T23:59:59"),
    ];
    for (typed, want) in ok {
        assert_eq!(parse_date(typed).as_deref(), Some(want), "{typed}");
    }
    let bad = [
        "2024-02-30",
        "2023-02-29",
        "1700-01-01",
        "2201-01-01",
        "2024-13-01",
        "2024-06-12 24:00",
        "2024-06-12 14:60",
        "2024-6-12",
        "12/06/2024",
        "2024-06-12 14",
        "",
        "yesterday",
    ];
    for typed in bad {
        assert_eq!(parse_date(typed), None, "{typed}");
    }
}

#[test]
fn a_typed_date_is_stored_and_one_equal_to_the_camera_clears_it() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let id = photo(&album, "a.jpg");
    set(&album, id, json!({ "date_override": "2020-01-02 03:04" })).unwrap();
    let d = detail(&album, id);
    assert_eq!(d.date.typed.as_deref(), Some("2020-01-02T03:04:00"));
    assert_eq!(d.date.effective.as_deref(), Some("2020-01-02T03:04:00"));

    set(
        &album,
        id,
        json!({ "date_override": "2024-06-12 14:03:00" }),
    )
    .unwrap();
    assert_eq!(detail(&album, id).date.typed, None);

    let e = set(&album, id, json!({ "date_override": "2024-02-30" })).unwrap_err();
    assert_eq!(e.code, ErrorCode::InvalidDate);
}

#[test]
fn location_rules() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let id = photo(&album, "a.jpg");

    set(
        &album,
        id,
        json!({ "location": { "lat": 51.5, "lon": -0.12, "name": " London " } }),
    )
    .unwrap();
    let d = detail(&album, id);
    assert!(d.place.typed);
    assert_eq!((d.place.lat, d.place.lon), (Some(51.5), Some(-0.12)));
    assert_eq!(d.place.name.as_deref(), Some("London"));

    // The camera's GPS to 5 decimals with no name clears the typed place.
    set(
        &album,
        id,
        json!({ "location": { "lat": 48.858371, "lon": 2.294481, "name": null } }),
    )
    .unwrap();
    let d = detail(&album, id);
    assert!(!d.place.typed);
    assert_eq!(d.place.name, None);

    // ... but with a name it is kept.
    set(
        &album,
        id,
        json!({ "location": { "lat": 48.85837, "lon": 2.29448, "name": "Tower" } }),
    )
    .unwrap();
    assert!(detail(&album, id).place.typed);

    set(&album, id, json!({ "location": null })).unwrap();
    assert!(!detail(&album, id).place.typed);

    for (lat, lon, msg) in [
        (91.0, 0.0, text::photo::LATITUDE_RANGE),
        (-90.5, 0.0, text::photo::LATITUDE_RANGE),
        (0.0, 180.5, text::photo::LONGITUDE_RANGE),
    ] {
        let e = set(
            &album,
            id,
            json!({ "location": { "lat": lat, "lon": lon, "name": null } }),
        )
        .unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidLocation);
        assert_eq!(e.message, msg);
    }
}

#[test]
fn people_are_cleaned_deduplicated_and_the_first_spelling_wins() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let a = photo(&album, "a.jpg");
    let b = photo(&album, "b.jpg");
    set(
        &album,
        a,
        json!({ "people": ["  Ann   Lee ", "ann lee", "", "Bob", "Émile"] }),
    )
    .unwrap();
    assert_eq!(detail(&album, a).people, ["Ann Lee", "Bob", "Émile"]);

    set(&album, b, json!({ "people": ["ANN LEE", "émile", "Cy"] })).unwrap();
    assert_eq!(detail(&album, b).people, ["Ann Lee", "Cy", "Émile"]);

    let long = "x".repeat(150);
    set(&album, b, json!({ "people": [long] })).unwrap();
    assert_eq!(detail(&album, b).people, ["x".repeat(MAX_NAME_CHARS)]);
}

#[test]
fn album_people_counts_orders_and_forgets_unused_names() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let a = photo(&album, "a.jpg");
    let b = photo(&album, "b.jpg");
    let c = photo(&album, "c.jpg");
    set(&album, a, json!({ "people": ["Bob", "Ann", "Pat 10"] })).unwrap();
    set(&album, b, json!({ "people": ["Bob", "Pat 9"] })).unwrap();
    set(&album, c, json!({ "people": ["Bob"] })).unwrap();
    let people = album_people(&album).unwrap();
    let got: Vec<(&str, u32)> = people.iter().map(|p| (p.name.as_str(), p.count)).collect();
    assert_eq!(got, [("Bob", 3), ("Ann", 1), ("Pat 9", 1), ("Pat 10", 1)]);

    set(&album, a, json!({ "people": null })).unwrap();
    let names: Vec<String> = album_people(&album)
        .unwrap()
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(names, ["Bob", "Pat 9"]);
    let rows: i64 = album
        .db()
        .read(|c| {
            Ok(c.query_row("SELECT count(*) FROM people", [], |r| r.get(0))
                .unwrap())
        })
        .unwrap();
    assert_eq!(rows, 2, "Ann and Pat 10 are deleted from people");

    // A missing photo's names are not counted.
    album
        .write_photos(move |tx, touched| {
            tx.execute("UPDATE photos SET missing = 1 WHERE id = ?1", [b])
                .map_err(failed())?;
            touched.photo(b);
            Ok(())
        })
        .unwrap();
    let got: Vec<(String, u32)> = album_people(&album)
        .unwrap()
        .into_iter()
        .map(|p| (p.name, p.count))
        .collect();
    assert_eq!(
        got,
        [("Bob".to_owned(), 1)],
        "Pat 9 is only on the missing photo"
    );
}

#[test]
fn an_unknown_or_missing_photo_answers_photo_not_found_and_writes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let a = photo(&album, "a.jpg");
    let e = set_info(&album, vec![a, 999], patch(json!({ "notes": "x" }))).unwrap_err();
    assert_eq!(e.code, ErrorCode::PhotoNotFound);
    assert_eq!(detail(&album, a).notes, None);
}

#[test]
fn every_change_answers_view_only_in_a_view_only_album() {
    let tmp = tempfile::tempdir().unwrap();
    let id = {
        let album = album(tmp.path());
        photo(&album, "a.jpg")
    };
    let album = Album::open(
        tmp.path(),
        &OpenOptions {
            view_only: true,
            ..OpenOptions::default()
        },
    )
    .unwrap();
    for p in [
        json!({ "notes": "x" }),
        json!({ "description": "x" }),
        json!({ "people": ["Ann"] }),
        json!({ "date_override": "2020-01-01" }),
        json!({ "location": null }),
    ] {
        let e = set(&album, id, p).unwrap_err();
        assert_eq!(e.code, ErrorCode::ViewOnly);
    }
    assert_eq!(detail(&album, id).notes, None);
}
