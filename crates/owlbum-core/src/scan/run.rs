//! One scan, start to finish, on the calling thread (with the scan pool
//! for the passes and one thread that batches writes).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime};

use super::keys::{self, Sample};
use super::memory::Overlay;
use super::order::natural_cmp;
use super::pixels;
use super::progress::{Changes, Progress};
use super::reader::{MediaReader, Reader};
use super::rows::{self, Facts, Known, Op};
use super::walk::{self, Listed, Walk};
use crate::AppError;
use crate::album::Album;
use crate::error::AppResult;
use crate::events::Emitter;
use crate::media::{
    self, Cost, Identified, MediaError, MediaErrorCode, MediaFormat, MediaKind, Pixels,
};

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
    /// Where the pixels pass writes thumbnails, and the warm pass after a
    /// scan (#11). None writes no thumbnails.
    pub thumbs: Option<crate::thumbs::Thumbs>,
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
            thumbs: None,
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
    /// What the walk found, for the scan report ([`super::report`]).
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

/// Runs one scan of the album's scope (docs/03-media-and-scanning.md, "The
/// scanner"). Always ends with a finished `scan_progress`.
pub fn run(scan: &Scan<'_>, request: Request) -> ScanSummary {
    run_with(scan, request, |_| {})
}

/// [`run`], calling `walked` with the walk as soon as a whole walk ends,
/// before the passes, so the scan report can answer while they run.
pub fn run_with(scan: &Scan<'_>, request: Request, walked: impl FnOnce(&Walk)) -> ScanSummary {
    scan.progress.start();
    let mut summary = ScanSummary::default();
    let location = scan.album.location();
    match walk::walk(&location.root, &location.scope, scan.cancel, |n| {
        scan.progress.found(n);
    }) {
        Ok(walk) => {
            if walk.complete {
                walked(&walk);
            }
            summary.walk = walk;
        }
        Err(e) => {
            // Nothing in the database is changed: an unplugged drive must
            // not mark an album missing.
            summary.error = Some(e);
            finish(scan, &mut summary);
            return summary;
        }
    }
    let changes = Changes::new(scan.album, &*scan.env.emitter, scan.env.changed_every);
    let stats = Stats::default();
    std::thread::scope(|threads| {
        let (sink, batcher) = if scan.album.is_view_only() {
            (Sink::Memory, None)
        } else {
            let (send, receive) = mpsc::channel::<Op>();
            let batcher = threads.spawn(|| batch_writes(scan, receive, &changes, &stats));
            (Sink::Db(send), Some(batcher))
        };
        let work = Work {
            scan,
            request,
            sink,
            changes: &changes,
            stats: &stats,
        };
        work.stages(&mut summary);
        // Closing the channel lets the batcher commit what waits, and end.
        drop(work);
        if let Some(batcher) = batcher {
            let _ = batcher.join();
        }
    });
    changes.flush();
    let mut st = stats.0.lock().unwrap_or_else(|p| p.into_inner());
    summary.batches = st.batches;
    summary.largest_batch = st.largest;
    if summary.error.is_none() {
        summary.error = st.error.take();
    }
    drop(st);
    finish(scan, &mut summary);
    summary
}

/// The last `scan_progress`, and the counts into the summary.
fn finish(scan: &Scan<'_>, summary: &mut ScanSummary) {
    summary.cancelled = scan.cancel.load(Ordering::SeqCst);
    let now = scan.progress.now(true, summary.cancelled);
    summary.found = now.found;
    summary.done = now.done;
    summary.failed = now.failed;
    scan.progress.finish(summary.cancelled);
}

/// What the writes of a scan came to.
#[derive(Default)]
struct Stats(Mutex<StatsInner>);

#[derive(Default)]
struct StatsInner {
    batches: u32,
    largest: u32,
    error: Option<AppError>,
}

/// Where a scan's ops go.
enum Sink {
    /// To the batcher, which commits them on the writer thread.
    Db(mpsc::Sender<Op>),
    /// Straight into the overlay: a view-only album.
    Memory,
}

