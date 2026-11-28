//! The app data folder (docs/02-data.md, "The app data folder"): a cache
//! plus settings, per user. Deleting it loses nothing about any photo.
//!
//! - [`paths`] works out where the folder is (`OwlbumData`, `owlbum`, or
//!   the `Dev` folder of a development build; `OWLBUM_DATA_DIR` overrides).
//! - [`db`] opens `app.sqlite`, runs its migrations, and sets a damaged file
//!   aside.
//! - [`settings`] and [`recent`] read and write its tables.
//! - [`log`] is the log file and the `tracing` subscriber that writes it.
//!
//! [`AppData`] is the open folder that the shell keeps for the whole run.

pub mod clock;
pub mod db;
pub mod log;
pub mod paths;
pub mod recent;
pub mod settings;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use rusqlite::Connection;
use serde_json::{Map, Value};

use crate::AppError;
use crate::error::AppResult;
use crate::text;

pub use paths::{Location, Platform, locate};
pub use recent::Recent;
pub use settings::{GridPlace, Settings, Theme};

/// The open app data folder and its `app.sqlite`. Cloning is cheap; every
/// clone shares one connection, used by one caller at a time.
#[derive(Debug, Clone)]
pub struct AppData {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    dir: PathBuf,
    conn: Mutex<Connection>,
    set_aside: Option<PathBuf>,
}

impl AppData {
    /// Opens the app data folder `dir`, creating it and `app.sqlite` when
    /// missing. A damaged `app.sqlite` is set aside and a new one made; see
    /// [`AppData::set_aside`].
    ///
    /// # Errors
    ///
    /// `internal` when the folder cannot be created or `app.sqlite` cannot
    /// be opened for a reason other than damage. Both are fatal at start.
    pub fn open(dir: &Path) -> AppResult<Self> {
        std::fs::create_dir_all(dir).map_err(|e| {
            AppError::internal(text::appdata::CREATING_FOLDER)
                .with_details(serde_json::json!({ "cause": e.to_string() }))
        })?;
        let opened = db::open(dir, clock::now_ms())?;
        Ok(Self {
            inner: Arc::new(Inner {
                dir: dir.to_owned(),
                conn: Mutex::new(opened.conn),
                set_aside: opened.set_aside,
            }),
        })
    }

    /// The app data folder.
    pub fn dir(&self) -> &Path {
        &self.inner.dir
    }

    /// Where a damaged `app.sqlite` was moved when this open set one aside.
    /// The user is told once that settings were reset
    /// ([`text::appdata::SETTINGS_RESET`]).
    pub fn set_aside(&self) -> Option<&Path> {
        self.inner.set_aside.as_deref()
    }

    /// `<app data>/webview`, the webview's own data.
    pub fn webview_dir(&self) -> PathBuf {
        self.dir().join("webview")
    }

    /// Every setting, with defaults filled in.
    pub fn settings(&self) -> AppResult<Settings> {
        settings::get(&self.conn())
    }

    /// Writes `patch` and answers every setting. See [`settings::set`].
    pub fn set_settings(&self, patch: &Map<String, Value>) -> AppResult<Settings> {
        settings::set(&self.conn(), patch)
    }

    /// The recent list, newest first, with `exists` checked: every folder at
    /// once, each given at most 1.5 seconds.
    pub fn recent(&self) -> AppResult<Vec<Recent>> {
        self.recent_with(&recent::folder_exists(), recent::EXISTS_TIMEOUT)
    }

    /// [`AppData::recent`] with a given check and time limit. The database
    /// is not held while folders are checked.
    pub fn recent_with(&self, probe: &recent::Probe, timeout: Duration) -> AppResult<Vec<Recent>> {
        let entries = recent::entries(&self.conn())?;
        Ok(recent::with_exists(entries, probe, timeout))
    }

    /// Removes `root` from the recent list and answers the list.
    pub fn remove_recent(&self, root: &str) -> AppResult<Vec<Recent>> {
        recent::remove(&self.conn(), root)?;
        self.recent()
    }

    /// Puts `root` at the top of the recent list (an album was opened).
    pub fn touch_recent(&self, root: &str, now_ms: i64) -> AppResult<()> {
        recent::touch(&self.conn(), root, now_ms)
    }

