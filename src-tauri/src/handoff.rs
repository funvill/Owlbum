//! Handing a folder to the page (docs/08-desktop-and-release.md, "Handing
//! a folder to the page").
//!
//! A folder can arrive before the page listens: from the command line, or
//! dropped on the window while the page loads. Until the page has called
//! `app_startup`, the newest such folder is kept and handed over in its
//! reply; after that, each one is sent at once as `open_folder`.

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
    pending: Option<String>,
}

impl Handoff {
    /// The page has called `app_startup`: answers the pending folder, which
    /// is then cleared. Folders that arrive from now on are sent as events.
    pub fn start(&self) -> Option<String> {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.started = true;
        state.pending.take()
    }

    /// Keeps `path` for `app_startup` and answers true, or answers false
    /// once the page has started.
    fn keep(&self, path: &str) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.started {
            return false;
        }
        state.pending = Some(path.to_owned());
        true
    }
}

/// A folder arrived that the page should open.
pub fn deliver<R: Runtime>(app: &AppHandle<R>, folder: &Path) {
    let path = folder.display().to_string();
    let kept = app.try_state::<Handoff>().is_some_and(|h| h.keep(&path));
    if !kept {
        let _ = emit(app, &OpenFolder { path });
    }
}