/// Collects ops and commits them in one transaction on the writer thread
/// when [`ScanEnv::batch_rows`] wait or [`ScanEnv::batch_wait`] has passed
/// since the first, whichever comes first. A typed change sent to the
/// writer meanwhile waits for at most one batch.
fn batch_writes(scan: &Scan<'_>, ops: mpsc::Receiver<Op>, changes: &Changes<'_>, stats: &Stats) {
    let mut waiting: Vec<Op> = Vec::new();
    let mut first_at: Option<Instant> = None;
    loop {
        let next = match first_at {
            Some(t) => {
                let left = scan.env.batch_wait.saturating_sub(t.elapsed());
                ops.recv_timeout(left)
            }
            None => ops.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        let ended = match next {
            Ok(op) => {
                if waiting.is_empty() {
                    first_at = Some(Instant::now());
                }
                waiting.push(op);
                if waiting.len() < scan.env.batch_rows {
                    continue;
                }
                false
            }
            Err(RecvTimeoutError::Timeout) => false,
            Err(RecvTimeoutError::Disconnected) => true,
        };
        commit(scan, std::mem::take(&mut waiting), changes, stats);
        first_at = None;
        if ended {
            return;
        }
    }
}

fn commit(scan: &Scan<'_>, ops: Vec<Op>, changes: &Changes<'_>, stats: &Stats) {
    if ops.is_empty() || stats.0.lock().is_ok_and(|s| s.error.is_some()) {
        return;
    }
    let n = u32::try_from(ops.len()).unwrap_or(u32::MAX);
    // Through write_photos, so the album index reads the changed rows again
    // before the next batch (docs/05-organising.md, "Loading and keeping in
    // step").
    let result = scan.album.write_photos(move |tx, touched| {
        let mut rekeyed = Vec::new();
        for op in &ops {
            let applied = rows::apply(tx, op)?;
            rekeyed.extend(applied.rekeyed);
            touched.photos(applied.photo);
        }
        Ok(rekeyed)
    });
    let mut s = stats.0.lock().unwrap_or_else(|p| p.into_inner());
    match result {
        Ok(rekeyed) => {
            s.batches += 1;
            s.largest = s.largest.max(n);
            drop(s);
            changes.changed(rekeyed);
        }
        Err(e) => {
            // The album turned view-only (its lock was taken over) or the
            // database failed: stop, and write nothing more.
            s.error = Some(e);
            scan.cancel.store(true, Ordering::SeqCst);
        }
    }
}

/// Files per scan thread read in parallel before their rows are written,
/// in walker order.
const FACTS_CHUNK: usize = 16;

/// How the compare stage classes a file that has a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    /// Nothing is read.
    Unchanged,
    /// The facts are kept; the pixels pass runs.
    Waiting,
    /// Every scan fact is read again.
    Changed,
}

/// docs/03, "Compare".
fn class(row: &Known, file: &Listed, manual: bool, version: i64) -> Class {
    if row.size != file.size || row.mtime_ms != file.mtime_ms || row.scan_version != version {
        return Class::Changed;
    }
    match &row.scan_error {
        Some(e) if e.starts_with("io:") => Class::Changed,
        Some(_) if manual => Class::Changed,
        Some(_) => Class::Unchanged,
        None if row.waiting => Class::Waiting,
        None => Class::Unchanged,
    }
}

/// A file for the facts pass.
struct FactsJob<'w> {
    /// Walker order.
    index: usize,
    file: &'w Listed,
    /// The row it keeps, when it has one.
    row: Option<Known>,
    /// Its two ends, when move matching already read them.
    sample: Option<std::io::Result<Sample>>,
}

/// A row for the pixels pass.
#[derive(Debug, Clone)]
struct PixelsJob {
    /// Walker order, for the order of work.
    index: usize,
    rel_path: String,
    format: &'static MediaFormat,
    header: (u32, u32),
    size: u64,
    mtime_ms: i64,
    file_key: String,
    cost: Cost,
}