    /// The saved session of `root`, if it has one.
    pub fn session(&self, root: &str) -> AppResult<Option<Value>> {
        recent::session(&self.conn(), root)
    }

    /// Saves the session of `root`, keeping fields this version does not
    /// know, and moves `root` to the top of the recent list.
    pub fn save_session(&self, root: &str, session: &Value, now_ms: i64) -> AppResult<()> {
        recent::save_session(&self.conn(), root, session, now_ms)
    }

    pub(crate) fn conn(&self) -> MutexGuard<'_, Connection> {
        // A panic while holding the lock leaves the connection usable: every
        // write is one transaction, rolled back if it did not finish.
        match self.inner.conn.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Instant;

    fn patch(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn opening_creates_the_folder_and_app_sqlite() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("nested").join("OwlbumData Dev");
        let data = AppData::open(&dir).unwrap();
        assert_eq!(data.dir(), dir);
        assert!(dir.join("app.sqlite").is_file());
        assert!(data.set_aside().is_none());
        assert_eq!(data.webview_dir(), dir.join("webview"));
        assert_eq!(data.settings().unwrap(), Settings::default());
    }

    #[test]
    fn settings_and_the_recent_list_survive_a_restart() {
        let tmp = tempfile::tempdir().unwrap();
        {
            let data = AppData::open(tmp.path()).unwrap();
            data.set_settings(&patch(json!({ "thumb_size": 200 })))
                .unwrap();
            data.touch_recent("/photos/a", 1).unwrap();
            data.save_session("/photos/a", &json!({ "view": "map" }), 2)
                .unwrap();
        }
        let data = AppData::open(tmp.path()).unwrap();
        assert_eq!(data.settings().unwrap().thumb_size, 200);
        let recent = data
            .recent_with(
                &(std::sync::Arc::new(|_: &Path| true) as recent::Probe),
                Duration::from_secs(1),
            )
            .unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].root, "/photos/a");
        assert_eq!(recent[0].name, "a");
        assert!(recent[0].exists);
        assert_eq!(
            data.session("/photos/a").unwrap(),
            Some(json!({ "view": "map" }))
        );
    }

    #[test]
    fn a_damaged_app_sqlite_is_reset_and_reported() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("app.sqlite"), b"garbage".repeat(200)).unwrap();
        let data = AppData::open(tmp.path()).unwrap();
        let moved = data.set_aside().expect("reported");
        assert!(moved.is_file());
        assert!(
            moved
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("app.damaged-")
        );
        assert_eq!(data.settings().unwrap(), Settings::default());
    }

    #[test]
    fn a_folder_that_cannot_be_created_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("a file");
        std::fs::write(&file, b"x").unwrap();
        let e = AppData::open(&file.join("data")).unwrap_err();
        assert_eq!(e.code, crate::ErrorCode::Internal);
    }

    #[test]
    fn remove_recent_answers_the_list_without_the_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let data = AppData::open(tmp.path()).unwrap();
        let a = tmp.path().join("a");
        std::fs::create_dir(&a).unwrap();
        data.touch_recent(&a.to_string_lossy(), 1).unwrap();
        data.touch_recent("/b", 2).unwrap();
        let list = data.remove_recent("/b").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].root, a.to_string_lossy());
        assert!(list[0].exists);
    }

    #[test]
    fn checking_folders_does_not_hold_up_settings() {
        let tmp = tempfile::tempdir().unwrap();
        let data = AppData::open(tmp.path()).unwrap();
        data.touch_recent("/slow", 1).unwrap();
        let reader = {
            let data = data.clone();
            std::thread::spawn(move || {
                let slow: recent::Probe = std::sync::Arc::new(|_: &Path| {
                    std::thread::sleep(Duration::from_secs(3));
                    true
                });
                data.recent_with(&slow, Duration::from_millis(1500))
                    .unwrap()
            })
        };
        std::thread::sleep(Duration::from_millis(200));
        let start = Instant::now();
        data.settings().unwrap();
        assert!(
            start.elapsed() < Duration::from_millis(500),
            "{:?}",
            start.elapsed()
        );
        assert!(!reader.join().unwrap()[0].exists);
    }
}
