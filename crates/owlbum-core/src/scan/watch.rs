//! Folder watching (docs/03-media-and-scanning.md, "Folder watching").
//!
//! While an album is open, [`AlbumWatch`] keeps one recursive `notify`
//! watch on the scope's folder, on a thread of its own. Each changed path
//! becomes a folder to rescan by the rules in [`intake`]; the folders are
//! collected until the events go quiet and then handed to the
//! [`Scanner`] as one targeted scan, whose `album_changed` events have
//! reason `watch`.
//!
//! When watching cannot be trusted (a network share, a watch that will not
//! start, such as the Linux inotify watch limit, or a watch that fails
//! later) there is no watch, `album_status` says `watching: false`, and
//! the album is rescanned when the window gains focus and the last scan
//! ended over 5 minutes ago ([`AlbumWatch::focused`]). A watch lost because
//! its folder went away (an unplugged drive, a share that dropped) is
//! started again on focus once the folder is back, followed by a whole
//! scan, since changes made meanwhile were not seen.

pub mod intake;
pub mod share;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use notify::event::{EventKind, ModifyKind};
use notify::{Config, EventKindMask, RecommendedWatcher, RecursiveMode, Watcher};

use self::intake::{Debounce, Due, Kind, Seen, Stat, Timing};
use super::run::Request;
use super::scanner::Scanner;
use super::walk;

/// How a watch runs. Tests shorten the waits and replace the starter.
#[derive(Clone)]
pub struct WatchEnv {
    /// The debounce: 2 s quiet, 10 s at most, 500 folders.
    pub timing: Timing,
    /// Without a watch, a window focus rescans when the last scan ended at
    /// least this long ago.
    pub focus_after: Duration,
    /// How often the watched folder is checked to be still there. A
    /// watch is not told when its own folder is renamed or its drive goes
    /// away on every system, so this is how the watch learns it is lost.
    pub check_every: Duration,
    /// Starts the watch on a folder, sending its events to the handler.
    pub start: StartWatch,
}

/// Starts a recursive watch on a folder. The returned value is the watch:
/// dropping it ends the watch.
pub type StartWatch = fn(
    &Path,
    Box<dyn FnMut(notify::Result<notify::Event>) + Send>,
) -> notify::Result<Box<dyn std::any::Any + Send>>;

impl std::fmt::Debug for WatchEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WatchEnv")
            .field("timing", &self.timing)
            .field("focus_after", &self.focus_after)
            .finish_non_exhaustive()
    }
}

impl Default for WatchEnv {
    fn default() -> Self {
        Self {
            timing: Timing::default(),
            focus_after: Duration::from_secs(5 * 60),
            check_every: Duration::from_secs(5),
            start: start_notify,
        }
    }
}

/// The real watch: the system's recommended `notify` backend
/// (ReadDirectoryChangesW on Windows, inotify on Linux), recursive, not
/// following links, without access events (the scanner's own reads must
/// not look like changes).
fn start_notify(
    folder: &Path,
    handler: Box<dyn FnMut(notify::Result<notify::Event>) + Send>,
) -> notify::Result<Box<dyn std::any::Any + Send>> {
    let config = Config::default()
        .with_event_kinds(EventKindMask::CORE)
        .with_follow_symlinks(false);
    let mut watcher = RecommendedWatcher::new(handler, config)?;
    watcher.watch(folder, RecursiveMode::Recursive)?;
    Ok(Box::new(watcher))
}

/// Whether the scope is watched, and if not, why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchState {
    /// The watch is being set up.
    Starting,
    /// Changes are seen as they happen.
    Watching,
    /// The folder is on a network share: no watch is set.
    NetworkShare,
    /// The watch would not start (for example the Linux inotify watch
    /// limit). Not tried again while the album is open.
    Failed,
    /// The watch reported an error, or its folder went away. Tried again
    /// on window focus.
    Lost,
}

/// An open album's folder watch. Drop it before its [`Scanner`]'s last
/// owner: dropping ends the watch and waits for its thread.
pub struct AlbumWatch {
    scanner: Arc<Scanner>,
    env: WatchEnv,
    state: Arc<Mutex<WatchState>>,
    running: Mutex<Option<Running>>,
}

impl std::fmt::Debug for AlbumWatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AlbumWatch")
            .field("state", &self.state())
            .finish_non_exhaustive()
    }
}

/// The watch thread, and how to stop it.
struct Running {
    stop: mpsc::Sender<Msg>,
    thread: JoinHandle<()>,
}

impl Running {
    fn stop(self) {
        let _ = self.stop.send(Msg::Stop);
        let _ = self.thread.join();
    }
}

enum Msg {
    Event(notify::Result<notify::Event>),
    Stop,
}

