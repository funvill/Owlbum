//! Failure handling (docs/08-desktop-and-release.md, "Failure handling"):
//! the fatal error dialog's words and its technical block, and the line a
//! panic leaves in the log. Nothing is uploaded; a failure is always a
//! message.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Event;

/// The longest technical block the native box shows.
pub const DETAILS_MAX_CHARS: usize = 2_500;

/// How long the page has to call `app_startup` before phase `page` is
/// fatal.
pub const PAGE_START_WAIT_SECS: u64 = 30;

/// Where a fatal error happened, which decides its words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fatal {
    /// A panic on the main thread or in an album's writer thread.
    Run,
    /// The page did not start.
    Page,
    /// The app data folder cannot be written.
    StartDataFolder,
    /// The program's own files are missing or damaged.
    StartFilesMissing,
    /// The webview cannot be created.
    StartNoWebview,
    /// Anything else while starting.
    Start,
}

/// The dialog's title, message and advice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FatalText {
    pub title: &'static str,
    pub message: String,
    pub advice: &'static str,
}

impl Fatal {
    /// Whether the program is still starting: the last button reads
    /// `Close` and only ends it.
    pub fn starting(self) -> bool {
        !matches!(self, Self::Run | Self::Page)
    }

    /// The words of the table in docs/08, "The fatal error dialog".
    pub fn text(self, data_dir: &str, linux: bool) -> FatalText {
        const START: &str = "Owlbum could not start";
        match self {
            Self::Run => FatalText {
                title: "Owlbum stopped working",
                message: "The part of Owlbum that reads your photos stopped unexpectedly. Stars and notes up to your last change are saved.".into(),
                advice: "Restart Owlbum. If this keeps happening, send the details below as feedback on GitHub.",
            },
            Self::Page => FatalText {
                title: "Owlbum could not show its window",
                message: "Owlbum started, but its window could not load.".into(),
                advice: "Restart Owlbum. If this keeps happening, reinstall it.",
            },
            Self::StartDataFolder => FatalText {
                title: START,
                message: format!("Owlbum could not save to its data folder:\n{data_dir}"),
                advice: "Check that the disk is not full and that you are allowed to save files in that folder, then start Owlbum again.",
            },
            Self::StartFilesMissing => FatalText {
                title: START,
                message: "Some of Owlbum's own files are missing or damaged.".into(),
                advice: "Reinstall Owlbum.",
            },
            Self::StartNoWebview => FatalText {
                title: START,
                message: "Owlbum could not open its window. A part of the system that Owlbum needs to draw its window is missing.".into(),
                advice: if linux {
                    "Install your system's WebKitGTK 4.1 package, then start Owlbum again."
                } else {
                    "Reinstall Owlbum; the installer adds what is missing."
                },
            },
            Self::Start => FatalText {
                title: START,
                message: "Owlbum ran into a problem while starting.".into(),
                advice: "Start it again. If this keeps happening, reinstall Owlbum or send the details below as feedback on GitHub.",
            },
        }
    }
}

/// The `app_fatal` event: a fatal error of phase `run` the page shows.
/// `message` holds the message and, after a blank line, the advice. The
/// technical block is not sent: `Copy details` asks Rust for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AppFatal {
    pub title: String,
    pub message: String,
}

impl Event for AppFatal {
    const NAME: &'static str = "app_fatal";
}

impl From<&FatalText> for AppFatal {
    fn from(t: &FatalText) -> Self {
        Self {
            title: t.title.to_owned(),
            message: format!("{}\n\n{}", t.message, t.advice),
        }
    }
}

/// The technical block: version, platform, data folder, log file and the
/// error with its backtrace, cut to [`DETAILS_MAX_CHARS`].
pub fn details(package: &str, data_dir: &str, log_file: &str, error: &str) -> String {
    let block = format!(
        "Owlbum {} ({} {}, {package})\nData folder: {data_dir}\nLog file: {log_file}\n\n{error}",
        crate::app::version().version,
        std::env::consts::OS,
        std::env::consts::ARCH,
    );
    if block.chars().count() <= DETAILS_MAX_CHARS {
        block
    } else {
        let mut cut: String = block.chars().take(DETAILS_MAX_CHARS - 1).collect();
        cut.push('…');
        cut
    }
}

/// A panic's message, whatever its payload.
pub fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic with no message".to_owned())
}

/// Whether a panic on this thread is fatal: the main thread, or an album's
/// writer thread outside its jobs (a job's own panic is caught and rolled
/// back). Panics in scan, thumbnail, tagging or render jobs are only
/// logged.
pub fn fatal_thread(name: Option<&str>) -> bool {
    matches!(name, Some("main" | "owlbum-album-writer"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_phase_has_its_words() {
        let run = Fatal::Run.text("D", false);
        assert_eq!(run.title, "Owlbum stopped working");
        assert!(!Fatal::Run.starting());
        let data = Fatal::StartDataFolder.text("C:\\data", false);
        assert_eq!(
            data.message,
            "Owlbum could not save to its data folder:\nC:\\data"
        );
        assert!(Fatal::StartDataFolder.starting());
        assert!(
            Fatal::StartNoWebview
                .text("", true)
                .advice
                .contains("WebKitGTK")
        );
        assert!(
            Fatal::StartNoWebview
                .text("", false)
                .advice
                .contains("installer")
        );
    }

    #[test]
    fn the_event_carries_the_message_and_the_advice() {
        let e = AppFatal::from(&Fatal::Page.text("", false));
        assert_eq!(
            e.message,
            "Owlbum started, but its window could not load.\n\nRestart Owlbum. If this keeps happening, reinstall it."
        );
    }

    #[test]
    fn the_technical_block_names_the_build_and_is_cut_to_its_limit() {
        let d = details("nsis", "C:\\data", "C:\\data\\logs\\owlbum.log", "boom");
        assert!(d.starts_with(&format!("Owlbum {} (", crate::app::version().version)));
        assert!(d.contains("Data folder: C:\\data\nLog file: C:\\data\\logs\\owlbum.log\n\nboom"));
        let long = details("nsis", "d", "l", &"x".repeat(5_000));
        assert_eq!(long.chars().count(), DETAILS_MAX_CHARS);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn a_panic_message_is_read_from_either_payload() {
        assert_eq!(panic_message(&"static"), "static");
        assert_eq!(panic_message(&String::from("owned")), "owned");
        assert_eq!(panic_message(&7_u8), "a panic with no message");
    }

    #[test]
    fn only_the_main_and_writer_threads_are_fatal() {
        assert!(fatal_thread(Some("main")));
        assert!(fatal_thread(Some("owlbum-album-writer")));
        assert!(!fatal_thread(Some("owlbum-scan")));
        assert!(!fatal_thread(None));
    }
}
