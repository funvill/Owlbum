//! Opening and closing an album, its status, the folder dialog, start-up
//! and dropped folders, through Tauri's mock runtime
//! (docs/04-browsing.md, "Opening a folder").

mod common;

use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use common::invoke;
use owlbum_core::app::{DropPhase, FolderDrop, OpenFolder, Startup};
use owlbum_core::appdata::AppData;
use owlbum_core::scan::{AlbumOpened, AlbumStatus};
use owlbum_lib::commands::app::FolderPicker;
use serde_json::json;
use tauri::test::{MockRuntime, mock_builder};
use tauri::{DragDropEvent, Listener, Manager};

struct Fx {
    tmp: tempfile::TempDir,
    data: AppData,
    app: tauri::App<MockRuntime>,
    window: tauri::WebviewWindow<MockRuntime>,
}

fn fixture() -> Fx {
    let tmp = tempfile::tempdir().unwrap();
    let data = AppData::open(&tmp.path().join("data")).unwrap();
    let (app, window) = common::window(owlbum_lib::with_app_data(
        owlbum_lib::setup(mock_builder()),
        data.clone(),
    ));
    Fx {
        tmp,
        data,
        app,
        window,
    }
}

impl Fx {
    /// A folder with one picture in `sub`, and the folder's path.
    fn folder(&self, name: &str) -> PathBuf {
        let root = self.tmp.path().join(name);
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("sub").join("a.txt"), b"not a photo").unwrap();
        root
    }

    fn open(
        &self,
        path: &Path,
        options: serde_json::Value,
    ) -> Result<AlbumOpened, serde_json::Value> {
        invoke(
            &self.window,
            "album_open",
            json!({ "path": path.display().to_string(), "options": options }),
        )
        .map(|r| r.deserialize().unwrap())
    }

    fn status(&self) -> Option<AlbumStatus> {
        invoke(&self.window, "album_status", json!({}))
            .unwrap()
            .deserialize()
            .unwrap()
    }

    /// Every payload of `event` the app sends, as it is sent.
    fn events<T: serde::de::DeserializeOwned + Send + 'static>(
        &self,
        event: &str,
    ) -> mpsc::Receiver<T> {
        let (send, receive) = mpsc::channel();
        self.app.listen_any(event, move |e| {
            let _ = send.send(serde_json::from_str(e.payload()).unwrap());
        });
        receive
    }
}

impl Drop for Fx {
    fn drop(&mut self) {
        let _ = invoke(&self.window, "album_close", json!({}));
    }
}

fn shown(path: &Path) -> String {
    owlbum_core::album::locate(path)
        .unwrap()
        .root
        .display()
        .to_string()
}

#[test]
fn without_an_album_the_status_is_null_and_closing_does_nothing() {
    let f = fixture();
    assert_eq!(f.status(), None);
    invoke(&f.window, "album_close", json!({})).expect("no error");
}

#[test]
fn opening_a_folder_answers_its_status_and_puts_it_in_the_recent_list() {
    let f = fixture();
    let root = f.folder("Trip");
    let opened = f.open(&root, json!(null)).unwrap();
    assert_eq!(opened.status.root, shown(&root));
    assert_eq!(opened.status.scope, "");
    assert_eq!(opened.status.name, "Trip");
    assert_eq!(opened.status.view_only, None);
    assert_eq!(opened.session, None);
    assert!(
        root.join(".owlbum").join("owlbum.sqlite").is_file(),
        "a new album is created"
    );
    assert_eq!(f.status().unwrap().root, opened.status.root);
    let recent = f.data.recent().unwrap();
    assert_eq!(recent[0].root, shown(&root));

    invoke(&f.window, "album_close", json!({})).unwrap();
    assert_eq!(f.status(), None);
    assert!(
        !root.join(".owlbum").join("lock").exists(),
        "the lock is released"
    );
}

#[test]
fn a_file_opens_its_folder_and_a_subfolder_opens_scoped() {
    let f = fixture();
    let root = f.folder("Trip");
    f.open(&root, json!(null)).unwrap();
    let scoped = f
        .open(&root.join("sub").join("a.txt"), json!(null))
        .unwrap();
    assert_eq!(scoped.status.root, shown(&root));
    assert_eq!(scoped.status.scope, "sub");
    assert_eq!(scoped.status.name, "sub");
    assert_eq!(
        scoped.status.folder,
        Path::new(&scoped.status.root)
            .join("sub")
            .display()
            .to_string()
    );
    assert_eq!(
        f.data.recent().unwrap()[0].root,
        scoped.status.folder,
        "the recent list holds the opened folder"
    );
    assert_eq!(
        scoped.status.view_only, None,
        "the old album's lock was released first"
    );

    // Show whole album: the root again, still writable.
    let whole = f.open(&root, json!(null)).unwrap();
    assert_eq!(whole.status.scope, "");
    assert_eq!(whole.status.view_only, None);
}

#[test]
fn a_folder_that_is_not_there_answers_not_a_folder_and_closes_the_open_album() {
    let f = fixture();
    let root = f.folder("Trip");
    f.open(&root, json!(null)).unwrap();
    let err = f.open(&f.tmp.path().join("gone"), json!(null)).unwrap_err();
    assert_eq!(err["code"], "not_a_folder");
    assert_eq!(f.status(), None);
}

