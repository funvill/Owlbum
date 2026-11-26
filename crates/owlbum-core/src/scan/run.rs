//! One scan, start to finish, on the calling thread (with the scan pool
//! for the passes and one thread that batches writes).

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::memory::Overlay;
use super::progress::Progress;
use super::reader::{MediaReader, Reader};
use super::walk::Walk;
use crate::AppError;
use crate::album::Album;
use crate::events::Emitter;

/// What asked for a scan. Requests that arrive while a scan runs are
/// merged into one more scan. The folder watcher's targeted rescans (#14)
/// add the folders to scan here.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Request {
    /// A manual Rescan: rows with a `decode`, `unsupported` or `too_large`
    /// error are read again too.
    pub manual: bool,
}

impl Request {
    /// One request that does what both ask.
    #[must_use]
    pub fn merge(self, other: Self) -> Self {
        Self {
            manual: self.manual || other.manual,
        }
    }
}

/// How scans run: what reads files, where events go, and the limits from
/// docs/03 ("Constants"). Tests shorten the waits.
#[derive(Clone)]
pub struct ScanEnv {
    /// Reads files: the format registry's handlers.
    pub reader: Arc<dyn Reader>,
    /// Where `scan_progress` and `album_changed` go.
    pub emitter: Arc<dyn Emitter>,
    /// The scan pool: `max(1, logical processors − 1)`.
    pub threads: usize,
    /// [`super::SCAN_VERSION`].
    pub scan_version: i64,
    /// A batch of writes commits when this many rows wait…
    pub batch_rows: usize,
    /// …or this long after the first one, whichever comes first.
    pub batch_wait: Duration,
    /// `scan_progress` at most once per this.
    pub progress_every: Duration,
    /// `album_changed` at most once per this while the scan writes.
    pub changed_every: Duration,
    /// A file modified less than this long ago that fails to decode is
    /// taken to be still being written…
    pub busy_recent: Duration,
    /// …and is tried again after this long…
    pub busy_wait: Duration,
    /// …at most this many times.
    pub busy_tries: u32,
}

impl std::fmt::Debug for ScanEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScanEnv")
            .field("threads", &self.threads)
            .field("scan_version", &self.scan_version)
            .finish_non_exhaustive()
    }
}

impl ScanEnv {
    /// The real limits, reading with the format registry.
    pub fn new(emitter: Arc<dyn Emitter>) -> Self {
        let cpus = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
        Self {
            reader: Arc::new(MediaReader),
            emitter,
            threads: cpus.saturating_sub(1).max(1),
            scan_version: super::SCAN_VERSION,
            batch_rows: 200,
            batch_wait: Duration::from_millis(250),
            progress_every: Duration::from_millis(100),
            changed_every: Duration::from_secs(1),
            busy_recent: Duration::from_secs(2),
            busy_wait: Duration::from_secs(2),
            busy_tries: 5,
        }
    }
}

/// What one scan needs: the album, its in-memory rows, the environment, the
/// cancel flag and the progress counts.
pub struct Scan<'a> {
    pub album: &'a Album,
    pub overlay: &'a Mutex<Overlay>,
    pub env: &'a ScanEnv,
    pub cancel: &'a AtomicBool,
    pub progress: &'a Progress,
}

/// What a scan did.
#[derive(Debug, Clone, Default)]
pub struct ScanSummary {
    /// Supported files listed.
    pub found: u32,
    /// Files that need nothing more.
    pub done: u32,
    /// Files given a `scan_error` by this scan.
    pub failed: u32,
    /// The scan was cancelled (or stopped by a failed write).
    pub cancelled: bool,
    /// Why the scan stopped early: the scope could not be read, or a write
    /// failed. Nothing is marked missing then.
    pub error: Option<AppError>,
    /// What the walk found, for the scan report (#9).
    pub walk: Walk,
    /// New rows.
    pub added: u32,
    /// Rows given to a renamed or moved file.
    pub moved: u32,
    /// Rows whose facts were read again.
    pub changed: u32,
    /// Rows marked missing.
    pub missing: u32,
    /// Photos whose `file_key` changed.
    pub rekeyed: Vec<i64>,
    /// Transactions the scan's writes took.
    pub batches: u32,
    /// The most rows one of them held.
    pub largest_batch: u32,
}

/// Runs one scan of the album's scope.
pub fn run(scan: &Scan<'_>, request: Request) -> ScanSummary {
    let _ = (scan, request);
    ScanSummary::default()
}
