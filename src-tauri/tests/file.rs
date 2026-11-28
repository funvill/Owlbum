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
use owlbum_core::files::{CopiedFiles, CopiedImage, CopiedPaths, TrashQuestion, Trashed};
use owlbum_core::scan::ScanProgress;
use owlbum_lib::commands::file::{Desktop, DesktopShell};
use owlbum_lib::session::Session;
use serde_json::json;
use tauri::test::{MockRuntime, mock_builder};
use tauri::{Listener, Manager};

#[derive(Default)]
struct Fake {
    text: Mutex<Option<String>>,
    image: Mutex<Option<(u32, u32)>>,
    files: Mutex<Vec<PathBuf>>,
    revealed: Mutex<Option<PathBuf>>,
    bin: PathBuf,
}

impl Desktop for Fake {
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
fn trash_asks_first_and_moves_nothing_until_told() {
    let f = fixture();
    let root = f.open();
    let a = f.id_of("a.png");
    let q: TrashQuestion = f
        .call("file_trash_ask", json!({ "photo_ids": [a] }))
        .unwrap();
    assert_eq!(q.count, 1);
    assert!(q.message.starts_with("Move \"a.png\" to the "), "{q:?}");
    assert!(
        q.detail.starts_with("You can restore it from the "),
        "{q:?}"
    );
    assert!(q.button.starts_with("Move to "), "{q:?}");
    assert!(root.join("a.png").exists(), "asking moves nothing");
    // A ticket that was never handed out does nothing.
    let out: Trashed = f
        .call("file_trash", json!({ "ticket": q.ticket + 7 }))
        .unwrap();
    assert!(out.cancelled && out.trashed.is_empty());
    assert!(root.join("a.png").exists());
}

#[test]
fn trash_moves_the_photos_asked_about_once() {
    let f = fixture();
    let root = f.open();
    let (a, b) = (f.id_of("a.png"), f.id_of("b.png"));
    let (send, receive) = mpsc::channel();
    f.app.listen_any("album_changed", move |event| {
        let _ = send.send(event.payload().to_owned());
    });
    let q: TrashQuestion = f
        .call("file_trash_ask", json!({ "photo_ids": [b, a] }))
        .unwrap();
    assert!(q.message.starts_with("Move 2 photos"), "{q:?}");
    let out: Trashed = f.call("file_trash", json!({ "ticket": q.ticket })).unwrap();
    assert_eq!(out.trashed, vec![b, a]);
    assert!(out.failed.is_empty() && !out.cancelled);
    assert!(!root.join("a.png").exists());
    assert!(f.tmp.path().join("bin/a.png").exists());
    let changed = receive.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(changed.contains("\"trashed\""), "{changed}");
    // The ticket is used up.
    let again: Trashed = f.call("file_trash", json!({ "ticket": q.ticket })).unwrap();
    assert!(again.cancelled && again.trashed.is_empty());
    // Gone from the album's photos: another question cannot name them.
    let err = f
        .call::<Option<TrashQuestion>>("file_trash_ask", json!({ "photo_ids": [a] }))
        .unwrap_err();
    assert_eq!(err["code"], "photo_not_found");
}

#[test]
fn a_newer_question_replaces_the_older_ticket() {
    let f = fixture();
    let root = f.open();
    let (a, b) = (f.id_of("a.png"), f.id_of("b.png"));
    let first: TrashQuestion = f
        .call("file_trash_ask", json!({ "photo_ids": [a] }))
        .unwrap();
    let second: TrashQuestion = f
        .call("file_trash_ask", json!({ "photo_ids": [b] }))
        .unwrap();
    let stale: Trashed = f
        .call("file_trash", json!({ "ticket": first.ticket }))
        .unwrap();
    assert!(stale.cancelled);
    assert!(root.join("a.png").exists());
    let out: Trashed = f
        .call("file_trash", json!({ "ticket": second.ticket }))
        .unwrap();
    assert_eq!(out.trashed, vec![b]);
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
fn copy_text_puts_the_text_on_the_clipboard() {
    let f = fixture();
    let text = "Photo\nTaken: 12 Jun 2024";
    let () = f
        .call("file_copy_text", json!({ "content": text }))
        .unwrap();
    assert_eq!(f.fake.text.lock().unwrap().as_deref(), Some(text));
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
fn copy_image_draws_the_stored_edit() {
    let f = fixture();
    f.open();
    let b = f.id_of("b.png");
    f.call::<serde_json::Value>(
        "edit_set",
        json!({ "photo_id": b, "edit": { "rotate": 90, "flip": false } }),
    )
    .unwrap();
    let out: CopiedImage = f.call("file_copy_image", json!({ "photo_id": b })).unwrap();
    assert!(!out.scaled && !out.file);
    assert_eq!(
        *f.fake.image.lock().unwrap(),
        Some((20, 31)),
        "turned a quarter"
    );
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
fn save_all_writes_the_edits_into_the_files_in_album_mode_too() {
    // Owner's feedback, issue 209: Save all is offered whenever a photo has
    // an edit, whatever `edit_target` says.
    use owlbum_core::edit::save::{EditStatus, SavedEdits};
    let f = fixture();
    let root = f.open();
    let a = f.id_of("a.png");
    f.call::<serde_json::Value>(
        "edit_set",
        json!({ "photo_id": a, "edit": { "rotate": 90, "flip": false } }),
    )
    .unwrap();
    let out: SavedEdits = f.call("edit_save_all", json!({})).unwrap();
    assert_eq!(out.saved, vec![a], "{out:?}");
    assert!(
        f.fake.bin.join("a.png").exists(),
        "the original is in the bin"
    );
    let saved = image::open(root.join("a.png")).unwrap();
    assert_eq!((saved.width(), saved.height()), (20, 30));
    let status: EditStatus = f.call("edit_status", json!({})).unwrap();
    assert_eq!(status.edited, 0);
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
