//! The album database (docs/02-data.md, "The album database").
//!
//! [`Album::open`] is the way in: it runs album safety ([`open`]: the lock
//! file, view-only, the damage check, the daily backup) around three steps
//! that are kept apart for it:
//!
//! 1. [`root::locate`] finds the album root for the folder the user opened,
//!    and the scope inside it. It reads only; it creates nothing.
//! 2. [`db::AlbumDb::open`] creates `.owlbum/owlbum.sqlite` when there is
//!    none, opens it, and brings its schema up to date ([`schema`]). A
//!    database made by a newer Owlbum is reported, not changed.
//! 3. The open database has one [`writer::Writer`] thread that does every
//!    write, and a small pool of read-only connections ([`read::Readers`]).

pub mod backup;
pub mod check;
pub mod db;
pub mod lock;
pub mod open;
pub mod read;
pub mod root;
pub mod schema;
pub mod writer;

pub use db::{AlbumDb, Opened};
pub use open::{Album, OpenOptions, Safety, ViewOnly};
pub use root::{AlbumLocation, locate};

/// Maps a SQLite error to an `internal` error that says what was being
/// done. The message for the user names only `what`; SQLite's own text is
/// kept in `details.cause` for the log (there is no logger yet, #5).
pub(crate) fn sqlite_error(what: &'static str) -> impl Fn(rusqlite::Error) -> crate::AppError {
    move |e| {
        crate::AppError::internal(what).with_details(serde_json::json!({ "cause": e.to_string() }))
    }
}
