//! The tagging thread and its queue (docs/07-tagging.md, "The tagging
//! queue", "Loading and unloading", "Status and the `tagging_progress`
//! event").
//!
//! One thread serves the open album: it loads the active models, tags one
//! photo at a time (the focus list first, then scan order), sends the rows
//! to the album's writer, and keeps the counts the page sees. Everything
//! that runs a model goes through a [`TagBackend`], so the queue is tested
//! with the fake one.

use std::collections::{BTreeSet, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use image::RgbImage;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::backend::{BackendError, BackendFactory, Deadline, TagBackend, TagImage, TextEmbedder};
use super::catalogue::{ModelChoice, Part};
use super::install;
use super::results;
use super::store::{self, Status, TagRow};
use super::vocabulary::VOCABULARY;
use crate::album::{Album, AlbumChanged, ChangeReason};
use crate::error::{AppError, AppResult, ErrorCode};
use crate::events::{Emitter, Event, emit};
use crate::text::tagging as words;

/// Three losses in a row stop a part.
pub const MAX_LOSSES: u32 = 3;
/// The most ids kept from a focus list.
pub const MAX_FOCUS: usize = 1000;

/// Time limits of "Timeouts", and the idle unload. Tests shorten them.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub load: Duration,
    pub describe: Duration,
    pub keywords: Duration,
    /// One sampled caption (Regenerate).
    pub caption: Duration,
    /// A run that has not returned this long after it was asked to end
    /// writes the tagging thread off ("Timeouts", step 3).
    pub stuck: Duration,
    pub idle_unload: Duration,
    /// `album_changed { reason: "tags" }` at most this often while tagging.
    pub tags_event: Duration,
    /// `tagging_progress` at most this often while only the counts move.
    pub progress_event: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            load: Duration::from_secs(180),
            // A caption and the object detection, 120 s each.
            describe: Duration::from_secs(240),
            keywords: Duration::from_secs(30),
            caption: Duration::from_secs(120),
            stuck: Duration::from_secs(30),
            idle_unload: Duration::from_secs(300),
            tags_event: Duration::from_secs(5),
            progress_event: Duration::from_millis(250),
        }
    }
}

/// The `tagging_progress` event, and `tagging_status`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TaggingProgress {
    /// Eligible photos, when a model is active; else 0.
    pub total: u64,
    /// Of those, photos where every active model has a done row.
    pub done: u64,
    /// Of those, photos where some active model has a failed row.
    pub failed: u64,
    pub paused: bool,
    /// Tagging, or loading a model in order to tag.
    pub running: bool,
    /// Parts whose model is damaged, would not load, or kept failing.
    pub not_working: Vec<Part>,
    /// Why tagging stopped by itself.
    pub problem: Option<String>,
}

impl Event for TaggingProgress {
    const NAME: &'static str = "tagging_progress";
}

/// What reading a photo's picture gave.
pub enum Picture {
    /// The medium thumbnail, and the file name.
    Ready(RgbImage, String),
    /// The thumbnail could not be made or decoded.
    Unreadable(String),
    /// The file is no longer there: skip it.
    Gone,
}

/// Where the tagging thread gets pictures.
pub trait Pictures: Send + Sync {
    fn picture(&self, id: i64) -> Picture;
}

/// The open album, as the tagging thread sees it.
#[derive(Clone)]
pub struct AlbumWork {
    pub album: Arc<Album>,
    pub pictures: Arc<dyn Pictures>,
    /// True while a full scan runs; tagging waits for it.
    pub scanning: Arc<dyn Fn() -> bool + Send + Sync>,
}

/// The answer to a Regenerate, taken by whoever answers first: the tagging
/// thread, or the watchdog when it writes the thread off.
type Reply = Arc<Mutex<Option<mpsc::Sender<AppResult<()>>>>>;

fn answer(reply: &Reply, result: AppResult<()>) {
    if let Some(tx) = reply.lock().unwrap_or_else(|p| p.into_inner()).take() {
        let _ = tx.send(result);
    }
}

fn stopped() -> AppError {
    AppError::new(ErrorCode::TaggingStopped, words::TAGGING_STOPPED)
}

/// A Regenerate waiting for the tagging thread.
struct Job {
    id: i64,
    model: &'static ModelChoice,
    reply: Reply,
}

/// What the tagging thread is running now, for the watchdog.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RunKind {
    Load,
    Photo,
    Regenerate,
}

#[derive(Clone)]
struct InRun {
    epoch: u64,
    generation: u64,
    kind: RunKind,
    id: i64,
    model: &'static ModelChoice,
    /// When the run was asked to end: its deadline, or earlier when it was
    /// told to stop.
    ends: Instant,
    reply: Option<Reply>,
}

/// The run in hand is told to end now (the album closed, Owlbum closes).
fn ask_to_end(s: &mut State) {
    let now = Instant::now();
    if let Some(r) = s.in_run.as_mut() {
        r.ends = r.ends.min(now);
    }
}

