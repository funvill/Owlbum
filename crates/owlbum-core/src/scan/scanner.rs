//! An open album's scans: one at a time, in the background
//! (docs/03-media-and-scanning.md, "Cancellation").

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::memory::Overlay;
use super::progress::{Progress, ScanPhase};
use super::report::{self, ScanErrors, ScanReport, UnsupportedFiles};
use super::run::{Request, Scan, ScanEnv, ScanSummary, run_with};
use super::walk::Walk;
use crate::album::Album;
use crate::error::AppResult;

/// The `scan` object of `album_status` (docs/03, "Commands").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ScanStatus {
    pub phase: ScanPhase,
    pub found: u32,
    pub done: u32,
    pub failed: u32,
    /// False when the folder cannot be watched ([`super::watch`]).
    pub watching: bool,
    /// Recognised but unsupported files in the last whole walk.
    pub unsupported: u32,
    /// Links, online-only files and unreadable names the last whole walk
    /// skipped ([`Walk::skipped`]).
    pub skipped: u32,
}

/// Runs an open album's scans on a background thread. Dropping it cancels
/// the running scan and waits for it, so drop it before the album.
pub struct Scanner {
    shared: Arc<Shared>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

struct Shared {
    album: Arc<Album>,
    env: ScanEnv,
    overlay: Mutex<Overlay>,
    progress: Progress,
    cancel: AtomicBool,
    /// The folder watcher has a watch on the scope.
    watching: AtomicBool,
    state: Mutex<State>,
    idle: Condvar,
}

#[derive(Default)]
struct State {
    running: bool,
    pending: Option<Request>,
    last: Option<ScanSummary>,
    /// When the last scan ended.
    ended_at: Option<Instant>,
    /// The last walk that ended, for the report, kept from the moment it
    /// ends: a walk that failed or was cancelled does not replace it.
    walk: Option<Walk>,
    closed: bool,
}

impl std::fmt::Debug for Scanner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scanner")
            .field("status", &self.status())
            .finish_non_exhaustive()
    }
}

impl Shared {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }
}

impl Scanner {
    /// A scanner for `album`. No scan starts until one is requested.
    pub fn new(album: Arc<Album>, env: ScanEnv) -> Self {
        let progress = Progress::new(Arc::clone(&env.emitter), env.progress_every);
        Self {
            shared: Arc::new(Shared {
                album,
                env,
                overlay: Mutex::new(Overlay::default()),
                progress,
                cancel: AtomicBool::new(false),
                watching: AtomicBool::new(false),
                state: Mutex::new(State::default()),
                idle: Condvar::new(),
            }),
            thread: Mutex::new(None),
        }
    }

    /// The album this scanner scans.
    pub fn album(&self) -> &Arc<Album> {
        &self.shared.album
    }

    /// Starts a scan, or, while one runs, asks for one more when it ends
    /// (merged with any other request already waiting).
    pub fn request(&self, request: Request) {
        let mut state = self.shared.state();
        if state.closed {
            return;
        }
        state.pending = Some(match state.pending.take() {
            Some(p) => p.merge(request),
            None => request,
        });
        if state.running {
            return;
        }
        state.running = true;
        drop(state);

        let shared = Arc::clone(&self.shared);
        let spawned = std::thread::Builder::new()
            .name("owlbum-scan".into())
            .spawn(move || scan_loop(&shared));
        let mut thread = self.thread.lock().unwrap_or_else(|p| p.into_inner());
        match spawned {
            Ok(handle) => {
                if let Some(old) = thread.replace(handle) {
                    // It ended before `running` was cleared.
                    let _ = old.join();
                }
            }
            Err(_) => {
                let mut state = self.shared.state();
                state.running = false;
                state.pending = None;
                self.shared.idle.notify_all();
            }
        }
    }

    /// Cancels the running scan and any waiting request. Nothing happens
    /// when no scan runs.
    pub fn cancel(&self) {
        let mut state = self.shared.state();
        state.pending = None;
        if state.running {
            self.shared.cancel.store(true, Ordering::SeqCst);
        }
    }

    /// Waits until no scan runs and none waits.
    pub fn wait(&self) {
        let mut state = self.shared.state();
        while state.running {
            state = self
                .shared
                .idle
                .wait(state)
                .unwrap_or_else(|p| p.into_inner());
        }
    }

    /// Requests a scan and waits for it (and any merged after it). Answers
    /// the last scan's summary.
    pub fn scan_and_wait(&self, request: Request) -> ScanSummary {
        self.request(request);
        self.wait();
        self.last().unwrap_or_default()
    }

    /// True while a scan runs.
    pub fn is_running(&self) -> bool {
        self.shared.state().running
    }

    /// The last finished scan's summary.
    pub fn last(&self) -> Option<ScanSummary> {
        self.shared.state().last.clone()
    }

    /// The counts for `album_status`.
    pub fn status(&self) -> ScanStatus {
        let now = self.shared.progress.now(false, false);
        let state = self.shared.state();
        let walk = state.walk.as_ref();
        ScanStatus {
            phase: now.phase,
            found: now.found,
            done: now.done,
            failed: now.failed,
            watching: self.shared.watching.load(Ordering::SeqCst),
            unsupported: walk.map_or(0, |w| {
                u32::try_from(w.unsupported.len()).unwrap_or(u32::MAX)
            }),
            skipped: walk.map_or(0, Walk::skipped),
        }
    }

