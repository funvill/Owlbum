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
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
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

struct State {
    album: Option<AlbumWork>,
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
}

/// The tagging thread and its queue.
pub struct Runtime {
    shared: Arc<Shared>,
    thread: Mutex<Option<JoinHandle<()>>>,
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
            }),
            thread: Mutex::new(None),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.shared.lock()
    }

    fn ensure_thread(&self) {
        let mut t = self.thread.lock().unwrap_or_else(|p| p.into_inner());
        if t.is_none() {
            let shared = Arc::clone(&self.shared);
            *t = std::thread::Builder::new()
                .name("owlbum-tagging".into())
                .spawn(move || run(&shared))
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
        self.change(|s| {
            s.stop.store(true, Ordering::Relaxed);
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
        self.shared.text.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Stops the thread and unloads the models (Owlbum closes).
    pub fn shutdown(&self) {
        {
            let mut s = self.state();
            s.shutdown = true;
            s.stop.store(true, Ordering::Relaxed);
        }
        self.shared.wake.notify_all();
        if let Some(t) = self.thread.lock().unwrap_or_else(|p| p.into_inner()).take() {
            let _ = t.join();
        }
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
        let (total, done, failed) = if counted { s.counts.unwrap_or_default() } else { (0, 0, 0) };
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
            let due = s.last_tags_at.is_none_or(|t| t.elapsed() >= self.limits.tags_event);
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

/// What the thread does next.
enum Next {
    Wait(Option<Duration>),
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
fn run(shared: &Arc<Shared>) {
    let mut backend: Option<Box<dyn TagBackend>> = None;
    let mut last_work = Instant::now();
    loop {
        // Unloads asked for.
        let unload: Vec<Part> = std::mem::take(&mut shared.lock().unload).into_iter().collect();
        if let Some(b) = backend.as_mut() {
            for part in unload {
                b.unload(part);
            }
            shared.set_text(&**b);
        }
        let next = pick(shared);
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
                if idle >= shared.limits.idle_unload {
                    if let Some(b) = backend.as_mut()
                        && (b.loaded(Part::Describe).is_some() || b.loaded(Part::Keywords).is_some())
                    {
                        b.unload(Part::Describe);
                        b.unload(Part::Keywords);
                        shared.set_text(&**b);
                    }
                }
                let s = shared.lock();
                if s.shutdown {
                    continue;
                }
                let wait = limit
                    .unwrap_or(Duration::from_secs(1))
                    .min(shared.limits.idle_unload.saturating_sub(idle).max(Duration::from_millis(10)));
                let _ = shared.wake.wait_timeout(s, wait);
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
                if backend.is_none() {
                    match (shared.factory)() {
                        Ok(b) => backend = Some(b),
                        Err(reason) => {
                            let mut s = shared.lock();
                            s.problem = Some(words::runtime_failed(&reason));
                            s.not_working.extend(Part::ALL);
                            drop(s);
                            shared.publish(true);
                            continue;
                        }
                    }
                }
                let Some(b) = backend.as_mut() else { continue };
                tag(shared, &mut **b, &work, id, &models, generation, &stop, first_of_focus);
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
        return Next::Wait(None);
    };
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
        if s.album.as_ref().is_some_and(|a| Arc::ptr_eq(&a.album, &work.album)) {
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
        if !s.album.as_ref().is_some_and(|a| Arc::ptr_eq(&a.album, &work.album)) {
            return Next::Wait(Some(Duration::ZERO));
        }
        s.list = Some(set);
    }
    let list = s.list.as_ref().map(BTreeSet::clone).unwrap_or_default();
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
fn ensure_loaded(shared: &Shared, b: &mut dyn TagBackend, model: &'static ModelChoice) -> bool {
    if b.loaded(model.part) == Some(model.id) {
        return true;
    }
    let dir = install::model_dir(&shared.models, model);
    let deadline = Deadline::after(shared.limits.load);
    let loaded = match install::quick_state(&dir, model) {
        install::InstallState::Ok => {
            match catch_unwind(AssertUnwindSafe(|| b.load(model, &dir, shared.threads, VOCABULARY, &deadline))) {
                Ok(r) => r,
                Err(_) => Err(BackendError::Load("the load panicked".into())),
            }
        }
        _ => Err(BackendError::Load(words::incomplete(model.label, "files missing or unchecked"))),
    };
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

/// Tags one photo with the active models.
#[allow(clippy::too_many_arguments)]
fn tag(
    shared: &Shared,
    b: &mut dyn TagBackend,
    work: &AlbumWork,
    id: i64,
    models: &[&'static ModelChoice],
    generation: u64,
    stop: &Arc<AtomicBool>,
    first_of_focus: bool,
) {
    let album = &work.album;
    let scope = album.location().scope.clone();
    let current = || shared.lock().generation == generation && !stop.load(Ordering::Relaxed);
    // Copies' rows first: no model runs for them.
    let _ = album.db().write(move |tx| store::take_from_copies(tx, id));
    let mut todo = Vec::new();
    for m in models {
        let has = album.db().read(|c| store::row(c, id, m.id)).ok().flatten().is_some();
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
    let all: Vec<&'static str> = shared.lock().active.iter().flatten().map(|m| m.id).collect();
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
                rows.push(TagRow::failed(m.part, m.id, words::COULD_NOT_READ_PHOTO, made_at));
                continue;
            };
            if !ensure_loaded(shared, b, m) {
                continue;
            }
            let image = TagImage { rgb, name: &name };
            let limit = match m.part {
                Part::Describe => shared.limits.describe,
                Part::Keywords => shared.limits.keywords,
            };
            let deadline = Deadline::new(limit, Arc::clone(stop));
            let outcome = catch_unwind(AssertUnwindSafe(|| run_model(b, m, &image, &deadline, made_at)));
            if !current() {
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
                let (Some(b4), Some(af)) = (b4, af) else { continue };
                *done = (*done + u64::from(af.0)).saturating_sub(u64::from(b4.0));
                *failed = (*failed + u64::from(af.1)).saturating_sub(u64::from(b4.1));
            }
        }
        s.tags_pending = true;
    }
    shared.publish(false);
    shared.flush_tags(first_of_focus);
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
                        &format!("SELECT 1 FROM photos p WHERE p.id = ?2 AND {}", store::ELIGIBLE),
                        rusqlite::params![scope, id],
                        |_| Ok(()),
                    )
                    .is_ok();
                out.push(if eligible { Some(store::photo_state(c, id, models)?) } else { None });
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
                return Ok(TagRow::failed(m.part, m.id, words::UNUSABLE_VECTOR, made_at));
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