#[test]
fn a_stale_lock_is_a_question_answered_with_an_option() {
    let f = fixture();
    let root = f.folder("Trip");
    f.open(&root, json!(null)).unwrap();
    invoke(&f.window, "album_close", json!({})).unwrap();
    let day_ago = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
        - 86_400_000;
    let lock = json!({
        "host": "STUDIO-PC", "pid": 1, "user": "them", "app_version": "0.1.0",
        "opened_at": day_ago, "heartbeat": day_ago,
    });
    std::fs::write(root.join(".owlbum").join("lock"), lock.to_string()).unwrap();

    let err = f.open(&root, json!(null)).unwrap_err();
    assert_eq!(err["code"], "album_lock_stale");
    assert_eq!(err["details"]["host"], "STUDIO-PC");

    let chosen = f.open(&root, json!({ "view_only": true })).unwrap();
    assert_eq!(
        chosen.status.view_only,
        Some(owlbum_core::album::ViewOnly::Chosen)
    );
    assert_eq!(chosen.status.lock_host.as_deref(), Some("STUDIO-PC"));

    let taken = f
        .open(
            &root,
            json!({ "take_lock": true, "view_only": false, "restore_backup": false }),
        )
        .unwrap();
    assert_eq!(taken.status.view_only, None);
}

#[test]
fn the_folder_dialog_answers_the_chosen_folder_or_null() {
    let f = fixture();
    let err = invoke(&f.window, "app_pick_folder", json!({})).unwrap_err();
    assert_eq!(err["code"], "internal", "no dialog installed");

    let chosen = Arc::new(std::sync::Mutex::new(Some(PathBuf::from("/photos/Trip"))));
    let answer = Arc::clone(&chosen);
    f.app.manage(FolderPicker(Arc::new(move || {
        answer.lock().unwrap().clone()
    })));
    let picked: Option<String> = invoke(&f.window, "app_pick_folder", json!({}))
        .unwrap()
        .deserialize()
        .unwrap();
    assert_eq!(
        picked,
        Some(PathBuf::from("/photos/Trip").display().to_string())
    );
    *chosen.lock().unwrap() = None;
    let cancelled: Option<String> = invoke(&f.window, "app_pick_folder", json!({}))
        .unwrap()
        .deserialize()
        .unwrap();
    assert_eq!(cancelled, None);
}

fn startup(f: &Fx) -> Startup {
    invoke(&f.window, "app_startup", json!({}))
        .unwrap()
        .deserialize()
        .unwrap()
}

#[test]
fn startup_reopens_the_newest_recent_folder_that_exists() {
    let f = fixture();
    let first = startup(&f);
    assert_eq!(first.launch_folder, None);
    assert_eq!(first.reopen, None, "nothing recent");
    assert_eq!(first.version, owlbum_core::app::version().version);
    assert!(first.show_update_notice);

    let root = f.folder("Trip");
    f.data.touch_recent(&root.display().to_string(), 1).unwrap();
    let again = startup(&f);
    assert_eq!(again.reopen, Some(root.display().to_string()));
    assert_eq!(again.recent.len(), 1);
}

#[test]
fn a_folder_that_arrives_before_startup_is_handed_over_and_later_ones_are_events() {
    let f = fixture();
    let opens = f.events::<OpenFolder>("open_folder");
    let early = f.folder("Early");
    let earlier = f.folder("Earlier");
    owlbum_lib::handoff::deliver(f.app.handle(), &earlier);
    owlbum_lib::handoff::deliver(f.app.handle(), &early);
    let s = startup(&f);
    assert_eq!(
        s.launch_folder,
        Some(early.display().to_string()),
        "the later one wins"
    );
    assert_eq!(s.reopen, None, "the launch folder wins");
    assert!(opens.try_recv().is_err(), "no event before start");

    let late = f.folder("Late");
    owlbum_lib::handoff::deliver(f.app.handle(), &late);
    let event = opens.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(event.path, late.display().to_string());
}

#[test]
fn a_drag_shows_the_outline_and_a_drop_opens_its_first_folder() {
    let f = fixture();
    startup(&f);
    let drops = f.events::<FolderDrop>("folder_drop");
    let opens = f.events::<OpenFolder>("open_folder");
    let handle = f.app.handle();
    let root = f.folder("Trip");
    let file = root.join("sub").join("a.txt");

    owlbum_lib::dropped(
        handle,
        &DragDropEvent::Enter {
            paths: vec![file.clone()],
            position: Default::default(),
        },
    );
    assert_eq!(
        drops.recv_timeout(Duration::from_secs(5)).unwrap().phase,
        DropPhase::Over
    );
    owlbum_lib::dropped(handle, &DragDropEvent::Leave);
    assert_eq!(
        drops.recv_timeout(Duration::from_secs(5)).unwrap().phase,
        DropPhase::Left
    );

    owlbum_lib::dropped(
        handle,
        &DragDropEvent::Drop {
            paths: vec![file.clone(), root.clone()],
            position: Default::default(),
        },
    );
    assert_eq!(
        drops.recv_timeout(Duration::from_secs(5)).unwrap().phase,
        DropPhase::Left
    );
    let open = opens.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(
        open.path,
        root.display().to_string(),
        "the first folder, not the file's"
    );

    owlbum_lib::dropped(
        handle,
        &DragDropEvent::Drop {
            paths: vec![f.tmp.path().join("gone")],
            position: Default::default(),
        },
    );
    assert_eq!(
        drops.recv_timeout(Duration::from_secs(5)).unwrap().phase,
        DropPhase::Left
    );
    assert_eq!(
        drops.recv_timeout(Duration::from_secs(5)).unwrap().phase,
        DropPhase::Refused
    );
    assert!(opens.try_recv().is_err());
}
