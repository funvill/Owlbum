//! The photo commands through Tauri's mock runtime.

mod common;

use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use common::invoke;
use image::{Rgb, RgbImage};
use owlbum_core::album::{Album, OpenOptions};
use owlbum_core::appdata::AppData;
use owlbum_core::info::detail::PhotoDetail;
use owlbum_core::info::histogram::Histogram;
use owlbum_core::media::MediaKind;
use owlbum_core::scan::ScanProgress;
use owlbum_lib::session::Session;
use serde_json::json;
use tauri::test::{MockRuntime, mock_builder};
use tauri::{Listener, Manager};

struct Fixture {
    app: common::TestApp,
    window: tauri::WebviewWindow<MockRuntime>,
    /// Last, so it is dropped last: on Windows the folder cannot be deleted
    /// while a database in it is still open.
    tmp: tempfile::TempDir,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let data = AppData::open(&tmp.path().join("data")).unwrap();
    let (app, window) = common::window(owlbum_lib::with_app_data(
        owlbum_lib::setup(mock_builder()),
        data,
    ));
    Fixture { app, window, tmp }
}

impl Fixture {
    /// Opens the album at `root` and waits for its first scan.
    fn open(&self, root: &Path) {
        let (send, receive) = mpsc::channel();
        self.app.listen_any("scan_progress", move |event| {
            let p: ScanProgress = serde_json::from_str(event.payload()).unwrap();
            if p.finished {
                let _ = send.send(());
            }
        });
        let album = Album::open(root, &OpenOptions::default()).unwrap();
        owlbum_lib::session::open_album(self.app.handle(), album);
        receive
            .recv_timeout(Duration::from_secs(60))
            .expect("the scan finishes");
    }

    /// The id of the photo at `rel_path`.
    fn id_of(&self, rel_path: &str) -> i64 {
        let scanner = self.app.state::<Session>().scanner().unwrap();
        let rel_path = rel_path.to_owned();
        scanner
            .album()
            .db()
            .read(move |c| {
                Ok(c.query_row(
                    "SELECT id FROM photos WHERE rel_path = ?1",
                    [&rel_path],
                    |r| r.get(0),
                )
                .unwrap())
            })
            .unwrap()
    }

    fn detail(&self, id: i64) -> Result<PhotoDetail, serde_json::Value> {
        invoke(&self.window, "photo_detail", json!({ "id": id })).map(|r| r.deserialize().unwrap())
    }

    fn histogram(&self, id: i64) -> Result<Histogram, serde_json::Value> {
        invoke(&self.window, "photo_histogram", json!({ "id": id }))
            .map(|r| r.deserialize().unwrap())
    }
}

/// A folder holding one pure red PNG.
fn red_album(root: &Path) {
    std::fs::create_dir_all(root).unwrap();
    let red = RgbImage::from_pixel(300, 200, Rgb([255, 0, 0]));
    std::fs::write(root.join("red.png"), owlbum_fixtures::encode::png_rgb(&red)).unwrap();
}

#[test]
fn without_an_album_the_photo_commands_answer_no_album() {
    let f = fixture();
    for command in ["photo_detail", "photo_histogram"] {
        let err = invoke(&f.window, command, json!({ "id": 1 })).unwrap_err();
        assert_eq!(err["code"], "no_album", "{command}");
    }
}

#[test]
fn a_trip_photo_answers_its_details() {
    let f = fixture();
    let fixture = owlbum_fixtures::trip(&f.tmp.path().join("trip")).unwrap();
    f.open(&fixture.root);
    let id = f.id_of("day1/lake.jpg");
    let d = f.detail(id).unwrap();
    assert_eq!(d.id, id);
    assert_eq!(d.file_name, "lake.jpg");
    assert_eq!(d.rel_dir, "day1");
    assert_eq!(d.kind, MediaKind::Image);
    assert_eq!(d.format, "jpeg");
    assert_eq!((d.width, d.height), (640, 480));
    assert_eq!(d.camera.camera.as_deref(), Some("Owlbum Fixture One"));
    assert_eq!(d.camera.iso, Some(100));
    // The lake is near Vancouver.
    assert_eq!(d.place.country.as_deref(), Some("Canada"));
    assert!(d.date.effective.is_some());
    // The scan finished, so the analysis is there.
    assert!(d.clipping.is_some());

    let err = f.detail(987_654).unwrap_err();
    assert_eq!(err["code"], "photo_not_found");
    let err = f.histogram(987_654).unwrap_err();
    assert_eq!(err["code"], "photo_not_found");
}

