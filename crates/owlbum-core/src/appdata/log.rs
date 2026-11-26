//! The log file (docs/02-data.md, "The app data folder";
//! docs/08-desktop-and-release.md, "Logs").
//!
//! Code logs with `tracing`'s macros, naming its area as the target:
//! `tracing::info!(target: "album", "opened {}", root)`. [`Logger`] is the
//! one subscriber; it writes each event as one line,
//! `[<UTC time, ISO 8601 with milliseconds>] <LEVEL> <area> <message>`, to a
//! [`LogFile`].
//!
//! - The file is `<app data>/logs/owlbum.log`. If that folder cannot be
//!   written, `<system temp>/owlbum-logs/owlbum.log`; if neither works,
//!   logging is off and nothing else is affected.
//! - When the file is first opened in a run and is over 2,000,000 bytes, it
//!   is renamed `owlbum.previous.log`, replacing the older one. It is not
//!   rotated again during that run.
//! - `ERROR`, `WARN` and `INFO` are written; `DEBUG` only when
//!   `OWLBUM_LOG=debug`.
//! - A failed write is ignored. Development builds also copy each line to
//!   the console.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tracing::field::{Field, Visit};
use tracing::level_filters::LevelFilter;
use tracing::span;
use tracing::{Event, Level, Metadata, Subscriber};

use crate::appdata::clock::{Utc, now_ms};

/// The log's file name.
pub const FILE_NAME: &str = "owlbum.log";

/// The previous run's log, kept after a rotation.
pub const PREVIOUS_FILE_NAME: &str = "owlbum.previous.log";

/// The log is rotated when first opened in a run if it is over this size.
pub const ROTATE_OVER_BYTES: u64 = 2_000_000;

/// The folder inside the app data folder that holds the log.
pub const LOGS_DIR: &str = "logs";

/// The folder inside the system temp folder used when the app data folder
/// cannot be written.
pub const FALLBACK_DIR: &str = "owlbum-logs";

/// The variable that turns on `DEBUG` lines when set to `debug`.
pub const LEVEL_VAR: &str = "OWLBUM_LOG";

/// The areas a line may name (docs/08-desktop-and-release.md, "Logs").
pub const AREAS: &[&str] = &[
    "app", "album", "scan", "thumbs", "tagging", "edit", "file", "update", "page",
];

/// The folders to try for the log, in order: `<app data>/logs`, then
/// `<temp>/owlbum-logs`.
pub fn folders(app_data: &Path, temp: &Path) -> [PathBuf; 2] {
    [app_data.join(LOGS_DIR), temp.join(FALLBACK_DIR)]
}

/// Whether the value of `OWLBUM_LOG` asks for `DEBUG` lines.
pub fn debug_requested(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|v| v.eq_ignore_ascii_case("debug"))
}

/// One log line, without its line ending.
pub fn line(ms: i64, level: Level, area: &str, message: &str) -> String {
    format!(
        "[{}] {} {} {}",
        Utc::from_ms(ms).iso(),
        level.as_str(),
        area,
        message
    )
}

/// The open log file, shared by every thread. Writing never fails.
#[derive(Debug)]
pub struct LogFile {
    file: Mutex<Option<File>>,
    path: Option<PathBuf>,
    console: bool,
}

impl LogFile {
    /// Opens `owlbum.log` in the first of `folders` that can be written,
    /// creating the folder and rotating the file there as needed. With no
    /// usable folder, logging is off.
    pub fn open(folders: &[PathBuf], console: bool) -> Self {
        for dir in folders {
            if let Some((file, path)) = open_in(dir) {
                return Self {
                    file: Mutex::new(Some(file)),
                    path: Some(path),
                    console,
                };
            }
        }
        Self::off(console)
    }

    /// A log that writes no file (lines still go to the console when
    /// `console` is true).
    pub fn off(console: bool) -> Self {
        Self {
            file: Mutex::new(None),
            path: None,
            console,
        }
    }

    /// The file being written, or `None` when logging to a file is off.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Appends one line. Errors are ignored: a full disk never stops the
    /// program because of the log.
    pub fn write_line(&self, line: &str) {
        if self.console {
            // Not `eprintln!`, which panics when there is no console.
            let _ = writeln!(std::io::stderr(), "{line}");
        }
        let mut guard = match self.file.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(file) = guard.as_mut() {
            let _ = writeln!(file, "{line}");
        }
    }
}

/// Creates `dir`, rotates its log if it is too big, and opens the log for
/// appending.
fn open_in(dir: &Path) -> Option<(File, PathBuf)> {
    std::fs::create_dir_all(dir).ok()?;
    let path = dir.join(FILE_NAME);
    rotate(dir);
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok()?;
    Some((file, path))
}