/// One scan's stages after the walk.
struct Work<'s, 'a> {
    scan: &'s Scan<'a>,
    request: Request,
    sink: Sink,
    changes: &'s Changes<'a>,
    stats: &'s Stats,
}

impl Work<'_, '_> {
    fn cancelled(&self) -> bool {
        self.scan.cancel.load(Ordering::SeqCst)
    }

    fn send(&self, op: Op) {
        match &self.sink {
            Sink::Db(ops) => {
                // The batcher only stops after this end of the channel is
                // dropped, so a send cannot fail.
                let _ = ops.send(op);
            }
            Sink::Memory => {
                let rekeyed = match &op {
                    Op::Facts {
                        id: Some(id),
                        rekey: true,
                        ..
                    } => Some(*id),
                    _ => None,
                };
                let mut overlay = self.scan.overlay.lock().unwrap_or_else(|p| p.into_inner());
                let applied = self.scan.album.db().read(|conn| {
                    let Some(id) = overlay.apply(conn, &op)? else {
                        return Ok(None);
                    };
                    match overlay.get(id) {
                        Some(row) => Ok(Some((id, row.index_facts(conn)?))),
                        None => Ok(None),
                    }
                });
                drop(overlay);
                match applied {
                    Ok(row) => {
                        // The album index follows memory as it follows the
                        // database.
                        match row {
                            Some((_, Some(facts))) => self.scan.album.index().upsert(&[facts]),
                            Some((id, None)) => self.scan.album.index().remove(&[id]),
                            None => {}
                        }
                        self.changes.changed(rekeyed);
                    }
                    Err(e) => {
                        let mut s = self.stats.0.lock().unwrap_or_else(|p| p.into_inner());
                        s.error.get_or_insert(e);
                        self.scan.cancel.store(true, Ordering::SeqCst);
                    }
                }
            }
        }
    }

    /// The rows in scope, from the database with memory's rows over them.
    fn known(&self) -> AppResult<Vec<Known>> {
        let scope = &self.scan.album.location().scope;
        let mut rows = self
            .scan
            .album
            .db()
            .read(|conn| rows::load_known(conn, scope))?;
        let overlay = self.scan.overlay.lock().unwrap_or_else(|p| p.into_inner());
        if !overlay.is_empty() {
            let mut by_id: HashMap<i64, Known> = rows.into_iter().map(|k| (k.id, k)).collect();
            for row in overlay.rows() {
                if walk::is_under(&row.rel_path, scope) {
                    by_id.insert(row.id, row.known());
                } else {
                    by_id.remove(&row.id);
                }
            }
            rows = by_id.into_values().collect();
        }
        Ok(rows)
    }

