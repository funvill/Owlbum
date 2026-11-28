//! Failure handling (docs/08-desktop-and-release.md, "Failure handling"):
//! the panic hook, the fatal error dialog and its buttons. There is no
//! crash reporter; nothing is uploaded.
//!
//! A panic is always logged with its backtrace. A panic on the main thread
//! or in an album's writer thread is fatal: the main thread's ends the
//! event loop, so it is shown in a native message box at once; the
//! writer's is sent to the page as `app_fatal`, which draws the dialog.
//! The program's start and the page's start are watched too, and their
//! failures shown in the native box.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

use owlbum_core::app::fatal::{self, AppFatal, Fatal};

/// What the technical block names: set once in `run`.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    pub package: String,
    pub data_dir: String,
    pub log_file: Option<PathBuf>,
}

static FACTS: OnceLock<Facts> = OnceLock::new();
/// The last fatal error's technical block, for `app_copy_details`.
static DETAILS: Mutex<Option<String>> = Mutex::new(None);
/// A fatal error was shown: it is shown once.
static SHOWN: AtomicBool = AtomicBool::new(false);
/// Owlbum is quitting: nothing fatal is reported any more.
static QUITTING: AtomicBool = AtomicBool::new(false);
/// The page has called `app_startup`.
static PAGE_STARTED: AtomicBool = AtomicBool::new(false);
/// Sends `app_fatal` to the page; set in `run`'s setup.
type PageReport = Box<dyn Fn(&AppFatal) + Send + Sync>;
static PAGE: OnceLock<PageReport> = OnceLock::new();

/// Records what the technical block names.
pub fn set_facts(facts: Facts) {
    let _ = FACTS.set(facts);
}

/// Gives the panic hook a way to reach the page.
pub fn set_page_report(report: PageReport) {
    let _ = PAGE.set(report);
}

/// The closing steps have begun.
pub fn quitting() {
    QUITTING.store(true, Ordering::SeqCst);
}

/// The page called `app_startup`.
pub fn page_started() {
    PAGE_STARTED.store(true, Ordering::SeqCst);
}

fn facts() -> Facts {
    FACTS.get().cloned().unwrap_or_default()
}

fn log_file() -> String {
    facts()
        .log_file
        .map_or_else(|| "none".to_owned(), |p| p.display().to_string())
}

/// The technical block for an error.
pub fn details(error: &str) -> String {
    let f = facts();
    fatal::details(&f.package, &f.data_dir, &log_file(), error)
}

/// The last fatal error's technical block, if any.
pub fn last_details() -> Option<String> {
    DETAILS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// Claims the one fatal report. False when one was shown or Owlbum is
/// quitting.
fn claim(error: &str) -> bool {
    if QUITTING.load(Ordering::SeqCst) || SHOWN.swap(true, Ordering::SeqCst) {
        return false;
    }
    *DETAILS.lock().unwrap_or_else(PoisonError::into_inner) = Some(details(error));
    true
}

/// Installs the panic hook: every panic is logged with its backtrace; a
/// fatal one is reported.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let thread = std::thread::current();
        let name = thread.name();
        let message = fatal::panic_message(info.payload());
        let place = info
            .location()
            .map(|l| format!(" at {}:{}", l.file(), l.line()))
            .unwrap_or_default();
        let backtrace = std::backtrace::Backtrace::force_capture();
        let error = format!(
            "panic in thread {}: {message}{place}\n{backtrace}",
            name.unwrap_or("unnamed")
        );
        tracing::error!(target: "app", "{error}");
        if !fatal::fatal_thread(name) {
            return;
        }
        if name == Some("main") {
            show_native(Fatal::Run, &error);
        } else {
            report_to_page(&error);
        }
    }));
}

/// A fatal error of phase `run` while the page is alive: the page shows it.
fn report_to_page(error: &str) {
    if !claim(error) {
        return;
    }
    let text = Fatal::Run.text(&facts().data_dir, cfg!(target_os = "linux"));
    match PAGE.get() {
        Some(report) => report(&AppFatal::from(&text)),
        None => native_box(Fatal::Run, error),
    }
}

/// Shows the native box for a fatal error and ends the program.
pub fn show_native(phase: Fatal, error: &str) {
    if !claim(error) {
        return;
    }
    native_box(phase, error);
}

/// Watches the page's start: if it has not called `app_startup` within
/// 30 seconds, phase `page` is fatal.
pub fn watch_page_start() {
    let _ = std::thread::Builder::new()
        .name("owlbum-page-watch".into())
        .spawn(|| {
            std::thread::sleep(std::time::Duration::from_secs(fatal::PAGE_START_WAIT_SECS));
            if !PAGE_STARTED.load(Ordering::SeqCst) {
                let error = "the page did not call app_startup within 30 seconds";
                tracing::error!(target: "app", "{error}");
                show_native(Fatal::Page, error);
            }
        });
}

/// The kind of a start failure from Tauri's error.
pub fn start_phase(error: &str) -> Fatal {
    let lower = error.to_lowercase();
    if lower.contains("webview") || lower.contains("webkit") {
        Fatal::StartNoWebview
    } else {
        Fatal::Start
    }
}

const COPY: &str = "Copy details";
const OPEN_LOG: &str = "Open the log folder";
const RESTART: &str = "Restart";
const CLOSE: &str = "Close";

/// The native fatal message box, with the technical block below the
/// message and advice. Copy details and Open the log folder keep it open;
/// Restart starts Owlbum again; Close, or closing the box, ends the
/// program with exit code 1.
fn native_box(phase: Fatal, error: &str) -> ! {
    let f = facts();
    let text = phase.text(&f.data_dir, cfg!(target_os = "linux"));
    let block = details(error);
    let last = if phase.starting() { CLOSE } else { RESTART };
    loop {
        let answer = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title(text.title)
            .set_description(format!("{}\n\n{}\n\n{block}", text.message, text.advice))
            .set_buttons(rfd::MessageButtons::YesNoCancelCustom(
                COPY.into(),
                OPEN_LOG.into(),
                last.into(),
            ))
            .show();
        match answer {
            rfd::MessageDialogResult::Custom(b) if b == COPY => {
                copy_details();
            }
            rfd::MessageDialogResult::Custom(b) if b == OPEN_LOG => {
                open_log_folder();
            }
            rfd::MessageDialogResult::Custom(b) if b == RESTART => restart(),
            _ => std::process::exit(1),
        }
    }
}

/// Puts the technical block on the clipboard.
pub fn copy_details() -> bool {
    let Some(block) = last_details() else {
        return false;
    };
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(block))
        .map_err(|e| tracing::warn!(target: "app", "the details could not be copied: {e}"))
        .is_ok()
}

/// The log's folder.
pub fn log_folder() -> Option<PathBuf> {
    facts().log_file.and_then(|p| p.parent().map(PathBuf::from))
}

/// Opens the log's folder in the file manager.
pub fn open_log_folder() -> bool {
    let Some(folder) = log_folder() else {
        return false;
    };
    tauri_plugin_opener::open_path(&folder, None::<&str>)
        .map_err(|e| tracing::warn!(target: "app", "the log folder could not be opened: {e}"))
        .is_ok()
}

/// Starts Owlbum again and ends this copy with exit code 1.
pub fn restart() -> ! {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).spawn();
    }
    std::process::exit(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_start_failure_naming_the_webview_is_phase_no_webview() {
        assert_eq!(
            start_phase("failed to create webview: WebView2 not found"),
            Fatal::StartNoWebview
        );
        assert_eq!(start_phase("something else"), Fatal::Start);
    }
}