/// Renames `owlbum.log` to `owlbum.previous.log` (replacing it) when the
/// log is over [`ROTATE_OVER_BYTES`]. A failure leaves the files as they
/// are.
fn rotate(dir: &Path) {
    let path = dir.join(FILE_NAME);
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > ROTATE_OVER_BYTES) {
        let previous = dir.join(PREVIOUS_FILE_NAME);
        let _ = std::fs::remove_file(&previous);
        let _ = std::fs::rename(&path, &previous);
    }
}

/// The `tracing` subscriber that writes events to a [`LogFile`]. Spans are
/// not used; only events are written.
#[derive(Debug, Clone)]
pub struct Logger {
    file: Arc<LogFile>,
    max: Level,
}

impl Logger {
    /// A logger writing `INFO` and above, or `DEBUG` and above when `debug`.
    pub fn new(file: Arc<LogFile>, debug: bool) -> Self {
        Self {
            file,
            max: if debug { Level::DEBUG } else { Level::INFO },
        }
    }

    /// The file it writes.
    pub fn file(&self) -> &Arc<LogFile> {
        &self.file
    }

    /// Makes this the process's logger. Answers false when one was already
    /// installed (the first one stays).
    pub fn install(self) -> bool {
        tracing::subscriber::set_global_default(self).is_ok()
    }
}

impl Subscriber for Logger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        *metadata.level() <= self.max
    }

    fn max_level_hint(&self) -> Option<LevelFilter> {
        Some(LevelFilter::from_level(self.max))
    }

    fn new_span(&self, _span: &span::Attributes<'_>) -> span::Id {
        span::Id::from_u64(1)
    }

    fn record(&self, _span: &span::Id, _values: &span::Record<'_>) {}

    fn record_follows_from(&self, _span: &span::Id, _follows: &span::Id) {}

    fn event(&self, event: &Event<'_>) {
        let mut message = Message::default();
        event.record(&mut message);
        let meta = event.metadata();
        self.file
            .write_line(&line(now_ms(), *meta.level(), meta.target(), &message.0));
    }

    fn enter(&self, _span: &span::Id) {}

    fn exit(&self, _span: &span::Id) {}
}

/// Collects an event's message, then its other fields as ` name=value`.
#[derive(Default)]
struct Message(String);

impl Message {
    fn add(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            let rest = std::mem::take(&mut self.0);
            self.0 = format!("{value}{rest}");
        } else {
            self.0.push_str(&format!(" {}={value}", field.name()));
        }
    }
}

impl Visit for Message {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.add(field, value);
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.add(field, &format!("{value:?}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn a_line_has_the_documented_form() {
        assert_eq!(
            line(1_795_683_903_007, Level::WARN, "album", "opened /photos"),
            "[2026-11-26T09:05:03.007Z] WARN album opened /photos"
        );
        assert_eq!(
            line(0, Level::ERROR, "app", "x"),
            "[1970-01-01T00:00:00.000Z] ERROR app x"
        );
    }

    #[test]
    fn the_log_lives_in_the_logs_folder_of_app_data() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        let log = LogFile::open(&folders(&data, &tmp.path().join("temp")), false);
        assert_eq!(
            log.path(),
            Some(data.join("logs").join("owlbum.log").as_path())
        );
        log.write_line("one");
        log.write_line("two");
        assert_eq!(read(log.path().unwrap()), "one\ntwo\n");
    }