    /// The folder watcher has, or has lost, its watch on the scope.
    pub fn set_watching(&self, watching: bool) {
        self.shared.watching.store(watching, Ordering::SeqCst);
    }

    /// True when no scan runs or waits and the last one ended at least
    /// `after` before `now`: the rule for rescanning on window focus when
    /// the folder is not watched.
    pub fn idle_for(&self, after: Duration, now: Instant) -> bool {
        let state = self.shared.state();
        !state.running
            && state
                .ended_at
                .is_some_and(|t| now.saturating_duration_since(t) >= after)
    }

    /// True when any row (in the database, or held in memory for a
    /// view-only album) lies under the folder `rel_dir`: the folder
    /// watcher's test for a gone path that was a folder.
    pub fn has_rows_under(&self, rel_dir: &str) -> bool {
        if rel_dir.is_empty() {
            return true;
        }
        let in_memory = self
            .overlay()
            .rows()
            .any(|r| r.rel_path != rel_dir && super::walk::is_under(&r.rel_path, rel_dir));
        in_memory
            || self
                .shared
                .album
                .db()
                .read(|conn| super::rows::has_rows_under(conn, rel_dir))
                .unwrap_or(false)
    }

    /// `album_scan_report`: what the last whole walk left out.
    pub fn report(&self) -> ScanReport {
        report::report(self.shared.state().walk.as_ref())
    }

    /// `album_unsupported`: one page of the last whole walk's unsupported
    /// files.
    pub fn unsupported(&self, offset: u32, limit: u32) -> UnsupportedFiles {
        report::unsupported(self.shared.state().walk.as_ref(), offset, limit)
    }

    /// `album_scan_errors`: one page of the rows in scope with a scan
    /// error.
    pub fn scan_errors(&self, offset: u32, limit: u32) -> AppResult<ScanErrors> {
        let overlay = self.overlay();
        report::scan_errors(&self.shared.album, &overlay, offset, limit)
    }

    /// `album_copy_not_shown`: the whole Files not shown list as plain
    /// text.
    pub fn not_shown_text(&self) -> AppResult<String> {
        let unsupported = self
            .shared
            .state()
            .walk
            .as_ref()
            .map(|w| w.unsupported.clone())
            .unwrap_or_default();
        let overlay = self.overlay();
        report::not_shown(&self.shared.album, &overlay, &unsupported)
    }

    /// The open album and its rows held in memory, for the picture URLs.
    pub fn open(&self) -> crate::thumbs::serve::Open<'_> {
        crate::thumbs::serve::Open {
            album: &self.shared.album,
            overlay: &self.shared.overlay,
        }
    }

    /// The scan facts held in memory for a view-only album.
    pub fn overlay(&self) -> MutexGuard<'_, Overlay> {
        self.shared
            .overlay
            .lock()
            .unwrap_or_else(|p| p.into_inner())
    }
}

fn scan_loop(shared: &Shared) {
    loop {
        let request = {
            let mut state = shared.state();
            match state.pending.take() {
                Some(r) if !state.closed => {
                    shared.cancel.store(false, Ordering::SeqCst);
                    r
                }
                _ => {
                    state.running = false;
                    shared.idle.notify_all();
                    return;
                }
            }
        };
        let summary = run_with(
            &Scan {
                album: &shared.album,
                overlay: &shared.overlay,
                env: &shared.env,
                cancel: &shared.cancel,
                progress: &shared.progress,
            },
            request,
            // The report answers from this walk while the passes run.
            |walk| shared.state().walk = Some(walk.clone()),
        );
        let cancelled = summary.cancelled;
        {
            let mut state = shared.state();
            state.last = Some(summary);
            state.ended_at = Some(Instant::now());
        }
        if !cancelled {
            warm(shared);
        }
    }
}

/// The warm pass after a scan (docs/03, "Finish"): the small thumbnail of
/// every photo in scope that has none in the cache. Lowest priority: it
/// stops on cancel and when another scan is asked for.
fn warm(shared: &Shared) {
    let Some(thumbs) = &shared.env.thumbs else {
        return;
    };
    thumbs.cache().flush();
    let photos = match crate::thumbs::lookup::complete_in_scope(&shared.album, &shared.overlay) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(target: "thumbs", "reading photos for the warm pass: {}", e.message);
            return;
        }
    };
    let stop = || shared.cancel.load(Ordering::SeqCst) || shared.state().pending.is_some();
    thumbs.warm(&photos, &stop);
}

impl Drop for Scanner {
    fn drop(&mut self) {
        {
            let mut state = self.shared.state();
            state.closed = true;
            state.pending = None;
            self.shared.cancel.store(true, Ordering::SeqCst);
        }
        if let Some(handle) = self.thread.lock().unwrap_or_else(|p| p.into_inner()).take() {
            let _ = handle.join();
        }
    }
}