struct State {
    album: Option<AlbumWork>,
    /// Bumped when the tagging thread is written off; a thread whose epoch
    /// is not this one ends and writes nothing.
    epoch: u64,
    in_run: Option<InRun>,
    jobs: VecDeque<Job>,
    /// Bumped on every album open and close.
    generation: u64,
    active: [Option<&'static ModelChoice>; 2],
    paused: bool,
    problem: Option<String>,
    not_working: BTreeSet<Part>,
    losses: [u32; 2],
    focus: VecDeque<i64>,
    /// The ids that need work; None until built.
    list: Option<BTreeSet<i64>>,
    counts: Option<(u64, u64, u64)>,
    running: bool,
    /// Set to end the run in hand (album closed).
    stop: Arc<AtomicBool>,
    /// Parts to unload on the thread at its next turn.
    unload: BTreeSet<Part>,
    shutdown: bool,
    /// The scan was running at the last look.
    was_scanning: bool,
    last_sent: Option<TaggingProgress>,
    last_progress_at: Option<Instant>,
    tags_pending: bool,
    last_tags_at: Option<Instant>,
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    emitter: Arc<dyn Emitter>,
    factory: BackendFactory,
    models: PathBuf,
    threads: usize,
    limits: Limits,
    text: Mutex<Option<(&'static str, Arc<dyn TextEmbedder>)>>,
    /// The tagging thread now.
    thread: Mutex<Option<JoinHandle<()>>>,
}

/// The tagging thread and its queue.
pub struct Runtime {
    shared: Arc<Shared>,
    /// The watchdog that writes a stuck tagging thread off.
    watch: Mutex<Option<JoinHandle<()>>>,
}

impl Runtime {
    /// A runtime whose thread starts the first time it is needed.
    pub fn new(
        factory: BackendFactory,
        models: PathBuf,
        threads: usize,
        emitter: Arc<dyn Emitter>,
        limits: Limits,
    ) -> Self {
        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(State {
                    album: None,
                    epoch: 0,
                    in_run: None,
                    jobs: VecDeque::new(),
                    generation: 0,
                    active: [None, None],
                    paused: false,
                    problem: None,
                    not_working: BTreeSet::new(),
                    losses: [0, 0],
                    focus: VecDeque::new(),
                    list: None,
                    counts: None,
                    running: false,
                    stop: Arc::new(AtomicBool::new(false)),
                    unload: BTreeSet::new(),
                    shutdown: false,
                    was_scanning: false,
                    last_sent: None,
                    last_progress_at: None,
                    tags_pending: false,
                    last_tags_at: None,
                }),
                wake: Condvar::new(),
                emitter,
                factory,
                models,
                threads,
                limits,
                text: Mutex::new(None),
                thread: Mutex::new(None),
            }),
            watch: Mutex::new(None),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.shared.lock()
    }

    fn ensure_thread(&self) {
        if self.shared.lock().shutdown {
            return;
        }
        let none = self
            .shared
            .thread
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .is_none();
        if none {
            spawn_worker(&self.shared);
        }
        let mut w = self.watch.lock().unwrap_or_else(|p| p.into_inner());
        if w.is_none() {
            let shared = Arc::clone(&self.shared);
            *w = std::thread::Builder::new()
                .name("owlbum-tagging-watch".into())
                .spawn(move || watch(&shared))
                .ok();
        }
    }

    /// Changes something and wakes the thread; sends the status at once.
    fn change(&self, f: impl FnOnce(&mut State)) {
        {
            let mut s = self.state();
            f(&mut s);
        }
        self.ensure_thread();
        self.shared.wake.notify_all();
        self.shared.publish(true);
    }

    /// The open album, or none. Clears the pause, the focus list and the
    /// problem; the run in hand is told to end and its result is dropped.
    pub fn attach(&self, album: Option<AlbumWork>) {
        let jobs = {
            let mut s = self.state();
            std::mem::take(&mut s.jobs)
        };
        for job in jobs {
            answer(&job.reply, Err(stopped()));
        }
        self.change(|s| {
            s.stop.store(true, Ordering::Relaxed);
            ask_to_end(s);
            s.stop = Arc::new(AtomicBool::new(false));
            s.generation += 1;
            s.album = album;
            s.paused = false;
            s.problem = None;
            s.focus.clear();
            s.list = None;
            s.counts = None;
            s.tags_pending = false;
        });
    }

