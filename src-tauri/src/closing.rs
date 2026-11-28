//! The close handshake (docs/08-desktop-and-release.md, "Closing and
//! quitting"; docs/06-editing-and-file-actions.md, "Closing and quitting").
//!
//! A close of the window is held: the shell emits `app_closing`, the page
//! ends edit mode as Done, saves the session and answers `app_close_ready`.
//! The shell waits for that answer at most [`PAGE_WAIT`] while no save
//! runs, waits for a running save's current photo, asks the close question
//! in `file` mode when photos wait to be saved, closes the album and exits.
//! Once the question is past, a watchdog gives the remaining steps
//! [`CLOSE_WAIT`]: if closing the album stalls, the process exits anyway.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use owlbum_core::app::AppClosing;
use owlbum_core::appdata::settings::EditTarget;
use owlbum_core::edit::save;
use owlbum_core::text;
use tauri::{AppHandle, Manager, Runtime};

use crate::commands::edit::edit_target;
use crate::commands::file::desktop;
use crate::session::Session;

/// How long the page has to answer `app_closing` while no save runs.
pub const PAGE_WAIT: Duration = Duration::from_secs(3);

/// The longest the close waits for a running save's current photo.
pub const SAVE_WAIT: Duration = Duration::from_secs(120);

/// How long the steps after the close question (geometry, tagging, closing
/// the album) may take before the process exits without them.
pub const CLOSE_WAIT: Duration = Duration::from_secs(5);

/// How the process leaves, as passed to an exit hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// The closing steps finished.
    Closed,
    /// Closing the album took longer than [`CLOSE_WAIT`].
    Stalled,
}

/// Stands in for the process exit (shell tests observe it instead).
pub type ExitHook = Arc<dyn Fn(Exit) + Send + Sync>;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// No close asked for.
    #[default]
    Open,
    /// `app_closing` was sent; waiting for the page.
    Asked,
    /// The page answered `app_close_ready`.
    Ready,
}

/// Why the program is closing. The shell keeps it to itself; the page is
/// told only that it is closing.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// The window's close button, File > Quit or `app_quit`.
    #[default]
    Close,
    /// An update install: the install follows the closing steps.
    Update,
}

/// The close handshake's state, managed by `setup`.
#[derive(Default)]
pub struct Closing {
    phase: Mutex<Phase>,
    changed: Condvar,
    reason: Mutex<Reason>,
    /// Set by whichever leaves first: the finished steps or the watchdog.
    left: AtomicBool,
    exit_hook: Mutex<Option<ExitHook>>,
}

impl std::fmt::Debug for Closing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Closing")
            .field("phase", &*self.phase())
            .field("left", &self.left.load(Ordering::SeqCst))
            .finish_non_exhaustive()
    }
}

impl Closing {
    /// Replaces the process exit with `hook`. Without one, a finished close
    /// exits through Tauri and a stalled one ends the process at once.
    pub fn set_exit_hook(&self, hook: ExitHook) {
        *self
            .exit_hook
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(hook);
    }