    fn stages(&self, summary: &mut ScanSummary) {
        let env = self.scan.env;
        let progress = self.scan.progress;
        let known = match self.known() {
            Ok(k) => k,
            Err(e) => {
                summary.error = Some(e);
                return;
            }
        };
        let walk = &summary.walk;
        let mut by_path: HashMap<&str, &Known> =
            known.iter().map(|k| (k.rel_path.as_str(), k)).collect();

        // Compare.
        let mut facts_jobs: Vec<FactsJob<'_>> = Vec::new();
        let mut pixels_jobs: Vec<PixelsJob> = Vec::new();
        let mut new_files: Vec<usize> = Vec::new();
        for (index, file) in walk.files.iter().enumerate() {
            match by_path.remove(file.rel_path.as_str()) {
                Some(row) => self.found(row, file, index, None, &mut facts_jobs, &mut pixels_jobs),
                None => new_files.push(index),
            }
        }
        // What is left in `by_path` is gone. Rows under a folder the walk
        // could not read, and rows of files stored online only, are not
        // gone, and are no one's candidates.
        let mut gone: Vec<&Known> = by_path
            .into_values()
            .filter(|k| !walk.is_under_unreadable(&k.rel_path) && !walk.is_online_only(&k.rel_path))
            .collect();
        gone.sort_by(|a, b| natural_cmp(&a.rel_path, &b.rel_path));
        progress.reading();

        // Match moves: keys for new files, read in parallel; pairing in
        // walker order.
        let root = &self.scan.album.location().root;
        let samples = parallel(env.threads, &new_files, self.scan.cancel, None, |&i| {
            env.reader
                .sample(&walk::absolute(root, &walk.files[i].rel_path))
        });
        let mut by_key: HashMap<&str, Vec<usize>> = HashMap::new();
        for (c, k) in gone.iter().enumerate() {
            if !keys::is_unreadable_key(&k.file_key) {
                by_key.entry(k.file_key.as_str()).or_default().push(c);
            }
        }
        let mut taken = vec![false; gone.len()];
        let mut moved = 0;
        for (&index, sample) in new_files.iter().zip(samples) {
            let Some(sample) = sample else {
                // Cancelled before its key was read.
                continue;
            };
            let file = &walk.files[index];
            let candidate = sample
                .as_ref()
                .ok()
                .filter(|s| !keys::is_unreadable_key(&s.file_key))
                .and_then(|s| by_key.get(s.file_key.as_str()))
                .and_then(|cands| pick(cands, &gone, &taken, rows::file_name(&file.rel_path)));
            match candidate {
                Some(c) => {
                    taken[c] = true;
                    let row = gone[c];
                    self.send(Op::Move {
                        id: row.id,
                        rel_path: file.rel_path.clone(),
                    });
                    moved += 1;
                    let moved_row = Known {
                        rel_path: file.rel_path.clone(),
                        missing: false,
                        ..row.clone()
                    };
                    self.found(
                        &moved_row,
                        file,
                        index,
                        Some(sample),
                        &mut facts_jobs,
                        &mut pixels_jobs,
                    );
                }
                None => facts_jobs.push(FactsJob {
                    index,
                    file,
                    row: None,
                    sample: Some(sample),
                }),
            }
        }
        summary.moved = moved;
        if self.cancelled() {
            return;
        }

        // Facts pass. Files are read in parallel a chunk at a time and
        // written in walker order, so new rows get their ids in that order.
        facts_jobs.sort_by_key(|j| j.index);
        let facts_jobs: Vec<Mutex<FactsJob<'_>>> = facts_jobs.into_iter().map(Mutex::new).collect();
        let (mut added, mut changed, mut rekeyed) = (0, 0, Vec::new());
        for chunk in facts_jobs.chunks(env.threads.max(1) * FACTS_CHUNK) {
            let read = parallel(env.threads, chunk, self.scan.cancel, None, |job| {
                let mut job = job.lock().unwrap_or_else(|p| p.into_inner());
                self.facts(&mut job)
            });
            for (op, outcome) in read.into_iter().flatten() {
                self.send(op);
                match outcome.pixels {
                    Some(job) => pixels_jobs.push(job),
                    None => progress.done(1, 1),
                }
                if outcome.added {
                    added += 1;
                } else {
                    changed += 1;
                }
                rekeyed.extend(outcome.rekeyed);
            }
        }
        summary.added = added;
        summary.changed = changed;
        summary.rekeyed = rekeyed;
        if self.cancelled() {
            return;
        }

        // Pixels pass: cheap handlers first, then walker order.
        pixels_jobs.sort_by_key(|j| (j.cost, j.index));
        let heavy = (env.threads / 2).max(1);
        let is_heavy = |j: &PixelsJob| j.cost == Cost::Heavy;
        parallel(
            env.threads,
            &pixels_jobs,
            self.scan.cancel,
            Some((&is_heavy, heavy)),
            |job| self.pixels(job),
        );
        if self.cancelled() {
            return;
        }

        // Finish: rows still gone are missing, but only after a whole walk.
        if walk.complete {
            let mut missing = 0;
            for (c, row) in gone.iter().enumerate() {
                if !taken[c] && !row.missing {
                    self.send(Op::Missing { id: row.id });
                    missing += 1;
                }
            }
            summary.missing = missing;
        }
    }

