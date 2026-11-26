//! The open album and its scanner, held for the window as Tauri state.
//!
//! `album_open` (#16) puts an album here with [`open_album`]; the scan
//! commands (`album_rescan`, `album_scan_cancel`) act on it. Opening
//! another album, closing, and quitting drop the old one, which cancels its
//! scan and waits for it before the album database closes. A picture
//! request in flight holds the album ([`Session::scanner`]) until it
//! answers, so the last of those closes it.

use std::sync::{Arc, Mutex, MutexGuard};

use owlbum_core::album::Album;
use owlbum_core::error::AppResult;
use owlbum_core::scan::{Request, ScanEnv, Scanner};
use owlbum_core::thumbs::Thumbs;
use owlbum_core::{AppError, ErrorCode, text};
use tauri::{AppHandle, Manager, Runtime};

use crate::events::TauriEmitter;

/// An open album: its scanner owns it.
#[derive(Debug)]
pub struct OpenAlbum {
    scanner: Arc<Scanner>,
}

impl OpenAlbum {
    /// The album.
    pub fn album(&self) -> &Arc<Album> {
        self.scanner.album()
    }

    /// Its scans.
    pub fn scanner(&self) -> &Scanner {
        &self.scanner
    }
}

/// The window's open album, if any.
#[derive(Debug, Default)]
pub struct Session {
    open: Mutex<Option<OpenAlbum>>,
}

impl Session {
    fn slot(&self) -> MutexGuard<'_, Option<OpenAlbum>> {
        self.open.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Makes `album` the open album and starts its first scan in the
    /// background. An album already open is closed first.
    pub fn open(&self, album: Album, env: ScanEnv) {
        self.close();
        let scanner = Arc::new(Scanner::new(Arc::new(album), env));
        scanner.request(Request::default());
        *self.slot() = Some(OpenAlbum { scanner });
    }

    /// Closes the open album: cancels its scan and waits for it.
    pub fn close(&self) {
        let old = self.slot().take();
        if let Some(old) = &old {
            old.scanner.cancel();
        }
        drop(old);
    }

    /// The open album's scanner, held apart from the session's lock: for
    /// work that takes a while, such as making a picture.
    pub fn scanner(&self) -> Option<Arc<Scanner>> {
        self.slot().as_ref().map(|o| Arc::clone(&o.scanner))
    }

    /// Runs `f` with the open album; `no_album` when there is none.
    pub fn with<T>(&self, f: impl FnOnce(&OpenAlbum) -> AppResult<T>) -> AppResult<T> {
        match self.slot().as_ref() {
            Some(open) => f(open),
            None => Err(AppError::new(ErrorCode::NoAlbum, text::album::NO_ALBUM)),
        }
    }

    /// True when an album is open.
    pub fn is_open(&self) -> bool {
        self.slot().is_some()
    }
}

/// Opens `album` in the app's session, with its scans' events sent to the
/// page.
pub fn open_album<R: Runtime>(app: &AppHandle<R>, album: Album) {
    let mut env = ScanEnv::new(Arc::new(TauriEmitter(app.clone())));
    env.thumbs = app.try_state::<Thumbs>().map(|t| t.inner().clone());
    app.state::<Session>().open(album, env);
}