    /// The models that run. A part whose model changed is unloaded, and the
    /// list and counts are built again.
    pub fn set_active(&self, active: [Option<&'static ModelChoice>; 2]) {
        self.change(|s| {
            for part in Part::ALL {
                if s.active[part.index()].map(|m| m.id) != active[part.index()].map(|m| m.id) {
                    s.unload.insert(part);
                }
            }
            s.active = active;
            s.list = None;
            s.counts = None;
        });
    }

    /// Clears the not-working set and the loss counts (a new choice, a
    /// finished download, a removed model).
    pub fn clear_not_working(&self) {
        self.change(|s| {
            s.not_working.clear();
            s.losses = [0, 0];
        });
    }

    /// Unloads a part's model now, if it is loaded (a removed model).
    pub fn unload(&self, part: Part) {
        self.change(|s| {
            s.unload.insert(part);
        });
    }

    /// `tagging_pause`.
    pub fn pause(&self, paused: bool) {
        self.change(|s| s.paused = paused);
    }

    /// `tagging_restart`: clears the problem, the not-working set and the
    /// loss counts, then starts the queue.
    pub fn restart(&self) {
        self.change(|s| {
            s.problem = None;
            s.not_working.clear();
            s.losses = [0, 0];
            s.list = None;
        });
    }

    /// `tagging_focus`: the selected photo, then the photos on screen.
    pub fn focus(&self, ids: &[i64]) {
        let mut s = self.state();
        s.focus = ids.iter().copied().take(MAX_FOCUS).collect();
        drop(s);
        self.shared.wake.notify_all();
    }

    /// The album's rows changed (a scan, the watcher): the list and the
    /// counts are read again.
    pub fn album_changed(&self) {
        {
            let mut s = self.state();
            s.list = None;
            s.counts = None;
        }
        self.ensure_thread();
        self.shared.wake.notify_all();
    }

    /// `tagging_retry_failed`: deletes the failed rows of the active models
    /// for the eligible photos and their copies. Answers how many photos
    /// were cleared.
    pub fn retry_failed(&self) -> crate::error::AppResult<u64> {
        let (album, models) = {
            let s = self.state();
            let models: Vec<String> = s.active.iter().flatten().map(|m| m.id.to_owned()).collect();
            (s.album.clone(), models)
        };
        let Some(work) = album else {
            return Ok(0);
        };
        let scope = work.album.location().scope.clone();
        let cleared = work.album.db().write(move |tx| {
            let refs: Vec<&str> = models.iter().map(String::as_str).collect();
            store::retry_failed(tx, &refs, &scope)
        })?;
        self.change(|s| {
            s.list = None;
            s.counts = None;
            s.tags_pending = true;
            s.last_tags_at = None;
        });
        self.shared.flush_tags(true);
        Ok(cleared)
    }

    /// The status, as the event carries it. Reading it changes nothing.
    pub fn status(&self) -> TaggingProgress {
        self.shared.progress(&self.state())
    }

    /// The loaded keywords model's text side, and its id.
    pub fn text_embedder(&self) -> Option<(&'static str, Arc<dyn TextEmbedder>)> {
        self.shared
            .text
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// The active model of a part: chosen, installed and working.
    pub fn active(&self, part: Part) -> Option<&'static ModelChoice> {
        let s = self.state();
        s.active[part.index()].filter(|_| !s.not_working.contains(&part))
    }

    /// Regenerate (docs/07-tagging.md, "Regenerate"): a new sampled
    /// caption for one photo from `model`, run on the tagging thread after
    /// the photo in hand and before the next one, even while paused.
    /// Returns when the new sentence is written, or with the error.
    pub fn regenerate(&self, id: i64, model: &'static ModelChoice) -> AppResult<()> {
        let (tx, rx) = mpsc::channel();
        let reply: Reply = Arc::new(Mutex::new(Some(tx)));
        self.change(|s| {
            s.jobs.push_back(Job {
                id,
                model,
                reply: Arc::clone(&reply),
            });
        });
        if self.state().shutdown {
            answer(&reply, Err(stopped()));
        }
        rx.recv().unwrap_or_else(|_| Err(stopped()))
    }

    /// Stops the thread and unloads the models (Owlbum closes). Waits for
    /// the run in hand to end, at most `stuck`; a thread still running
    /// then is left behind.
    pub fn shutdown(&self) {
        let jobs = {
            let mut s = self.state();
            s.shutdown = true;
            s.stop.store(true, Ordering::Relaxed);
            ask_to_end(&mut s);
            std::mem::take(&mut s.jobs)
        };
        for job in jobs {
            answer(&job.reply, Err(stopped()));
        }
        self.shared.wake.notify_all();
        if let Some(w) = self.watch.lock().unwrap_or_else(|p| p.into_inner()).take() {
            let _ = w.join();
        }
        let thread = self
            .shared
            .thread
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take();
        if let Some(t) = thread {
            let start = Instant::now();
            while !t.is_finished() && start.elapsed() < self.shared.limits.stuck {
                std::thread::sleep(Duration::from_millis(5));
            }
            if t.is_finished() {
                let _ = t.join();
            } else {
                tracing::warn!(target: "tagging", "the tagging thread did not end after it was asked to; left behind");
            }
        }
    }
}

impl std::fmt::Debug for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runtime")
            .field("status", &self.status())
            .finish()
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn progress(&self, s: &State) -> TaggingProgress {
        let any_active = s.active.iter().any(Option::is_some);
        let counted = s.album.as_ref().is_some_and(|a| !a.album.is_view_only()) && any_active;
        let (total, done, failed) = if counted {
            s.counts.unwrap_or_default()
        } else {
            (0, 0, 0)
        };
        TaggingProgress {
            total,
            done,
            failed,
            paused: s.paused,
            running: s.running,
            not_working: s.not_working.iter().copied().collect(),
            problem: s.problem.clone(),
        }
    }