    #[test]
    fn reopening_appends() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = folders(tmp.path(), tmp.path());
        LogFile::open(&dirs, false).write_line("first run");
        let log = LogFile::open(&dirs, false);
        log.write_line("second run");
        assert_eq!(read(log.path().unwrap()), "first run\nsecond run\n");
    }

    #[test]
    fn a_log_over_the_limit_is_rotated_when_opened() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = folders(tmp.path(), tmp.path());
        let logs = tmp.path().join("logs");
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::write(logs.join(PREVIOUS_FILE_NAME), b"older").unwrap();
        let big = vec![b'x'; usize::try_from(ROTATE_OVER_BYTES).unwrap() + 1];
        std::fs::write(logs.join(FILE_NAME), &big).unwrap();

        let log = LogFile::open(&dirs, false);
        log.write_line("new run");
        assert_eq!(read(&logs.join(FILE_NAME)), "new run\n");
        assert_eq!(
            std::fs::read(logs.join(PREVIOUS_FILE_NAME)).unwrap(),
            big,
            "the older previous log was replaced"
        );
    }

    #[test]
    fn a_log_at_exactly_the_limit_is_not_rotated() {
        let tmp = tempfile::tempdir().unwrap();
        let logs = tmp.path().join("logs");
        std::fs::create_dir_all(&logs).unwrap();
        let exact = vec![b'x'; usize::try_from(ROTATE_OVER_BYTES).unwrap()];
        std::fs::write(logs.join(FILE_NAME), &exact).unwrap();
        drop(LogFile::open(&folders(tmp.path(), tmp.path()), false));
        assert!(!logs.join(PREVIOUS_FILE_NAME).exists());
    }

    #[test]
    fn the_log_is_not_rotated_again_during_a_run() {
        let tmp = tempfile::tempdir().unwrap();
        let log = LogFile::open(&folders(tmp.path(), tmp.path()), false);
        let chunk = "y".repeat(100_000);
        for _ in 0..25 {
            log.write_line(&chunk);
        }
        let len = std::fs::metadata(log.path().unwrap()).unwrap().len();
        assert!(len > ROTATE_OVER_BYTES, "{len}");
        assert!(!tmp.path().join("logs").join(PREVIOUS_FILE_NAME).exists());
    }

    #[test]
    fn an_unwritable_app_data_folder_falls_back_to_temp() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        // A file where the logs folder should be: it cannot be created.
        std::fs::write(data.join("logs"), b"in the way").unwrap();
        let temp = tmp.path().join("temp");
        let log = LogFile::open(&folders(&data, &temp), false);
        assert_eq!(
            log.path(),
            Some(temp.join("owlbum-logs").join("owlbum.log").as_path())
        );
        log.write_line("hello");
        assert_eq!(read(log.path().unwrap()), "hello\n");
    }

    #[test]
    fn with_no_usable_folder_logging_is_off_and_writing_is_harmless() {
        let tmp = tempfile::tempdir().unwrap();
        let blocked = tmp.path().join("blocked");
        std::fs::write(&blocked, b"a file").unwrap();
        let log = LogFile::open(&[blocked.join("logs"), blocked.join("owlbum-logs")], false);
        assert_eq!(log.path(), None);
        log.write_line("goes nowhere");
    }

    #[test]
    fn owlbum_log_debug_turns_on_debug_lines() {
        use std::ffi::OsStr;
        assert!(debug_requested(Some(OsStr::new("debug"))));
        assert!(debug_requested(Some(OsStr::new("DEBUG"))));
        assert!(!debug_requested(Some(OsStr::new("info"))));
        assert!(!debug_requested(Some(OsStr::new(""))));
        assert!(!debug_requested(None));
    }

    #[test]
    fn the_logger_writes_events_as_lines_with_their_area() {
        let tmp = tempfile::tempdir().unwrap();
        let file = Arc::new(LogFile::open(&folders(tmp.path(), tmp.path()), false));
        let logger = Logger::new(Arc::clone(&file), false);
        tracing::subscriber::with_default(logger, || {
            tracing::info!(target: "album", "opened {}", "/photos/trip");
            tracing::warn!(target: "scan", count = 3, "files could not be read");
            tracing::error!(target: "app", "boom");
            tracing::debug!(target: "scan", "not written without OWLBUM_LOG=debug");
        });
        let text = read(file.path().unwrap());
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3, "{text}");
        let rest = |l: &str| l.split_once("] ").unwrap().1.to_owned();
        assert_eq!(rest(lines[0]), "INFO album opened /photos/trip");
        assert_eq!(rest(lines[1]), "WARN scan files could not be read count=3");
        assert_eq!(rest(lines[2]), "ERROR app boom");
        assert!(lines[0].starts_with('['));
        let stamp = &lines[0][1..lines[0].find(']').unwrap()];
        assert_eq!(stamp.len(), "2026-11-26T09:05:03.007Z".len(), "{stamp}");
        assert!(stamp.ends_with('Z'));
    }

    #[test]
    fn debug_lines_are_written_when_asked_for() {
        let tmp = tempfile::tempdir().unwrap();
        let file = Arc::new(LogFile::open(&folders(tmp.path(), tmp.path()), false));
        tracing::subscriber::with_default(Logger::new(Arc::clone(&file), true), || {
            tracing::debug!(target: "thumbs", "made one");
            tracing::trace!(target: "thumbs", "never");
        });
        let text = read(file.path().unwrap());
        assert_eq!(text.lines().count(), 1, "{text}");
        assert!(text.contains("DEBUG thumbs made one"), "{text}");
    }

    #[test]
    fn writes_from_many_threads_stay_whole_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let file = Arc::new(LogFile::open(&folders(tmp.path(), tmp.path()), false));
        let threads: Vec<_> = (0..8)
            .map(|t| {
                let file = Arc::clone(&file);
                std::thread::spawn(move || {
                    for i in 0..200 {
                        file.write_line(&format!("thread {t} line {i} {}", "z".repeat(50)));
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let text = read(file.path().unwrap());
        assert_eq!(text.lines().count(), 1600);
        assert!(
            text.lines()
                .all(|l| l.starts_with("thread ") && l.ends_with(&"z".repeat(50)))
        );
    }
}