    fn exit_hook(&self) -> Option<ExitHook> {
        self.exit_hook
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Claims the way out; false when the other side already left.
    fn leave(&self) -> bool {
        !self.left.swap(true, Ordering::SeqCst)
    }

    fn phase(&self) -> std::sync::MutexGuard<'_, Phase> {
        self.phase.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Starts a close; false when one is already under way.
    pub fn begin(&self) -> bool {
        let mut p = self.phase();
        if *p != Phase::Open {
            return false;
        }
        *p = Phase::Asked;
        self.left.store(false, Ordering::SeqCst);
        true
    }

    /// The page's `app_close_ready`.
    pub fn ready(&self) {
        let mut p = self.phase();
        if *p == Phase::Asked {
            *p = Phase::Ready;
            self.changed.notify_all();
        }
    }

    /// The close was called off ("Go back").
    pub fn cancel(&self) {
        *self.phase() = Phase::Open;
    }

    /// Waits for the page's answer until `deadline`, or longer while
    /// `busy()` (a save runs). True when the page answered.
    pub fn wait_ready(&self, limit: Duration, busy: &dyn Fn() -> bool) -> bool {
        let deadline = Instant::now() + limit;
        let mut p = self.phase();
        loop {
            if *p == Phase::Ready {
                return true;
            }
            let now = Instant::now();
            if now >= deadline && !busy() {
                return false;
            }
            let wait = if now >= deadline {
                Duration::from_millis(100)
            } else {
                (deadline - now).min(Duration::from_millis(100))
            };
            p = self
                .changed
                .wait_timeout(p, wait)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }
}

/// The window was asked to close: hold the close and run the handshake on
/// a thread of its own. Answers true when the close must be held.
pub fn requested<R: Runtime>(app: &AppHandle<R>) -> bool {
    requested_for(app, Reason::Close)
}

/// Starts the closing steps for a reason. With [`Reason::Update`] the
/// install runs after them instead of the exit.
pub fn requested_for<R: Runtime>(app: &AppHandle<R>, reason: Reason) -> bool {
    let Some(closing) = app.try_state::<Closing>() else {
        return false;
    };
    if closing.begin() {
        *closing
            .reason
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = reason;
        let app = app.clone();
        std::thread::Builder::new()
            .name("owlbum-close".into())
            .spawn(move || run(&app))
            .is_ok()
    } else {
        // A close is already under way.
        true
    }
}

/// The closing steps after the close was held.
fn run<R: Runtime>(app: &AppHandle<R>) {
    let closing = app.state::<Closing>();
    let reason = *closing
        .reason
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let saves_running = || {
        app.state::<Session>()
            .scanner()
            .is_some_and(|s| s.album().saves().count() > 0)
    };
    // 1. The page ends edit mode and saves the session.
    let _ = crate::events::emit(app, &AppClosing {});
    if !closing.wait_ready(PAGE_WAIT, &saves_running) {
        tracing::warn!(target: "app", "the page did not answer app_closing within 3 s; closing anyway");
    }
    // A running save finishes its current photo; Save all stops after it.
    let scanner = app.state::<Session>().scanner();
    if let Some(scanner) = &scanner {
        scanner.album().saves().stop();
        if !scanner.album().saves().wait_idle(SAVE_WAIT) {
            tracing::warn!(target: "app", "a save was still running when Owlbum closed");
        }
    }
    // 2. The close question, in `file` mode with photos waiting.
    if let Some(scanner) = &scanner
        && edit_target(app) == EditTarget::File
    {
        let waiting = save::status(scanner.album(), EditTarget::File).map_or(0, |s| s.waiting);
        if waiting > 0 {
            let go_on = desktop(app).is_ok_and(|d| {
                d.confirm_close(
                    &text::edit::close_message(waiting),
                    text::edit::CLOSE_DETAIL,
                )
            });
            if !go_on {
                scanner.album().saves().resume();
                closing.cancel();
                if reason == Reason::Update {
                    crate::update::called_off(app);
                }
                return;
            }
        }
    }
    drop(scanner);
    // Nothing fatal is reported from here on.
    crate::fatal::quitting();
    // From here the steps get CLOSE_WAIT; the watchdog exits without them.
    let finished = watchdog(app, reason);
    // 3. The window geometry.
    crate::window::save(app);
    // 4. Tagging stops: the download is cancelled, the run in hand is told
    //    to end and the models are unloaded.
    crate::commands::tagging::shutdown(app);
    // 5. Close the album: its writer finishes and its lock goes. Not while
    //    an album is opening or closing.
    {
        let session = app.state::<Session>();
        let _one = session.one_at_a_time();
        session.close();
    }
    let _ = finished.send(());
    if !closing.leave() {
        // The watchdog has already ended the process.
        return;
    }
    // 6. Exit, or install the update, which starts the new version.
    if reason == Reason::Update {
        tracing::info!(target: "app", "closing to install the update");
        match closing.exit_hook() {
            Some(hook) => hook(Exit::Closed),
            None => crate::update::after_close(app),
        }
        return;
    }
    tracing::info!(target: "app", "closing");
    match closing.exit_hook() {
        Some(hook) => hook(Exit::Closed),
        None => app.exit(0),
    }
}

/// Starts the watchdog over the last closing steps. Sending on the answer
/// (or dropping it) says they finished; if that does not come within
/// [`CLOSE_WAIT`], the process exits without them and the log says so.
fn watchdog<R: Runtime>(app: &AppHandle<R>, reason: Reason) -> mpsc::Sender<()> {
    let (tx, rx) = mpsc::channel::<()>();
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("owlbum-close-watchdog".into())
        .spawn(move || {
            // A dropped answer without a send means the steps panicked.
            if rx.recv_timeout(CLOSE_WAIT).is_ok() {
                return;
            }
            let closing = app.state::<Closing>();
            if !closing.leave() {
                return;
            }
            if reason == Reason::Update {
                tracing::warn!(target: "app", "closing the album did not finish within 5 s; exiting without installing the update");
            } else {
                tracing::warn!(target: "app", "closing the album did not finish within 5 s; exiting anyway");
            }
            match closing.exit_hook() {
                Some(hook) => hook(Exit::Stalled),
                None => std::process::exit(0),
            }
        });
    if let Err(e) = spawned {
        tracing::warn!(target: "app", "the close watchdog could not start: {e}");
    }
    tx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ready_page_is_not_waited_for() {
        let c = Closing::default();
        assert!(c.begin());
        assert!(!c.begin(), "one close at a time");
        c.ready();
        assert!(c.wait_ready(Duration::from_secs(5), &|| false));
    }

    #[test]
    fn a_silent_page_is_waited_for_only_while_no_save_runs() {
        let c = Closing::default();
        c.begin();
        let start = Instant::now();
        assert!(!c.wait_ready(Duration::from_millis(50), &|| false));
        assert!(start.elapsed() < Duration::from_secs(2));
        let calls = std::sync::atomic::AtomicU32::new(0);
        let busy = || calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) < 3;
        assert!(!c.wait_ready(Duration::ZERO, &busy));
        assert!(calls.load(std::sync::atomic::Ordering::SeqCst) >= 3);
    }

    #[test]
    fn going_back_allows_a_new_close() {
        let c = Closing::default();
        c.begin();
        c.cancel();
        assert!(c.begin());
    }
}
