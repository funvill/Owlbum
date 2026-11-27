//! The file commands through Tauri's mock runtime, with a fake desktop
//! that records what it was asked and moves "trashed" files to a folder.

mod common;

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::invoke;
use image::{Rgb, RgbImage};
use owlbum_core::album::{Album, OpenOptions};
use owlbum_core::appdata::AppData;
use owlbum_core::files::{CopiedFiles, CopiedImage, CopiedPaths, Trashed};
use owlbum_core::scan::ScanProgress;
use owlbum_lib::commands::file::{Desktop, DesktopShell};
use owlbum_lib::session::Session;
use serde_json::json;
use tauri::test::{MockRuntime, mock_builder};
use tauri::{Listener, Manager};

#[derive(Default)]
struct Fake {
    answer: Mutex<bool>,
    asked: Mutex<Vec<String>>,
    text: Mutex<Option<String>>,
    image: Mutex<Option<(u32, u32)>>,
    files: Mutex<Vec<PathBuf>>,
    revealed: Mutex<Option<PathBuf>>,
    bin: PathBuf,
}

impl Desktop for Fake {
    fn confirm_trash(&self, message: &str, _detail: &str) -> bool {
        self.asked.lock().unwrap().push(message.to_owned());
        *self.answer.lock().unwrap()
    }
    fn trash(&self, path: &Path) -> Result<(), String> {
        std::fs::rename(path, self.bin.join(path.file_name().unwrap())).map_err(|e| e.to_string())
    }
    fn reveal(&self, path: &Path) -> Result<(), String> {
        *self.revealed.lock().unwrap() = Some(path.to_owned());
        Ok(())
    }
    fn copy_text(&self, text: &str) -> Result<(), String> {
        *self.text.lock().unwrap() = Some(text.to_owned());
        Ok(())
    }
    fn copy_image(&self, width: u32, height: u32, rgba: Vec<u8>) -> Result<(), String> {
        assert_eq!(rgba.len(), (width * height * 4) as usize);
        *self.image.lock().unwrap() = Some((width, height));
        Ok(())
    }
    fn copy_files(&self, paths: &[PathBuf]) -> Result<(), String> {
        *self.files.lock().unwrap() = paths.to_vec();
        Ok(())
    }
    fn start_drag(&self, _paths: Vec<PathBuf>, _icon: Vec<u8>) -> Result<(), String> {
        Err("refused".into())
    }
}

struct Fixture {
    app: common::TestApp,
    window: tauri::WebviewWindow<MockRuntime>,
    fake: Arc<Fake>,
    tmp: tempfile::TempDir,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let data = AppData::open(&tmp.path().join("data")).unwrap();
    let (app, window) = common::window(owlbum_lib::with_app_data(
        owlbum_lib::setup(mock_builder()),
        data,
    ));
    let bin = tmp.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let fake = Arc::new(Fake {
        bin,
        ..Fake::default()
    });
    app.manage(DesktopShell(fake.clone()));
    Fixture {
        app,
        window,
        fake,
        tmp,
    }
}

/// Closes the album before the folders go, so no scan or thumbnail work
/// still holds a file in them when the temporary folder is deleted.
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = invoke(&self.window, "album_close", json!({}));
    }
}

impl Fixture {
    /// Makes an album of three PNGs, opens it and waits for its scan.
    fn open(&self) -> PathBuf {
        self.open_with(&[])
    }