    /// A listed file that has a row (found by path, or given one by move
    /// matching).
    fn found<'w>(
        &self,
        row: &Known,
        file: &'w Listed,
        index: usize,
        sample: Option<std::io::Result<Sample>>,
        facts_jobs: &mut Vec<FactsJob<'w>>,
        pixels_jobs: &mut Vec<PixelsJob>,
    ) {
        let mut class = class(row, file, self.request.manual, self.scan.env.scan_version);
        let format = media::format_by_id(&row.format);
        if class == Class::Waiting && format.is_none() {
            class = Class::Changed;
        }
        match (class, format) {
            (Class::Unchanged, _) => {
                if row.missing {
                    self.send(Op::Unmiss { id: row.id });
                }
                self.scan.progress.done(1, 0);
            }
            (Class::Waiting, Some(format)) => {
                if row.missing {
                    self.send(Op::Unmiss { id: row.id });
                }
                pixels_jobs.push(PixelsJob {
                    index,
                    rel_path: file.rel_path.clone(),
                    format,
                    header: (row.width, row.height),
                    size: row.size,
                    mtime_ms: row.mtime_ms,
                    file_key: row.file_key.clone(),
                    cost: format.handler.cost(row.size),
                });
            }
            _ => facts_jobs.push(FactsJob {
                index,
                file,
                row: Some(row.clone()),
                sample,
            }),
        }
    }

    /// The facts pass for one file: its row's write, and whether the pixels
    /// pass follows.
    fn facts(&self, job: &mut FactsJob<'_>) -> (Op, FactsOutcome) {
        let mut first = Some(job.sample.take());
        let file = job.file;
        let read = busy_retry(
            self.scan,
            &file.rel_path,
            |r: &Read| r.error,
            || self.read_facts(file, first.take().flatten()),
        );
        let rekey = job.row.as_ref().is_some_and(|row| {
            !keys::is_unreadable_key(&row.file_key)
                && !keys::is_unreadable_key(&read.facts.file_key)
                && row.file_key != read.facts.file_key
        });
        let id = job.row.as_ref().map(|r| r.id);
        let pixels = read.format.map(|format| PixelsJob {
            index: job.index,
            rel_path: file.rel_path.clone(),
            format,
            header: (read.facts.width, read.facts.height),
            size: read.facts.size,
            mtime_ms: read.facts.mtime_ms,
            file_key: read.facts.file_key.clone(),
            cost: format.handler.cost(read.facts.size),
        });
        let op = Op::Facts {
            id,
            rel_path: file.rel_path.clone(),
            facts: Box::new(read.facts),
            rekey,
        };
        let outcome = FactsOutcome {
            pixels,
            added: id.is_none(),
            rekeyed: if rekey { id } else { None },
        };
        (op, outcome)
    }

    /// Reads one file's facts. `sample` is its two ends when already read.
    /// The scan facts that come from metadata (date, place, camera, the
    /// `exif` JSON) are read here by #10.
    fn read_facts(&self, file: &Listed, sample: Option<std::io::Result<Sample>>) -> Read {
        let env = self.scan.env;
        let rel = &file.rel_path;
        let name = rows::file_name(rel);
        let path = walk::absolute(&self.scan.album.location().root, rel);
        let named = media::format_of(name);
        let sample = match sample.unwrap_or_else(|| env.reader.sample(&path)) {
            Ok(s) => s,
            Err(e) => {
                let key = keys::unreadable_file_key(file.size);
                return self.error_read(named, file.size, file.mtime_ms, key, &MediaError::io(&e));
            }
        };
        // Stat again: a file that changed since the walk is read as it is
        // now.
        let mtime_ms = std::fs::metadata(&path).map_or(file.mtime_ms, |m| walk::mtime_ms(&m));
        let format = match media::identify(name, &sample.head) {
            Some(Identified::Format(f)) => f,
            Some(Identified::Unsupported(u)) => {
                let e = MediaError::unsupported(u.reason);
                return self.error_read(named, sample.size, mtime_ms, sample.file_key, &e);
            }
            None => {
                // Only listed (supported) names reach here.
                let e = MediaError::decode("");
                return self.error_read(named, sample.size, mtime_ms, sample.file_key, &e);
            }
        };
        let read = env.reader.metadata(format, &path).and_then(|m| {
            let mut facts = (m.width, m.height, m.duration_ms);
            if format.kind == MediaKind::Video
                && let Some(v) = env.reader.probe(format, &path)?
            {
                facts = (v.width, v.height, Some(v.duration_ms));
            }
            Ok(facts)
        });
        match read {
            Ok((width, height, duration_ms)) => Read {
                facts: Facts {
                    size: sample.size,
                    mtime_ms,
                    thumb_key: keys::thumb_key(
                        self.scan.album.db().album_id(),
                        &sample.file_key,
                        mtime_ms,
                    ),
                    file_key: sample.file_key,
                    kind: format.kind,
                    format: format.id.to_owned(),
                    width,
                    height,
                    duration_ms,
                    captured_at: None,
                    gps_lat: None,
                    gps_lon: None,
                    camera: None,
                    exif: None,
                    analysis: None,
                    scan_version: env.scan_version,
                    scan_error: None,
                },
                format: Some(format),
                error: None,
            },
            Err(e) => self.error_read(named, sample.size, mtime_ms, sample.file_key, &e),
        }
    }

    /// An error row as the facts pass reads it.
    fn error_read(
        &self,
        named: Option<&'static MediaFormat>,
        size: u64,
        mtime_ms: i64,
        file_key: String,
        error: &MediaError,
    ) -> Read {
        Read {
            facts: self.error_facts(named, size, mtime_ms, file_key, error),
            format: None,
            error: Some(error.code),
        }
    }

    /// An error row's placeholders (docs/03, "Errors per file"): `kind` and
    /// `format` from the extension's registry row, no size, every other
    /// scan fact NULL.
    fn error_facts(
        &self,
        named: Option<&'static MediaFormat>,
        size: u64,
        mtime_ms: i64,
        file_key: String,
        error: &MediaError,
    ) -> Facts {
        Facts {
            size,
            mtime_ms,
            thumb_key: keys::thumb_key(self.scan.album.db().album_id(), &file_key, mtime_ms),
            file_key,
            kind: named.map_or(MediaKind::Image, |f| f.kind),
            format: named.map_or("", |f| f.id).to_owned(),
            width: 0,
            height: 0,
            duration_ms: None,
            captured_at: None,
            gps_lat: None,
            gps_lon: None,
            camera: None,
            exif: None,
            analysis: None,
            scan_version: self.scan.env.scan_version,
            scan_error: Some(error.scan_error()),
        }
    }

    /// The pixels pass for one row: one decode, then the thumbnails and the
    /// analysis from it.
    fn pixels(&self, job: &PixelsJob) {
        let path = walk::absolute(&self.scan.album.location().root, &job.rel_path);
        let decoded = busy_retry(
            self.scan,
            &job.rel_path,
            |r: &Result<Pixels, MediaError>| r.as_ref().err().map(|e| e.code),
            || self.scan.env.reader.preview(job.format, &path),
        );
        match decoded {
            Ok(px) => {
                if let Some(thumbs) = &self.scan.env.thumbs {
                    let album_id = self.scan.album.db().album_id();
                    thumbs.write_from_scan(
                        &keys::thumb_key(album_id, &job.file_key, job.mtime_ms),
                        &px,
                    );
                }
                self.send(Op::Pixels {
                    rel_path: job.rel_path.clone(),
                    analysis: pixels::analysis(&px),
                    size: pixels::corrected_size(job.header, &px),
                });
                self.scan.progress.done(1, 0);
            }
            Err(e) => {
                let named = media::format_of(rows::file_name(&job.rel_path));
                let facts =
                    self.error_facts(named, job.size, job.mtime_ms, job.file_key.clone(), &e);
                self.send(Op::Failed {
                    rel_path: job.rel_path.clone(),
                    facts: Box::new(facts),
                });
                self.scan.progress.done(1, 1);
            }
        }
    }
}

