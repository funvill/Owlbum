//! An open album's scans: one at a time, in the background
//! (docs/03-media-and-scanning.md, "Cancellation").

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::memory::Overlay;
use super::progress::{Progress, ScanPhase};
use super::run::{Request, Scan, ScanEnv, ScanSummary, run};
use crate::album::Album;

/// The `scan` object of `album_status` (docs/03, "Commands").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ScanStatus {
    pub phase: ScanPhase,
    pub found: u32,
    pub done: u32,
    pub failed: u32,
    /// False when the folder cannot be watched (#14 sets it).
    pub watching: bool,
    /// Recognised but unsupported files in the last walk.
    pub unsupported: u32,
    /// Entries the last walk skipped and counts for the report (#9 adds
    /// links and online-only files to the unreadable names counted now).
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
    state: Mutex<State>,
    idle: Condvar,
}

#[derive(Default)]
struct State {
    running: bool,
    pending: Option<Request>,
    last: Option<ScanSummary>,
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
        state.pending = Some(state.pending.map_or(request, |p| p.merge(request)));
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
        let walk = state.last.as_ref().map(|s| &s.walk);
        ScanStatus {
            phase: now.phase,
            found: now.found,
            done: now.done,
            failed: now.failed,
            watching: false,
            unsupported: walk.map_or(0, |w| {
                u32::try_from(w.unsupported.len()).unwrap_or(u32::MAX)
            }),
            skipped: walk.map_or(0, |w| w.unreadable_names),
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
        let summary = run(
            &Scan {
                album: &shared.album,
                overlay: &shared.overlay,
                env: &shared.env,
                cancel: &shared.cancel,
                progress: &shared.progress,
            },
            request,
        );
        shared.state().last = Some(summary);
    }
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