    /// [`Fixture::open`], with `extra` files (name, bytes) beside the PNGs.
    fn open_with(&self, extra: &[(&str, &[u8])]) -> PathBuf {
        let root = self.tmp.path().join("album");
        std::fs::create_dir_all(&root).unwrap();
        for (name, bytes) in extra {
            std::fs::write(root.join(name), bytes).unwrap();
        }
        for (i, name) in ["a.png", "b.png", "c.png"].iter().enumerate() {
            let img = RgbImage::from_pixel(30 + i as u32, 20, Rgb([200, 10, 10]));
            std::fs::write(root.join(name), owlbum_fixtures::encode::png_rgb(&img)).unwrap();
        }
        let (send, receive) = mpsc::channel();
        self.app.listen_any("scan_progress", move |event| {
            let p: ScanProgress = serde_json::from_str(event.payload()).unwrap();
            if p.finished {
                let _ = send.send(());
            }
        });
        let album = Album::open(&root, &OpenOptions::default()).unwrap();
        owlbum_lib::session::open_album(self.app.handle(), album);
        receive
            .recv_timeout(Duration::from_secs(60))
            .expect("the scan finishes");
        root
    }

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

    fn call<T: serde::de::DeserializeOwned>(
        &self,
        cmd: &str,
        args: serde_json::Value,
    ) -> Result<T, serde_json::Value> {
        invoke(&self.window, cmd, args).map(|r| r.deserialize().unwrap())
    }
}

#[test]
fn trash_asks_first_and_a_cancel_changes_nothing() {
    let f = fixture();
    let root = f.open();
    let a = f.id_of("a.png");
    let out: Trashed = f.call("file_trash", json!({ "photo_ids": [a] })).unwrap();
    assert!(out.cancelled);
    assert!(out.trashed.is_empty());
    assert!(root.join("a.png").exists());
    let asked = f.fake.asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 1);
    assert!(asked[0].starts_with("Move \"a.png\" to the "), "{asked:?}");
}

#[test]
fn trash_moves_the_files_and_marks_the_rows_missing() {
    let f = fixture();
    let root = f.open();
    *f.fake.answer.lock().unwrap() = true;
    let (a, b) = (f.id_of("a.png"), f.id_of("b.png"));
    let (send, receive) = mpsc::channel();
    f.app.listen_any("album_changed", move |event| {
        let _ = send.send(event.payload().to_owned());
    });
    let out: Trashed = f
        .call("file_trash", json!({ "photo_ids": [b, a] }))
        .unwrap();
    assert_eq!(out.trashed, vec![b, a]);
    assert!(out.failed.is_empty());
    assert!(!root.join("a.png").exists());
    assert!(f.tmp.path().join("bin/a.png").exists());
    assert!(f.fake.asked.lock().unwrap()[0].starts_with("Move 2 photos"));
    let changed = receive.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(changed.contains("\"trashed\""), "{changed}");
    // Gone from the album's photos: another call cannot name them.
    let err = f
        .call::<Trashed>("file_trash", json!({ "photo_ids": [a] }))
        .unwrap_err();
    assert_eq!(err["code"], "photo_not_found");
}

#[test]
fn copy_paths_copies_every_path_in_order() {
    let f = fixture();
    let root = f.open();
    let (a, c) = (f.id_of("a.png"), f.id_of("c.png"));
    let out: CopiedPaths = f
        .call("file_copy_paths", json!({ "photo_ids": [c, a, c] }))
        .unwrap();
    assert_eq!((out.copied, out.dropped), (2, 0));
    let text = f.fake.text.lock().unwrap().clone().unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines,
        vec![
            root.join("c.png").display().to_string(),
            root.join("a.png").display().to_string()
        ]
    );
}

#[test]
fn copy_image_copies_the_upright_picture() {
    let f = fixture();
    f.open();
    let b = f.id_of("b.png");
    let out: CopiedImage = f.call("file_copy_image", json!({ "photo_id": b })).unwrap();
    assert!(!out.scaled);
    assert_eq!(*f.fake.image.lock().unwrap(), Some((31, 20)));
}

#[test]
fn copy_files_copies_the_files_or_their_paths() {
    let f = fixture();
    let root = f.open();
    let (a, b) = (f.id_of("a.png"), f.id_of("b.png"));
    let out: CopiedFiles = f
        .call("file_copy_files", json!({ "photo_ids": [a, b] }))
        .unwrap();
    assert_eq!(out.copied, 2);
    if cfg!(windows) {
        assert_eq!(
            *f.fake.files.lock().unwrap(),
            vec![root.join("a.png"), root.join("b.png")]
        );
    } else {
        assert!(f.fake.text.lock().unwrap().is_some());
    }
}

