//! What an open album shares between its parts while it is open: the
//! revision, the status listener, and whether a write has failed.
//!
//! The album database, the lock's heartbeat thread and the folder watch
//! each hold an `Arc` of it, so none of them needs the [`super::Album`]
//! itself. A change to the album's state (the lock taken over, a write the
//! database could not take, the folder no longer watched) raises the
//! revision and tells the listener, which the shell turns into
//! `album_changed` with reason `status` (docs/04-browsing.md, "Events").

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::{AppError, ErrorCode};

/// Told the album's new revision each time its state changes. It runs on
/// whichever thread saw the change (the lock's heartbeat, the writer's
/// caller, the watch), so it must only pass the news on: it must not wait
/// for the album, its scanner or the session that holds them.
pub type StatusListener = Arc<dyn Fn(u64) + Send + Sync>;

/// The shared state of one open album.
#[derive(Default)]
pub struct AlbumState {
    revision: AtomicU64,
    write_failed: AtomicBool,
    listener: Mutex<Option<StatusListener>>,
}

impl std::fmt::Debug for AlbumState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AlbumState")
            .field("revision", &self.revision())
            .field("write_failed", &self.write_failed())
            .finish_non_exhaustive()
    }
}

impl AlbumState {
    /// The revision now.
    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::SeqCst)
    }

    /// Raises the revision and answers the new one.
    pub fn next_revision(&self) -> u64 {
        self.revision.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Sets the listener, replacing any other.
    pub fn set_listener(&self, listener: StatusListener) {
        *self.listener.lock().unwrap_or_else(|p| p.into_inner()) = Some(listener);
    }

    /// The album's state changed: raises the revision and tells the
    /// listener, if there is one.
    pub fn status_changed(&self) {
        let revision = self.next_revision();
        let listener = self
            .listener
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        if let Some(listener) = listener {
            listener(revision);
        }
    }

    /// True once a write failed because the database could not take it.
    pub fn write_failed(&self) -> bool {
        self.write_failed.load(Ordering::SeqCst)
    }

    /// Looks at a failed write's error. When it says the database could
    /// not be written ([`is_storage_failure`]), the album is view-only from
    /// then on (docs/08-desktop-and-release.md, "Failure handling"); the
    /// first such failure is a status change.
    pub fn write_answered(&self, error: &AppError) {
        if is_storage_failure(error) && !self.write_failed.swap(true, Ordering::SeqCst) {
            tracing::error!(target: "album", "the album database could not be written; the album is view-only now: {error:?}");
            self.status_changed();
        }
    }
}

/// The SQLite result codes that mean the database cannot be written:
/// `SQLITE_READONLY` (8), `SQLITE_IOERR` (10), `SQLITE_FULL` (13) and
/// `SQLITE_CANTOPEN` (14). A busy or locked database, a constraint and
/// every other code are the job's own failure and leave the album writable.
const STORAGE_FAILURES: [i64; 4] = [8, 10, 13, 14];

/// True when `error` came from SQLite ([`super::sqlite_error`]) with one of
/// the result codes that mean the database cannot be written.
pub fn is_storage_failure(error: &AppError) -> bool {
    error.code == ErrorCode::Internal
        && error
            .details
            .as_ref()
            .and_then(|d| d.get(super::SQLITE_CODE))
            .and_then(serde_json::Value::as_i64)
            .is_some_and(|code| STORAGE_FAILURES.contains(&(code & 0xff)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::album::sqlite_error;

    fn failure(code: std::ffi::c_int) -> AppError {
        sqlite_error("x")(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(code),
            None,
        ))
    }

    #[test]
    fn only_the_codes_that_mean_the_disk_refused_are_storage_failures() {
        use rusqlite::ffi;
        for code in [
            ffi::SQLITE_READONLY,
            ffi::SQLITE_IOERR,
            ffi::SQLITE_IOERR_WRITE,
            ffi::SQLITE_FULL,
            ffi::SQLITE_CANTOPEN,
        ] {
            assert!(is_storage_failure(&failure(code)), "{code}");
        }
        for code in [
            ffi::SQLITE_BUSY,
            ffi::SQLITE_LOCKED,
            ffi::SQLITE_CONSTRAINT,
            ffi::SQLITE_CONSTRAINT_UNIQUE,
            ffi::SQLITE_CORRUPT,
            ffi::SQLITE_ERROR,
        ] {
            assert!(!is_storage_failure(&failure(code)), "{code}");
        }
        assert!(!is_storage_failure(&AppError::internal("x")));
        assert!(!is_storage_failure(&sqlite_error("x")(
            rusqlite::Error::QueryReturnedNoRows
        )));
    }

    #[test]
    fn the_listener_hears_each_change_with_its_revision() {
        let state = AlbumState::default();
        state.status_changed();
        let heard = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&heard);
        state.set_listener(Arc::new(move |r| sink.lock().unwrap().push(r)));
        state.status_changed();
        assert_eq!(*heard.lock().unwrap(), [2]);
        assert_eq!(state.revision(), 2);
    }
}