    /// Sends `tagging_progress` when it changed: at once for a change of
    /// state, at most every `progress_event` when only the counts moved.
    fn publish(&self, force_counts: bool) {
        let mut s = self.lock();
        let now = self.progress(&s);
        if s.last_sent.as_ref() == Some(&now) {
            return;
        }
        let state_changed = s.last_sent.as_ref().is_none_or(|l| {
            l.paused != now.paused
                || l.running != now.running
                || l.not_working != now.not_working
                || l.problem != now.problem
        });
        let due = s
            .last_progress_at
            .is_none_or(|t| t.elapsed() >= self.limits.progress_event);
        if state_changed || force_counts || due {
            s.last_sent = Some(now.clone());
            s.last_progress_at = Some(Instant::now());
            drop(s);
            emit(&*self.emitter, &now);
        }
    }

    /// Sends `album_changed { reason: "tags" }` when one is waiting and it
    /// is due (or `now`).
    fn flush_tags(&self, now: bool) {
        let album = {
            let mut s = self.lock();
            if !s.tags_pending {
                return;
            }
            let due = s
                .last_tags_at
                .is_none_or(|t| t.elapsed() >= self.limits.tags_event);
            if !(now || due) {
                return;
            }
            s.tags_pending = false;
            s.last_tags_at = Some(Instant::now());
            s.album.as_ref().map(|a| Arc::clone(&a.album))
        };
        if let Some(album) = album {
            emit(
                &*self.emitter,
                &AlbumChanged {
                    revision: album.next_revision(),
                    reason: ChangeReason::Tags,
                    rekeyed: Vec::new(),
                },
            );
        }
    }

    fn set_text(&self, backend: &dyn TagBackend) {
        let id = backend.loaded(Part::Keywords);
        *self.text.lock().unwrap_or_else(|p| p.into_inner()) =
            id.and_then(|id| backend.text_embedder().map(|t| (id, t)));
    }
}

/// Starts a tagging thread for the current epoch.
fn spawn_worker(shared: &Arc<Shared>) {
    let epoch = shared.lock().epoch;
    let for_thread = Arc::clone(shared);
    let handle = std::thread::Builder::new()
        .name("owlbum-tagging".into())
        .spawn(move || run(&for_thread, epoch))
        .ok();
    *shared.thread.lock().unwrap_or_else(|p| p.into_inner()) = handle;
}

/// The watchdog ("Timeouts", step 3): a run that has not returned `stuck`
/// after it was asked to end writes its thread off. A new thread starts
/// with empty slots; the old one's result is ignored when it returns.
fn watch(shared: &Arc<Shared>) {
    let tick = (shared.limits.stuck / 4).clamp(Duration::from_millis(5), Duration::from_secs(1));
    loop {
        let mut s = shared.lock();
        if s.shutdown {
            return;
        }
        let late = s
            .in_run
            .as_ref()
            .is_some_and(|r| r.epoch == s.epoch && Instant::now() >= r.ends + shared.limits.stuck);
        if !late {
            let _ = shared.wake.wait_timeout(s, tick);
            continue;
        }
        let Some(r) = s.in_run.take() else { continue };
        s.epoch += 1;
        tracing::warn!(target: "tagging", "{} did not end {:?} after it was asked to; the tagging thread is written off", r.model.id, shared.limits.stuck);
        let part = r.model.part;
        let mut write = None;
        match r.kind {
            RunKind::Load => {
                s.not_working.insert(part);
            }
            RunKind::Photo => {
                s.losses[part.index()] += 1;
                if s.losses[part.index()] >= MAX_LOSSES {
                    s.not_working.insert(part);
                }
                if s.generation == r.generation {
                    write = s.album.clone();
                }
            }
            RunKind::Regenerate => {}
        }
        s.counts = None;
        s.running = false;
        drop(s);
        if let Some(reply) = &r.reply {
            answer(
                reply,
                Err(AppError::new(
                    ErrorCode::TaggingTimeout,
                    words::tagging_timeout(r.model.label),
                )),
            );
        }
        if let Some(work) = write {
            // As a timeout: a failed row for the photo.
            let row = TagRow::failed(part, r.model.id, words::TOOK_TOO_LONG, store::now_ms());
            let id = r.id;
            let _ = work
                .album
                .db()
                .write(move |tx| store::write(tx, id, &[row]));
            let mut s = shared.lock();
            s.tags_pending = true;
        }
        *shared.text.lock().unwrap_or_else(|p| p.into_inner()) = None;
        spawn_worker(shared);
        shared.wake.notify_all();
        shared.publish(true);
    }
}

/// Marks what the thread runs now, for the watchdog.
#[allow(clippy::too_many_arguments)]
fn running(
    shared: &Shared,
    epoch: u64,
    generation: u64,
    kind: RunKind,
    id: i64,
    model: &'static ModelChoice,
    limit: Duration,
    reply: Option<&Reply>,
) {
    let mut s = shared.lock();
    if s.epoch == epoch {
        s.in_run = Some(InRun {
            epoch,
            generation,
            kind,
            id,
            model,
            ends: Instant::now() + limit,
            reply: reply.cloned(),
        });
        if s.stop.load(Ordering::Relaxed) || s.shutdown {
            ask_to_end(&mut s);
        }
    }
}

