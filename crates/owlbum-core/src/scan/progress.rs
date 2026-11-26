//! `scan_progress` and the scanner's `album_changed`
//! (docs/03-media-and-scanning.md, "Progress and change events").

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Event;
use crate::album::{Album, AlbumChanged, ChangeReason};
use crate::events::{Emitter, emit};

/// Where a scan is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ScanPhase {
    /// Listing files, until the compare stage ends.
    Walking,
    /// Reading new, changed and waiting files.
    Reading,
    /// No scan is running.
    Idle,
}

/// The `scan_progress` event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ScanProgress {
    pub phase: ScanPhase,
    /// Supported files listed so far; final when the walk ends.
    pub found: u32,
    /// Files that need nothing more.
    pub done: u32,
    /// Files given a `scan_error` by this scan.
    pub failed: u32,
    /// True on the last event of a scan, including a cancelled one.
    pub finished: bool,
    /// True on the last event when the scan was cancelled.
    pub cancelled: bool,
}

impl Event for ScanProgress {
    const NAME: &'static str = "scan_progress";
}

/// The counts of the running (or last) scan, and the rate limit on telling
/// the page about them.
pub struct Progress {
    found: AtomicU32,
    done: AtomicU32,
    failed: AtomicU32,
    phase: Mutex<ScanPhase>,
    last_sent: Mutex<Option<Instant>>,
    every: Duration,
    emitter: std::sync::Arc<dyn Emitter>,
    running: AtomicBool,
}

impl std::fmt::Debug for Progress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Progress")
            .field("now", &self.now(false, false))
            .finish_non_exhaustive()
    }
}

impl Progress {
    /// Progress sent through `emitter`, at most once per `every`.
    pub fn new(emitter: std::sync::Arc<dyn Emitter>, every: Duration) -> Self {
        Self {
            found: AtomicU32::new(0),
            done: AtomicU32::new(0),
            failed: AtomicU32::new(0),
            phase: Mutex::new(ScanPhase::Idle),
            last_sent: Mutex::new(None),
            every,
            emitter,
            running: AtomicBool::new(false),
        }
    }

    /// A scan starts: counts to 0, phase `walking`.
    pub fn start(&self) {
        self.found.store(0, Ordering::SeqCst);
        self.done.store(0, Ordering::SeqCst);
        self.failed.store(0, Ordering::SeqCst);
        *self.phase.lock().unwrap_or_else(|p| p.into_inner()) = ScanPhase::Walking;
        self.running.store(true, Ordering::SeqCst);
        self.tick();
    }

    /// `n` files listed so far.
    pub fn found(&self, n: usize) {
        self.found
            .store(u32::try_from(n).unwrap_or(u32::MAX), Ordering::SeqCst);
        self.tick();
    }

    /// The compare stage ended.
    pub fn reading(&self) {
        *self.phase.lock().unwrap_or_else(|p| p.into_inner()) = ScanPhase::Reading;
        self.tick();
    }

    /// `n` more files need nothing more; `failed` of them got a
    /// `scan_error`.
    pub fn done(&self, n: u32, failed: u32) {
        self.done.fetch_add(n, Ordering::SeqCst);
        self.failed.fetch_add(failed, Ordering::SeqCst);
        self.tick();
    }

    /// The scan ended: the last event, always sent.
    pub fn finish(&self, cancelled: bool) {
        *self.phase.lock().unwrap_or_else(|p| p.into_inner()) = ScanPhase::Idle;
        self.running.store(false, Ordering::SeqCst);
        let last = self.now(true, cancelled);
        *self.last_sent.lock().unwrap_or_else(|p| p.into_inner()) = Some(Instant::now());
        emit(&*self.emitter, &last);
    }

    /// The counts now, as an event payload.
    pub fn now(&self, finished: bool, cancelled: bool) -> ScanProgress {
        ScanProgress {
            phase: *self.phase.lock().unwrap_or_else(|p| p.into_inner()),
            found: self.found.load(Ordering::SeqCst),
            done: self.done.load(Ordering::SeqCst),
            failed: self.failed.load(Ordering::SeqCst),
            finished,
            cancelled,
        }
    }

    /// True while a scan runs.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Sends the counts unless an event went out less than `every` ago.
    fn tick(&self) {
        let mut last = self.last_sent.lock().unwrap_or_else(|p| p.into_inner());
        let now = Instant::now();
        if last.is_some_and(|t| now.duration_since(t) < self.every) {
            return;
        }
        *last = Some(now);
        let payload = self.now(false, false);
        drop(last);
        emit(&*self.emitter, &payload);
    }
}

/// Tells the page the album changed (`album_changed`, reason `scan`, or
/// `watch` for the folder watcher's scans), at most once per `every` while
/// a scan writes, and once at the end.
pub struct Changes<'a> {
    album: &'a Album,
    emitter: &'a dyn Emitter,
    every: Duration,
    reason: ChangeReason,
    state: Mutex<ChangesState>,
}

#[derive(Default)]
struct ChangesState {
    dirty: bool,
    rekeyed: Vec<i64>,
    last: Option<Instant>,
}

impl<'a> Changes<'a> {
    pub fn new(
        album: &'a Album,
        emitter: &'a dyn Emitter,
        every: Duration,
        reason: ChangeReason,
    ) -> Self {
        Self {
            album,
            emitter,
            every,
            reason,
            state: Mutex::new(ChangesState::default()),
        }
    }

    /// Rows were written; `rekeyed` lists photos whose content changed.
    pub fn changed(&self, rekeyed: impl IntoIterator<Item = i64>) {
        let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
        s.dirty = true;
        s.rekeyed.extend(rekeyed);
        let now = Instant::now();
        if s.last.is_none_or(|t| now.duration_since(t) >= self.every) {
            self.send(&mut s, now);
        }
    }

    /// The scan's last write: sends what has not been sent.
    pub fn flush(&self) {
        let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if s.dirty {
            self.send(&mut s, Instant::now());
        }
    }

    fn send(&self, s: &mut ChangesState, now: Instant) {
        let event = AlbumChanged {
            revision: self.album.next_revision(),
            reason: self.reason,
            rekeyed: std::mem::take(&mut s.rekeyed),
        };
        s.dirty = false;
        s.last = Some(now);
        emit(self.emitter, &event);
    }
}