#[test]
fn a_pure_red_photo_puts_every_red_count_in_the_last_bin() {
    let f = fixture();
    let root = f.tmp.path().join("red");
    red_album(&root);
    f.open(&root);
    let id = f.id_of("red.png");
    let h = f.histogram(id).unwrap();
    // 300 × 200 counted at 256 × 171.
    let n: u32 = h.r.iter().sum();
    assert_eq!(n, 256 * 171);
    assert_eq!(h.l.len(), 64);
    // The thumbnail is a JPEG, which may move a level by one or two.
    assert_eq!(h.r[63] + h.r[62], n);
    assert_eq!(h.g[0] + h.g[1], n);
    assert_eq!(h.b[0] + h.b[1], n);
}

#[test]
fn an_edited_photos_histogram_is_the_files() {
    let f = fixture();
    let root = f.tmp.path().join("red");
    red_album(&root);
    f.open(&root);
    let id = f.id_of("red.png");
    let before = f.histogram(id).unwrap();
    let scanner = f.app.state::<Session>().scanner().unwrap();
    scanner
        .album()
        .write_photos(move |tx, touched| {
            tx.execute(
                "UPDATE photos SET edit = '{\"exposure\":2.0}' WHERE id = ?1",
                [id],
            )
            .unwrap();
            touched.photo(id);
            Ok(())
        })
        .unwrap();
    assert_eq!(f.histogram(id).unwrap(), before);
}

#[test]
fn photo_set_info_saves_clears_and_tells_the_page() {
    let f = fixture();
    let fixture = owlbum_fixtures::trip(&f.tmp.path().join("trip")).unwrap();
    f.open(&fixture.root);
    let id = f.id_of("day1/lake.jpg");
    let (send, heard) = mpsc::channel::<serde_json::Value>();
    f.app.listen_any("album_changed", move |event| {
        let _ = send.send(serde_json::from_str(event.payload()).unwrap());
    });

    let set = |patch: serde_json::Value| {
        invoke(
            &f.window,
            "photo_set_info",
            json!({ "ids": [id], "patch": patch }),
        )
    };
    let r: serde_json::Value =
        set(json!({ "notes": "a lake", "description": "Calm", "people": ["Ann"] }))
            .unwrap()
            .deserialize()
            .unwrap();
    assert_eq!(r["changed"], json!([id]));
    let event = loop {
        let e = heard
            .recv_timeout(Duration::from_secs(10))
            .expect("album_changed is sent");
        if e["reason"] == "info" {
            break e;
        }
    };
    assert_eq!(event["revision"], r["revision"]);

    // An absent field is left alone; a null clears.
    set(json!({ "notes": null })).unwrap();
    let d = f.detail(id).unwrap();
    assert_eq!(d.notes, None);
    assert_eq!(d.description.as_deref(), Some("Calm"));
    assert_eq!(d.people, ["Ann"]);

    let people: serde_json::Value = invoke(&f.window, "album_people", json!({}))
        .unwrap()
        .deserialize()
        .unwrap();
    assert_eq!(people, json!([{ "name": "Ann", "count": 1 }]));

    let err = set(json!({ "date_override": "2024-02-30" })).unwrap_err();
    assert_eq!(err["code"], "invalid_date");
    let err = invoke(
        &f.window,
        "photo_set_info",
        json!({ "ids": [987_654], "patch": { "notes": "x" } }),
    )
    .unwrap_err();
    assert_eq!(err["code"], "photo_not_found");
}