/// The run in hand returned. False when this thread was written off
/// meanwhile: it must stop and write nothing.
fn returned(shared: &Shared, epoch: u64) -> bool {
    let mut s = shared.lock();
    if s.epoch != epoch {
        return false;
    }
    s.in_run = None;
    true
}

/// What the thread does next.
enum Next {
    Wait(Option<Duration>),
    Regenerate {
        work: AlbumWork,
        job: Job,
        generation: u64,
        stop: Arc<AtomicBool>,
    },
    Tag {
        work: AlbumWork,
        id: i64,
        models: Vec<&'static ModelChoice>,
        generation: u64,
        stop: Arc<AtomicBool>,
        first_of_focus: bool,
    },
    Exit,
}

/// The tagging thread.
fn run(shared: &Arc<Shared>, epoch: u64) {
    let mut backend: Option<Box<dyn TagBackend>> = None;
    let mut last_work = Instant::now();
    loop {
        if shared.lock().epoch != epoch {
            // Written off by the watchdog: a new thread has taken over.
            return;
        }
        // Unloads asked for.
        let unload: Vec<Part> = std::mem::take(&mut shared.lock().unload)
            .into_iter()
            .collect();
        if let Some(b) = backend.as_mut() {
            for part in unload {
                b.unload(part);
            }
            shared.set_text(&**b);
        }
        let next = pick(shared);
        if let Next::Tag { .. } | Next::Regenerate { .. } = next
            && backend.is_none()
        {
            match (shared.factory)() {
                Ok(b) => backend = Some(b),
                Err(reason) => {
                    let mut s = shared.lock();
                    s.problem = Some(words::runtime_failed(&reason));
                    s.not_working.extend(Part::ALL);
                    drop(s);
                    if let Next::Regenerate { job, .. } = &next {
                        answer(
                            &job.reply,
                            Err(AppError::new(
                                ErrorCode::ModelLoadFailed,
                                words::model_load_failed(job.model.label),
                            )),
                        );
                    }
                    shared.publish(true);
                    continue;
                }
            }
        }
        match next {
            Next::Exit => {
                if let Some(mut b) = backend.take() {
                    b.unload(Part::Describe);
                    b.unload(Part::Keywords);
                }
                *shared.text.lock().unwrap_or_else(|p| p.into_inner()) = None;
                return;
            }
            Next::Wait(limit) => {
                shared.flush_tags(true);
                shared.publish(false);
                let idle = last_work.elapsed();
                if idle >= shared.limits.idle_unload
                    && let Some(b) = backend.as_mut()
                    && (b.loaded(Part::Describe).is_some() || b.loaded(Part::Keywords).is_some())
                {
                    b.unload(Part::Describe);
                    b.unload(Part::Keywords);
                    shared.set_text(&**b);
                }
                let s = shared.lock();
                if s.shutdown {
                    continue;
                }
                let wait = limit.unwrap_or(Duration::from_secs(1)).min(
                    shared
                        .limits
                        .idle_unload
                        .saturating_sub(idle)
                        .max(Duration::from_millis(10)),
                );
                let _ = shared.wake.wait_timeout(s, wait);
            }
            Next::Regenerate {
                work,
                job,
                generation,
                stop,
            } => {
                last_work = Instant::now();
                let Some(b) = backend.as_mut() else { continue };
                let result = regenerate(shared, &mut **b, &work, &job, generation, &stop, epoch);
                if shared.lock().epoch != epoch {
                    return;
                }
                answer(&job.reply, result);
            }
            Next::Tag {
                work,
                id,
                models,
                generation,
                stop,
                first_of_focus,
            } => {
                last_work = Instant::now();
                let Some(b) = backend.as_mut() else { continue };
                tag(
                    shared,
                    &mut **b,
                    &work,
                    &Turn {
                        id,
                        generation,
                        epoch,
                        stop: &stop,
                        first_of_focus,
                    },
                    &models,
                );
            }
        }
    }
}