impl AlbumWatch {
    /// Starts watching the scope of `scanner`'s album, on a thread of its
    /// own (a recursive inotify watch walks the whole tree).
    pub fn start(scanner: Arc<Scanner>, env: WatchEnv) -> Self {
        let watch = Self {
            scanner,
            env,
            state: Arc::new(Mutex::new(WatchState::Starting)),
            running: Mutex::new(None),
        };
        watch.spawn(false);
        watch
    }

    /// Whether the scope is watched now.
    pub fn state(&self) -> WatchState {
        *lock(&self.state)
    }

    /// The window gained focus. Without a watch, rescans the whole scope
    /// when the last scan ended at least [`WatchEnv::focus_after`] ago; a
    /// watch lost because of an error or a vanished folder is started
    /// again, and then the whole scope is rescanned.
    pub fn focused(&self) {
        let state = self.state();
        if matches!(state, WatchState::Starting | WatchState::Watching) {
            return;
        }
        let due = self.scanner.idle_for(self.env.focus_after, Instant::now());
        if due {
            self.scanner.request(Request::default());
        }
        if state == WatchState::Lost && self.scope_is_folder() {
            *lock(&self.state) = WatchState::Starting;
            self.spawn(!due);
        }
    }

    fn scope_is_folder(&self) -> bool {
        self.watched().is_dir()
    }

    /// The scope's folder.
    fn watched(&self) -> PathBuf {
        let location = self.scanner.album().location();
        walk::absolute(&location.root, &location.scope)
    }

    /// Starts the watch thread, replacing any old one. `rescan` asks for a
    /// whole scan once the watch is set.
    fn spawn(&self, rescan: bool) {
        let mut running = lock(&self.running);
        if let Some(old) = running.take() {
            old.stop();
        }
        let (send, receive) = mpsc::channel();
        let job = Job {
            scanner: Arc::clone(&self.scanner),
            env: self.env.clone(),
            state: Arc::clone(&self.state),
            folder: self.watched(),
            scope: self.scanner.album().location().scope.clone(),
            rescan,
        };
        let events = send.clone();
        let spawned = std::thread::Builder::new()
            .name("owlbum-watch".into())
            .spawn(move || job.run(&events, &receive));
        match spawned {
            Ok(thread) => {
                *running = Some(Running { stop: send, thread });
            }
            Err(_) => self.set(WatchState::Failed),
        }
    }

    fn set(&self, state: WatchState) {
        set_state(&self.state, &self.scanner, state);
    }
}