/// One file's facts as read: an ordinary row (with the format to decode
/// next) or an error row.
struct Read {
    facts: Facts,
    format: Option<&'static MediaFormat>,
    error: Option<MediaErrorCode>,
}

struct FactsOutcome {
    pixels: Option<PixelsJob>,
    added: bool,
    rekeyed: Option<i64>,
}

/// Among the candidate rows for one key, the one a new file takes: the
/// first not yet taken with the same file name, else the first in path
/// order. `cands` are in path order.
fn pick(cands: &[usize], gone: &[&Known], taken: &[bool], name: &str) -> Option<usize> {
    let free = || cands.iter().copied().filter(|&c| !taken[c]);
    free()
        .find(|&c| gone[c].file_name() == name)
        .or_else(|| free().next())
}

/// Runs `read` again while it fails with `decode` and the file was modified
/// within [`ScanEnv::busy_recent`]: it is taken to be still being written
/// and is tried after [`ScanEnv::busy_wait`], at most
/// [`ScanEnv::busy_tries`] more times. After that the error stands.
fn busy_retry<T>(
    scan: &Scan<'_>,
    rel_path: &str,
    error: impl Fn(&T) -> Option<MediaErrorCode>,
    mut read: impl FnMut() -> T,
) -> T {
    let path = walk::absolute(&scan.album.location().root, rel_path);
    let mut tries = 0;
    loop {
        let result = read();
        let busy = error(&result) == Some(MediaErrorCode::Decode)
            && tries < scan.env.busy_tries
            && !scan.cancel.load(Ordering::SeqCst)
            && recently_modified(&path, scan.env.busy_recent);
        if !busy {
            return result;
        }
        tries += 1;
        std::thread::sleep(scan.env.busy_wait);
    }
}