/// Chooses the next photo, or how long to wait.
fn pick(shared: &Shared) -> Next {
    let mut s = shared.lock();
    if s.shutdown {
        return Next::Exit;
    }
    let models: Vec<&'static ModelChoice> = Part::ALL
        .iter()
        .filter(|p| !s.not_working.contains(p))
        .filter_map(|p| s.active[p.index()])
        .collect();
    let Some(work) = s.album.clone() else {
        s.running = false;
        let jobs = std::mem::take(&mut s.jobs);
        drop(s);
        for job in jobs {
            answer(&job.reply, Err(stopped()));
        }
        return Next::Wait(None);
    };
    // Regenerate comes before the next photo, paused or not.
    if let Some(job) = s.jobs.pop_front() {
        return Next::Regenerate {
            work,
            job,
            generation: s.generation,
            stop: Arc::clone(&s.stop),
        };
    }
    let blocked = work.album.is_view_only() || models.is_empty() || s.problem.is_some();
    let scanning = (work.scanning)();
    if std::mem::replace(&mut s.was_scanning, scanning) && !scanning {
        // A scan finished: the list and the counts are read again.
        s.list = None;
        s.counts = None;
    }
    if s.counts.is_none() && !work.album.is_view_only() {
        // The counts are read when an album opens, a scan finishes or the
        // models change.
        let all: Vec<&str> = s.active.iter().flatten().map(|m| m.id).collect();
        let scope = work.album.location().scope.clone();
        drop(s);
        let counts = work
            .album
            .db()
            .read(|c| store::counts(c, &all, &scope))
            .unwrap_or_default();
        s = shared.lock();
        if s.album
            .as_ref()
            .is_some_and(|a| Arc::ptr_eq(&a.album, &work.album))
        {
            s.counts = Some(counts);
        }
    }
    if blocked || s.paused || scanning {
        s.running = false;
        return Next::Wait(Some(Duration::from_millis(500)));
    }
    if s.list.is_none() {
        let scope = work.album.location().scope.clone();
        let ids: Vec<&'static str> = models.iter().map(|m| m.id).collect();
        drop(s);
        let mut set = BTreeSet::new();
        for m in &ids {
            if let Ok(v) = work.album.db().read(|c| store::needs_work(c, m, &scope)) {
                set.extend(v);
            }
        }
        s = shared.lock();
        if !s
            .album
            .as_ref()
            .is_some_and(|a| Arc::ptr_eq(&a.album, &work.album))
        {
            return Next::Wait(Some(Duration::ZERO));
        }
        s.list = Some(set);
    }
    let list = s.list.clone().unwrap_or_default();
    let mut first_of_focus = true;
    let mut chosen = None;
    while let Some(&f) = s.focus.front() {
        if list.contains(&f) {
            chosen = Some(f);
            break;
        }
        s.focus.pop_front();
        first_of_focus = false;
    }
    let id = match chosen {
        Some(id) => {
            s.focus.pop_front();
            id
        }
        None => {
            first_of_focus = false;
            match list.first() {
                Some(&id) => id,
                None => {
                    s.running = false;
                    return Next::Wait(None);
                }
            }
        }
    };
    if let Some(l) = s.list.as_mut() {
        l.remove(&id);
    }
    s.running = true;
    let next = Next::Tag {
        work,
        id,
        models,
        generation: s.generation,
        stop: Arc::clone(&s.stop),
        first_of_focus,
    };
    drop(s);
    shared.publish(false);
    next
}

/// Loads `model` if its slot does not hold it. A failed load verifies the
/// files and puts the part in the not-working set.
fn ensure_loaded(
    shared: &Shared,
    b: &mut dyn TagBackend,
    model: &'static ModelChoice,
    turn: (u64, u64, i64),
) -> bool {
    if b.loaded(model.part) == Some(model.id) {
        return true;
    }
    let (epoch, generation, id) = turn;
    let dir = install::model_dir(&shared.models, model);
    let deadline = Deadline::after(shared.limits.load);
    running(
        shared,
        epoch,
        generation,
        RunKind::Load,
        id,
        model,
        shared.limits.load,
        None,
    );
    let loaded = match install::quick_state(&dir, model) {
        install::InstallState::Ok => {
            match catch_unwind(AssertUnwindSafe(|| {
                b.load(model, &dir, shared.threads, VOCABULARY, &deadline)
            })) {
                Ok(r) => r,
                Err(_) => Err(BackendError::Load("the load panicked".into())),
            }
        }
        _ => Err(BackendError::Load(words::incomplete(
            model.label,
            "files missing or unchecked",
        ))),
    };
    if !returned(shared, epoch) {
        return false;
    }
    match loaded {
        Ok(()) => {
            shared.set_text(b);
            true
        }
        Err(e) => {
            b.unload(model.part);
            shared.set_text(b);
            tracing::warn!(target: "tagging", "{} would not load: {e}", model.id);
            // Every present file is hashed again, whatever the marker says.
            let state = install::verify(&dir, model, true);
            tracing::info!(target: "tagging", "{} after the failed load: {state:?}", model.id);
            shared.lock().not_working.insert(model.part);
            shared.publish(true);
            false
        }
    }
}

/// One photo's turn on the tagging thread.
struct Turn<'a> {
    id: i64,
    generation: u64,
    /// The thread's epoch: a thread written off writes nothing.
    epoch: u64,
    stop: &'a Arc<AtomicBool>,
    first_of_focus: bool,
}