#[test]
fn reveal_names_the_file_and_a_gone_file_is_reported() {
    let f = fixture();
    let root = f.open();
    let a = f.id_of("a.png");
    f.call::<()>("file_reveal", json!({ "photo_id": a }))
        .unwrap();
    assert_eq!(
        f.fake.revealed.lock().unwrap().clone(),
        Some(root.join("a.png"))
    );
    std::fs::remove_file(root.join("a.png")).unwrap();
    let err = f
        .call::<()>("file_reveal", json!({ "photo_id": a }))
        .unwrap_err();
    assert_eq!(err["code"], "file_missing");
}

#[test]
fn a_refused_drag_is_reported() {
    let f = fixture();
    f.open();
    let a = f.id_of("a.png");
    f.call::<()>("file_prepare_drag", json!({ "photo_ids": [a] }))
        .unwrap();
    let err = f
        .call::<serde_json::Value>("file_start_drag", json!({ "photo_ids": [a] }))
        .unwrap_err();
    if cfg!(windows) {
        assert_eq!(err["code"], "drag_failed");
        assert_eq!(err["message"], "Couldn't drag the photos: refused");
    } else {
        assert_eq!(err["code"], "not_supported");
    }
}

#[test]
fn copy_not_shown_copies_unsupported_then_unreadable_files() {
    let f = fixture();
    f.open_with(&[
        ("z.heic", b"not really a heic"),
        ("bad.jpg", b"these bytes are no picture at all"),
    ]);
    let copied: u32 = f.call("album_copy_not_shown", json!({})).unwrap();
    assert_eq!(copied, 2);
    let text = f.fake.text.lock().unwrap().clone().unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "z.heic	HEIC photos are not supported yet.");
    assert!(lines[1].starts_with("bad.jpg	"), "{text:?}");
}

#[test]
fn save_into_the_file_goes_through_the_bin_and_status_counts_from_the_database() {
    use owlbum_core::edit::save::{EditStatus, SavedEdits};
    let f = fixture();
    let root = f.open();
    let a = f.id_of("a.png");
    f.call::<serde_json::Value>(
        "edit_set",
        json!({ "photo_id": a, "edit": { "rotate": 90, "flip": false } }),
    )
    .unwrap();
    let status: EditStatus = f.call("edit_status", json!({})).unwrap();
    assert_eq!(
        (status.edited, status.waiting),
        (1, 0),
        "album mode waits for nothing"
    );
    let refused = f
        .call::<SavedEdits>("edit_save_all", json!({}))
        .unwrap_err();
    assert_eq!(refused["code"], "not_supported");

    f.call::<serde_json::Value>(
        "app_settings_set",
        json!({ "patch": { "edit_target": "file" } }),
    )
    .unwrap();
    let status: EditStatus = f.call("edit_status", json!({})).unwrap();
    assert_eq!(status.waiting, 1);

    let out: SavedEdits = f
        .call("edit_save_to_file", json!({ "photo_ids": [a] }))
        .unwrap();
    assert_eq!(out.saved, vec![a], "{out:?}");
    assert!(
        f.fake.bin.join("a.png").exists(),
        "the original is in the bin"
    );
    // The saved file is the turned picture: 20 × 30 instead of 30 × 20.
    let saved = image::open(root.join("a.png")).unwrap();
    assert_eq!((saved.width(), saved.height()), (20, 30));
    let status: EditStatus = f.call("edit_status", json!({})).unwrap();
    assert_eq!(status, EditStatus::default());
}

#[test]
fn the_page_answers_the_close_handshake() {
    let f = fixture();
    assert!(invoke(&f.window, "app_close_ready", json!({})).is_ok());
}