fn recently_modified(path: &std::path::Path, within: Duration) -> bool {
    let Ok(modified) = std::fs::metadata(path).and_then(|m| m.modified()) else {
        return false;
    };
    // A time in the future counts as just now.
    SystemTime::now()
        .duration_since(modified)
        .map_or(true, |age| age < within)
}

/// Which items are heavy, and how many of them may run at once.
type Limit<'f, T> = (&'f (dyn Fn(&T) -> bool + Sync), usize);

/// Runs `f` over `items` on `threads` threads, taking items in order, and
/// answers each result in the items' order: None for an item skipped
/// because `cancel` was set. `heavy` limits how many of the items it
/// picks run at once.
fn parallel<T: Sync, R: Send>(
    threads: usize,
    items: &[T],
    cancel: &AtomicBool,
    heavy: Option<Limit<'_, T>>,
    f: impl Fn(&T) -> R + Sync,
) -> Vec<Option<R>> {
    let results: Vec<Mutex<Option<R>>> = items.iter().map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    let running_heavy = Mutex::new(0usize);
    let heavy_free = Condvar::new();
    std::thread::scope(|s| {
        for _ in 0..threads.max(1).min(items.len()) {
            s.spawn(|| {
                loop {
                    if cancel.load(Ordering::SeqCst) {
                        return;
                    }
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(item) = items.get(i) else {
                        return;
                    };
                    let limited = heavy.filter(|(is_heavy, _)| is_heavy(item));
                    if let Some((_, max)) = limited {
                        let mut n = running_heavy.lock().unwrap_or_else(|p| p.into_inner());
                        while *n >= max {
                            n = heavy_free.wait(n).unwrap_or_else(|p| p.into_inner());
                        }
                        *n += 1;
                    }
                    let r = f(item);
                    if limited.is_some() {
                        *running_heavy.lock().unwrap_or_else(|p| p.into_inner()) -= 1;
                        heavy_free.notify_one();
                    }
                    *results[i].lock().unwrap_or_else(|p| p.into_inner()) = Some(r);
                }
            });
        }
    });
    results
        .into_iter()
        .map(|m| m.into_inner().unwrap_or_else(|p| p.into_inner()))
        .collect()
}