impl Drop for AlbumWatch {
    fn drop(&mut self) {
        if let Some(running) = lock(&self.running).take() {
            running.stop();
        }
        self.scanner.set_watching(false);
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn set_state(cell: &Mutex<WatchState>, scanner: &Scanner, state: WatchState) {
    *lock(cell) = state;
    scanner.set_watching(state == WatchState::Watching);
}

/// What the watch thread works with.
struct Job {
    scanner: Arc<Scanner>,
    env: WatchEnv,
    state: Arc<Mutex<WatchState>>,
    /// The scope's folder, watched.
    folder: PathBuf,
    /// The scope, relative to the album root.
    scope: String,
    rescan: bool,
}

impl Job {
    fn set(&self, state: WatchState) {
        set_state(&self.state, &self.scanner, state);
    }

    fn run(self, events: &mpsc::Sender<Msg>, messages: &mpsc::Receiver<Msg>) {
        if share::is_network_share(&self.folder) {
            tracing::info!(target: "scan", "not watching a network share");
            self.set(WatchState::NetworkShare);
            return;
        }
        let events = events.clone();
        let started = (self.env.start)(
            &self.folder,
            Box::new(move |e| {
                let _ = events.send(Msg::Event(e));
            }),
        );
        // Dropped when this thread ends, never inside its own handler.
        let _watch = match started {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!(target: "scan", "folder watch did not start: {e}");
                // A folder that is not there (yet) may come back.
                self.set(if self.folder.is_dir() {
                    WatchState::Failed
                } else {
                    WatchState::Lost
                });
                return;
            }
        };
        self.set(WatchState::Watching);
        if self.rescan {
            self.scanner.request(Request::default());
        }
        self.listen(messages);
    }

    /// Collects events into the folder set until it is due, then asks the
    /// scanner. Ends on stop, or when the watch cannot be trusted any more.
    fn listen(&self, messages: &mpsc::Receiver<Msg>) {
        let mut debounce = Debounce::new(self.env.timing);
        loop {
            let now = Instant::now();
            let check_at = now + self.env.check_every;
            let wake = debounce.due_at().map_or(check_at, |due| due.min(check_at));
            let next = messages.recv_timeout(wake.saturating_duration_since(now));
            match next {
                Ok(Msg::Event(Ok(event))) => {
                    if !self.take(&event, &mut debounce) {
                        self.lose(&mut debounce, "the watched folder is gone");
                        return;
                    }
                }
                Ok(Msg::Event(Err(e))) => {
                    if self.harmless(&e) {
                        continue;
                    }
                    self.lose(&mut debounce, &e.to_string());
                    return;
                }
                Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => {
                    if !self.folder.is_dir() {
                        self.lose(&mut debounce, "the watched folder is gone");
                        return;
                    }
                }
            }
            if let Some(due) = debounce.take_due(Instant::now()) {
                self.ask(due);
            }
        }
    }

    /// Adds one event's paths to the set. False when the watched folder
    /// itself is gone.
    fn take(&self, event: &notify::Event, debounce: &mut Debounce) -> bool {
        let now = Instant::now();
        if event.need_rescan() {
            debounce.add_whole(now);
        }
        let Some(kind) = kind_of(event.kind) else {
            return true;
        };
        for path in &event.paths {
            let Some(rel) = relative(&self.folder, &self.scope, path) else {
                // A path the watch did not report under its folder: the
                // whole scope is the safe answer.
                debounce.add_whole(now);
                continue;
            };
            let seen = intake::seen(&self.scope, &rel, kind, stat(path), |dir| {
                self.scanner.has_rows_under(dir)
            });
            match seen {
                Seen::Nothing => {}
                Seen::Folder { rel_dir, recursive } => debounce.add(&rel_dir, recursive, now),
                Seen::ScopeGone => return false,
            }
        }
        true
    }

    /// An error that leaves the watch sound: a folder that vanished while
    /// the watch was being added to it, while the scope is still there.
    fn harmless(&self, e: &notify::Error) -> bool {
        matches!(
            e.kind,
            notify::ErrorKind::PathNotFound | notify::ErrorKind::WatchNotFound
        ) && self.folder.is_dir()
    }

    /// The watch cannot be trusted any more: what was seen is still
    /// scanned, then the watch is dropped.
    fn lose(&self, debounce: &mut Debounce, why: &str) {
        tracing::warn!(target: "scan", "folder watch dropped: {why}");
        if self.folder.is_dir()
            && let Some(due) = debounce.take()
        {
            self.ask(due);
        }
        self.set(WatchState::Lost);
    }

    fn ask(&self, due: Due) {
        self.scanner.request(Request::watch(match due {
            Due::Folders(f) => Some(f),
            Due::Whole => None,
        }));
    }
}

/// The rules' kind of a `notify` event; `None` for events that change
/// nothing (access, which the watch does not ask for anyway).
fn kind_of(kind: EventKind) -> Option<Kind> {
    match kind {
        EventKind::Access(_) => None,
        EventKind::Create(_) => Some(Kind::Created),
        EventKind::Remove(_) => Some(Kind::Removed),
        EventKind::Modify(ModifyKind::Name(_)) => Some(Kind::Renamed),
        EventKind::Modify(_) | EventKind::Any | EventKind::Other => Some(Kind::Changed),
    }
}

/// What is at `path` now, without following links.
fn stat(path: &Path) -> Stat {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => Stat::Link,
        Ok(m) if m.is_dir() => Stat::Folder,
        Ok(_) => Stat::File,
        Err(_) => Stat::Gone,
    }
}

/// `path`, reported by a watch on `folder` (the scope `scope`), relative
/// to the album root with `/` separators. `None` when it is not under
/// `folder` or a part of it is not Unicode.
fn relative(folder: &Path, scope: &str, path: &Path) -> Option<String> {
    let below = path.strip_prefix(folder).ok()?;
    let mut rel = scope.to_owned();
    for part in below.components() {
        let std::path::Component::Normal(name) = part else {
            return None;
        };
        if !rel.is_empty() {
            rel.push('/');
        }
        rel.push_str(name.to_str()?);
    }
    Some(rel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_made_relative_to_the_album_root() {
        let folder = walk::absolute(Path::new("/albums/a"), "2024");
        assert_eq!(
            relative(&folder, "2024", &folder.join("trip").join("x.jpg")).as_deref(),
            Some("2024/trip/x.jpg")
        );
        assert_eq!(relative(&folder, "2024", &folder).as_deref(), Some("2024"));
        let root = PathBuf::from("/albums/a");
        assert_eq!(
            relative(&root, "", &root.join("x.jpg")).as_deref(),
            Some("x.jpg")
        );
        assert_eq!(
            relative(&folder, "2024", Path::new("/elsewhere/x.jpg")),
            None
        );
    }

    #[test]
    fn access_events_change_nothing() {
        use notify::event::{AccessKind, CreateKind, RemoveKind, RenameMode};
        assert_eq!(kind_of(EventKind::Access(AccessKind::Any)), None);
        assert_eq!(
            kind_of(EventKind::Create(CreateKind::Any)),
            Some(Kind::Created)
        );
        assert_eq!(
            kind_of(EventKind::Remove(RemoveKind::Any)),
            Some(Kind::Removed)
        );
        assert_eq!(
            kind_of(EventKind::Modify(ModifyKind::Name(RenameMode::From))),
            Some(Kind::Renamed)
        );
        assert_eq!(
            kind_of(EventKind::Modify(ModifyKind::Any)),
            Some(Kind::Changed)
        );
    }
}
