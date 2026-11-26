//! The open album and its scanner, held for the window as Tauri state.
//!
//! `album_open` (#16) puts an album here with [`open_album`]; the scan
//! commands (`album_rescan`, `album_scan_cancel`) act on it. Its folder is
//! watched while it is open, and when it cannot be, the window gaining
//! focus rescans it ([`Session::focused`]). Opening another album, closing,
//! and quitting drop the old one, which ends its watch, cancels its scan
//! and waits for both before the album database closes. A picture request
//! in flight holds the album ([`Session::scanner`]) until it answers, so
//! the last of those closes it.

use std::sync::{Arc, Mutex, MutexGuard};

use owlbum_core::album::Album;
use owlbum_core::error::AppResult;
use owlbum_core::scan::watch::{AlbumWatch, WatchEnv};
use owlbum_core::scan::{Request, ScanEnv, Scanner};
use owlbum_core::thumbs::Thumbs;
use owlbum_core::{AppError, ErrorCode, text};
use tauri::{AppHandle, Manager, Runtime};

use crate::events::TauriEmitter;

/// An open album: its scanner owns it, and its watch asks the scanner for
/// rescans.
#[derive(Debug)]
pub struct OpenAlbum {
    // Dropped first: the watch holds the scanner too.
    watch: AlbumWatch,
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

    /// Its folder watch.
    pub fn watch(&self) -> &AlbumWatch {
        &self.watch
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

    /// Makes `album` the open album, starts its first scan in the
    /// background and starts watching its folder. An album already open is
    /// closed first.
    pub fn open(&self, album: Album, env: ScanEnv) {
        self.open_with(album, env, WatchEnv::default());
    }

    /// [`Self::open`] with the watch's timing given (tests shorten it).
    pub fn open_with(&self, album: Album, env: ScanEnv, watch: WatchEnv) {
        self.close();
        let scanner = Arc::new(Scanner::new(Arc::new(album), env));
        scanner.request(Request::default());
        let watch = AlbumWatch::start(Arc::clone(&scanner), watch);
        *self.slot() = Some(OpenAlbum { watch, scanner });
    }

    /// The window gained focus: an album whose folder is not watched is
    /// rescanned if its last scan ended over 5 minutes ago
    /// (docs/03-media-and-scanning.md, "When watching cannot be trusted").
    pub fn focused(&self) {
        if let Some(open) = self.slot().as_ref() {
            open.watch.focused();
        }
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