/// Tags one photo with the active models.
fn tag(
    shared: &Shared,
    b: &mut dyn TagBackend,
    work: &AlbumWork,
    turn: &Turn<'_>,
    models: &[&'static ModelChoice],
) {
    let Turn {
        id,
        generation,
        epoch,
        stop,
        first_of_focus,
    } = *turn;
    let album = &work.album;
    let scope = album.location().scope.clone();
    let current = || {
        let s = shared.lock();
        s.generation == generation && s.epoch == epoch && !stop.load(Ordering::Relaxed)
    };
    // Copies' rows first: no model runs for them.
    let _ = album.db().write(move |tx| store::take_from_copies(tx, id));
    let mut todo = Vec::new();
    for m in models {
        let has = album
            .db()
            .read(|c| store::row(c, id, m.id))
            .ok()
            .flatten()
            .is_some();
        if !has {
            todo.push(*m);
        }
    }
    let touched_before = album
        .db()
        .read(|c| {
            let mut ids = vec![id];
            ids.extend(store::copies(c, id)?);
            Ok(ids)
        })
        .unwrap_or_else(|_| vec![id]);
    let all: Vec<&'static str> = shared
        .lock()
        .active
        .iter()
        .flatten()
        .map(|m| m.id)
        .collect();
    let before = states(album, &touched_before, &all, &scope);
    let mut rows = Vec::new();
    if !todo.is_empty() {
        let picture = work.pictures.picture(id);
        let (rgb, name) = match picture {
            Picture::Gone => return,
            Picture::Ready(rgb, name) => (Some(rgb), name),
            Picture::Unreadable(name) => (None, name),
        };
        for m in &todo {
            if !current() {
                return;
            }
            let made_at = store::now_ms();
            let Some(rgb) = rgb.as_ref() else {
                rows.push(TagRow::failed(
                    m.part,
                    m.id,
                    words::COULD_NOT_READ_PHOTO,
                    made_at,
                ));
                continue;
            };
            if !ensure_loaded(shared, b, m, (epoch, generation, id)) {
                if !current() {
                    return;
                }
                continue;
            }
            let image = TagImage { rgb, name: &name };
            let limit = match m.part {
                Part::Describe => shared.limits.describe,
                Part::Keywords => shared.limits.keywords,
            };
            let deadline = Deadline::new(limit, Arc::clone(stop));
            running(
                shared,
                epoch,
                generation,
                RunKind::Photo,
                id,
                m,
                limit,
                None,
            );
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                run_model(b, m, &image, &deadline, made_at)
            }));
            if !returned(shared, epoch) || !current() {
                return;
            }
            match outcome {
                Ok(Ok(row)) => {
                    shared.lock().losses[m.part.index()] = 0;
                    rows.push(row);
                }
                Ok(Err(BackendError::Photo(msg))) => {
                    shared.lock().losses[m.part.index()] = 0;
                    rows.push(TagRow::failed(m.part, m.id, &msg, made_at));
                }
                Ok(Err(BackendError::Timeout)) => {
                    rows.push(TagRow::failed(m.part, m.id, words::TOOK_TOO_LONG, made_at));
                    loss(shared, b, m.part);
                }
                Ok(Err(BackendError::Lost(_) | BackendError::Load(_))) | Err(_) => {
                    loss(shared, b, m.part);
                    // No row: the photo is tried again.
                    if let Some(l) = shared.lock().list.as_mut() {
                        l.insert(id);
                    }
                }
            }
        }
    }
    if !current() {
        return;
    }
    if !rows.is_empty() {
        let written = album.db().write(move |tx| store::write(tx, id, &rows));
        if let Err(e) = written {
            let mut s = shared.lock();
            s.problem = Some(e.message);
            drop(s);
            shared.publish(true);
            return;
        }
    }
    let after = states(album, &touched_before, &all, &scope);
    {
        let mut s = shared.lock();
        if s.generation != generation {
            return;
        }
        if let Some((_, done, failed)) = s.counts.as_mut() {
            for (b4, af) in before.iter().zip(&after) {
                let (Some(b4), Some(af)) = (b4, af) else {
                    continue;
                };
                *done = (*done + u64::from(af.0)).saturating_sub(u64::from(b4.0));
                *failed = (*failed + u64::from(af.1)).saturating_sub(u64::from(b4.1));
            }
        }
        s.tags_pending = true;
    }
    shared.publish(false);
    shared.flush_tags(first_of_focus);
}

