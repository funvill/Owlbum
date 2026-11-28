//! Handing a folder to the page (docs/08-desktop-and-release.md, "Handing
//! a folder to the page").
//!
//! A folder can arrive before the page listens: from the command line, or
//! dropped on the window while the page loads. Until the page has called
//! `app_startup`, the newest such folder is kept and handed over in its
//! reply; after that, each one is sent at once as `open_folder`. A launch
//! that named a picture (Owlbum opening it from the file manager) hands
//! over the picture with its folder, to be selected and shown.

use std::path::Path;
use std::sync::Mutex;

use owlbum_core::app::OpenFolder;
use tauri::{AppHandle, Manager, Runtime};

use crate::events::emit;

/// The pending folder, and whether the page has started.
#[derive(Debug, Default)]
pub struct Handoff {
    state: Mutex<State>,
}

#[derive(Debug, Default)]
struct State {
    started: bool,
    pending: Option<OpenFolder>,
}

impl Handoff {
    /// The page has called `app_startup`: answers the pending folder and
    /// picture, which are then cleared. Folders that arrive from now on are
    /// sent as events.
    pub fn start(&self) -> Option<OpenFolder> {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.started = true;
        state.pending.take()
    }

    /// Keeps `open` for `app_startup` and answers true, or answers false
    /// once the page has started.
    fn keep(&self, open: &OpenFolder) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.started {
            return false;
        }
        state.pending = Some(open.clone());
        true
    }
}

/// A folder arrived that the page should open.
pub fn deliver<R: Runtime>(app: &AppHandle<R>, folder: &Path) {
    deliver_photo(app, folder, None);
}

/// A folder arrived that the page should open, with the picture in it to
/// select and show when the launch named a file.
pub fn deliver_photo<R: Runtime>(app: &AppHandle<R>, folder: &Path, photo: Option<&Path>) {
    let open = OpenFolder {
        path: folder.display().to_string(),
        photo: photo.map(|p| p.display().to_string()),
    };
    let kept = app.try_state::<Handoff>().is_some_and(|h| h.keep(&open));
    if !kept {
        let _ = emit(app, &open);
    }
}

/// A command line arrived (the first launch's, or a second launch's
/// handed over): opens what it names.
pub fn deliver_launch<R: Runtime>(app: &AppHandle<R>, launch: &owlbum_core::app::Launch) {
    deliver_photo(app, &launch.folder, launch.photo.as_deref());
}
