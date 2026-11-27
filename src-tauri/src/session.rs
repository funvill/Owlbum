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
//!
//! Opening and closing take a while (an album index loads, a scan is
//! waited for), so `album_open` and `album_close` run them off the main
//! thread, one at a time ([`Session::one_at_a_time`]).

use std::sync::{Arc, Mutex, MutexGuard};

use owlbum_core::album::{Album, AlbumChanged, ChangeReason};
use owlbum_core::error::AppResult;
use owlbum_core::query::QueryEngine;
use owlbum_core::scan::watch::{AlbumWatch, WatchEnv};
use owlbum_core::scan::{AlbumStatus, Request, ScanEnv, Scanner};
use owlbum_core::thumbs::Thumbs;
use owlbum_core::{AppError, ErrorCode, text};
use tauri::{AppHandle, Manager, Runtime};

use crate::events::TauriEmitter;

/// An open album: its scanner owns it, and its watch asks the scanner for
/// rescans. Its query results live as long as it does, so a token never
/// reaches another album's results.
#[derive(Debug)]
pub struct OpenAlbum {
    // Dropped first: the watch holds the scanner too.
    watch: AlbumWatch,
    scanner: Arc<Scanner>,
    queries: Arc<QueryEngine>,
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

    /// Its scans, to hold apart from the session's lock.
    pub fn scanner_arc(&self) -> &Arc<Scanner> {
        &self.scanner
    }

    /// Its folder watch.
    pub fn watch(&self) -> &AlbumWatch {
        &self.watch
    }

    /// Its query results.
    pub fn queries(&self) -> &Arc<QueryEngine> {
        &self.queries
    }
}

/// The window's open album, if any.
#[derive(Debug, Default)]
pub struct Session {
    open: Mutex<Option<OpenAlbum>>,
    /// Held for the whole of an open or a close.
    gate: Mutex<()>,
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
        *self.slot() = Some(OpenAlbum {
            watch,
            scanner,
            queries: Arc::new(QueryEngine::new()),
        });
    }

    /// The window gained focus: an album whose folder is not watched is
    /// rescanned if its last scan ended over 5 minutes ago
    /// (docs/03-media-and-scanning.md, "When watching cannot be trusted").
    pub fn focused(&self) {
        if let Some(open) = self.slot().as_ref() {
            open.watch.focused();
        }
    }

    /// Closes the open album: cancels its scan and waits for it. A picture
    /// request still in flight may keep the album itself a moment longer;
    /// it no longer tells the page anything.
    pub fn close(&self) {
        let old = self.slot().take();
        if let Some(old) = &old {
            old.album().set_status_listener(Arc::new(|_| {}));
            // Save all stops after the photo it is saving.
            old.album().saves().stop();
            old.scanner.cancel();
        }
        drop(old);
    }

    /// Held while an album opens or closes, so that two never overlap: the
    /// old album's lock is released before the new one is taken.
    pub fn one_at_a_time(&self) -> MutexGuard<'_, ()> {
        self.gate.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The open album's status; None when no album is open. The database is
    /// read without holding the session.
    pub fn status(&self) -> AppResult<Option<AlbumStatus>> {
        let held = self
            .slot()
            .as_ref()
            .map(|o| (Arc::clone(&o.scanner), o.watch.state()));
        held.map(|(scanner, watch)| AlbumStatus::of(&scanner, watch))
            .transpose()
    }

    /// The open album's scanner, held apart from the session's lock: for
    /// work that takes a while, such as making a picture.
    pub fn scanner(&self) -> Option<Arc<Scanner>> {
        self.slot().as_ref().map(|o| Arc::clone(&o.scanner))
    }

    /// The open album's scanner and query engine, held apart from the
    /// session's lock so a query does not hold up other commands.
    /// `no_album` when there is none.
    pub fn queries(&self) -> AppResult<(Arc<Scanner>, Arc<QueryEngine>)> {
        self.with(|open| Ok((Arc::clone(&open.scanner), Arc::clone(&open.queries))))
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
/// page, and changes to its state sent as `album_changed` with reason
/// `status`.
pub fn open_album<R: Runtime>(app: &AppHandle<R>, album: Album) {
    let emitter = Arc::new(TauriEmitter(app.clone()));
    {
        let emitter = Arc::clone(&emitter);
        album.set_status_listener(Arc::new(move |revision| {
            owlbum_core::events::emit(
                &*emitter,
                &AlbumChanged {
                    revision,
                    reason: ChangeReason::Status,
                    rekeyed: Vec::new(),
                },
            );
        }));
    }
    let mut env = ScanEnv::new(emitter);
    env.thumbs = app.try_state::<Thumbs>().map(|t| t.inner().clone());
    recover_saves(app, &album);
    app.state::<Session>().open(album, env);
}

/// Settles the save records of a save into a file that did not finish
/// (docs/06-editing-and-file-actions.md, "The save record and recovery"),
/// before the album's first scan. Its messages go to the log.
fn recover_saves<R: Runtime>(app: &AppHandle<R>, album: &Album) {
    let no_bin = |_: &std::path::Path| Err(String::new());
    let env =
        owlbum_core::edit::save::SaveEnv::new(crate::commands::edit::renders_dir(app), &no_bin);
    for notice in owlbum_core::edit::save::recover(album, &env) {
        tracing::warn!(target: "edit", "{notice}");
    }
}