/// Regenerate on the tagging thread: up to two attempts, the second only
/// after a lost model; the new sentence goes to the row and every copy's.
fn regenerate(
    shared: &Shared,
    b: &mut dyn TagBackend,
    work: &AlbumWork,
    job: &Job,
    generation: u64,
    stop: &Arc<AtomicBool>,
    epoch: u64,
) -> AppResult<()> {
    let m = job.model;
    let id = job.id;
    let current = || {
        let s = shared.lock();
        s.generation == generation && s.epoch == epoch && !stop.load(Ordering::Relaxed)
    };
    let (rgb, name) = match work.pictures.picture(id) {
        Picture::Ready(rgb, name) => (rgb, name),
        Picture::Gone => {
            return Err(AppError::new(
                ErrorCode::PhotoNotFound,
                crate::text::photo::NOT_FOUND,
            ));
        }
        Picture::Unreadable(_) => return Err(AppError::internal(words::COULD_NOT_READ_PHOTO)),
    };
    let image = TagImage {
        rgb: &rgb,
        name: &name,
    };
    for attempt in 1..=2 {
        if !ensure_loaded(shared, b, m, (epoch, generation, id)) {
            if !current() {
                return Err(stopped());
            }
            return Err(AppError::new(
                ErrorCode::ModelLoadFailed,
                words::model_load_failed(m.label),
            ));
        }
        let deadline = Deadline::new(shared.limits.caption, Arc::clone(stop));
        running(
            shared,
            epoch,
            generation,
            RunKind::Regenerate,
            id,
            m,
            shared.limits.caption,
            Some(&job.reply),
        );
        let outcome = catch_unwind(AssertUnwindSafe(|| b.caption(&image, &deadline)));
        if !returned(shared, epoch) || !current() {
            return Err(stopped());
        }
        match outcome {
            Ok(Ok(text)) => {
                let text = text.trim().to_owned();
                if text.is_empty() {
                    return Err(AppError::new(
                        ErrorCode::EmptyDescription,
                        words::empty_description(m.label),
                    ));
                }
                let model = m.id;
                let made_at = store::now_ms();
                work.album
                    .db()
                    .write(move |tx| store::set_description(tx, id, model, &text, made_at))?;
                {
                    let mut s = shared.lock();
                    s.tags_pending = true;
                    s.last_tags_at = None;
                }
                shared.flush_tags(true);
                return Ok(());
            }
            Ok(Err(BackendError::Timeout)) => {
                b.unload(m.part);
                shared.set_text(b);
                return Err(AppError::new(
                    ErrorCode::TaggingTimeout,
                    words::tagging_timeout(m.label),
                ));
            }
            Ok(Err(BackendError::Photo(message))) => {
                return Err(AppError::new(ErrorCode::TaggingLost, message));
            }
            Ok(Err(BackendError::Lost(_) | BackendError::Load(_))) | Err(_) => {
                b.unload(m.part);
                shared.set_text(b);
                if attempt == 2 {
                    return Err(AppError::new(
                        ErrorCode::TaggingLost,
                        words::tagging_lost(m.label),
                    ));
                }
            }
        }
    }
    Err(AppError::new(
        ErrorCode::TaggingLost,
        words::tagging_lost(m.label),
    ))
}

/// Done and failed, for each photo that counts (eligible, in scope).
fn states(album: &Album, ids: &[i64], models: &[&str], scope: &str) -> Vec<Option<(bool, bool)>> {
    album
        .db()
        .read(|c| {
            let mut out = Vec::new();
            for &id in ids {
                let eligible = c
                    .query_row(
                        &format!(
                            "SELECT 1 FROM photos p WHERE p.id = ?2 AND {}",
                            store::ELIGIBLE
                        ),
                        rusqlite::params![scope, id],
                        |_| Ok(()),
                    )
                    .is_ok();
                out.push(if eligible {
                    Some(store::photo_state(c, id, models)?)
                } else {
                    None
                });
            }
            Ok(out)
        })
        .unwrap_or_default()
}

fn loss(shared: &Shared, b: &mut dyn TagBackend, part: Part) {
    b.unload(part);
    shared.set_text(b);
    let mut s = shared.lock();
    s.losses[part.index()] += 1;
    if s.losses[part.index()] >= MAX_LOSSES {
        s.not_working.insert(part);
    }
    drop(s);
    shared.publish(true);
}

/// One model on one picture: the row to store.
fn run_model(
    b: &mut dyn TagBackend,
    m: &ModelChoice,
    image: &TagImage,
    deadline: &Deadline,
    made_at: i64,
) -> Result<TagRow, BackendError> {
    let mut row = TagRow {
        part: m.part,
        model_id: m.id.to_owned(),
        status: Status::Done,
        description: None,
        objects: Vec::new(),
        keywords: Vec::new(),
        error: None,
        made_at,
        vector: None,
    };
    match m.part {
        Part::Describe => {
            let d = b.describe(image, deadline)?;
            row.description = d.description;
            row.objects = d.objects;
        }
        Part::Keywords => {
            let k = b.keywords(image, deadline)?;
            if !results::usable_vector(&k.vector, m.dim.unwrap_or(0)) {
                return Ok(TagRow::failed(
                    m.part,
                    m.id,
                    words::UNUSABLE_VECTOR,
                    made_at,
                ));
            }
            row.keywords = k.keywords;
            row.vector = Some(k.vector);
        }
    }
    Ok(row)
}

/// Pictures from the album's thumbnails: the medium thumbnail, made if it
/// is missing.
pub struct ThumbPictures {
    pub album: Arc<Album>,
    pub thumbs: crate::thumbs::Thumbs,
}

impl Pictures for ThumbPictures {
    fn picture(&self, id: i64) -> Picture {
        let overlay = Mutex::new(crate::scan::memory::Overlay::default());
        let photo = match crate::thumbs::lookup::by_id(&self.album, &overlay, id) {
            Ok(Some(p)) => p,
            _ => return Picture::Gone,
        };
        if !photo.path.exists() {
            return Picture::Gone;
        }
        let name = photo
            .rel_path
            .rsplit('/')
            .next()
            .unwrap_or(&photo.rel_path)
            .to_owned();
        match self
            .thumbs
            .background_thumbnail(&photo, crate::thumbs::Kind::Medium)
            .and_then(|bytes| image::load_from_memory(&bytes).ok())
        {
            Some(img) => Picture::Ready(img.to_rgb8(), name),
            None => Picture::Unreadable(name),
        }
    }
}
