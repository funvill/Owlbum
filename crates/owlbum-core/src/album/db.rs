//! Creating and opening the album database (docs/02-data.md, "Where it
//! lives" and "Opening, locking and view-only").
//!
//! The database is opened with `journal_mode = DELETE` and
//! `synchronous = FULL`; write-ahead logging is never used, so between
//! sessions the only file is `owlbum.sqlite`.
//!
//! This module does not decide whether an album may be written: the lock
//! file, the daily backup and the damage check are album safety
//! ([`super::open`]), which runs between [`super::locate`] and
//! [`AlbumDb::open`] and makes a view-only [`AlbumDb`] when it must.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, Transaction};

use crate::album::read::Readers;
use crate::album::root::AlbumLocation;
use crate::album::schema::{self, SchemaState};
use crate::album::sqlite_error;
use crate::album::state::AlbumState;
use crate::album::writer::Writer;
use crate::error::AppResult;
use crate::text;
use crate::{AppError, ErrorCode};

/// How long a connection waits on SQLite's file lock for another program
/// (a backup tool, a database browser) to finish. Inside this program reads
/// and write transactions take turns on a [`Gate`] instead, so they never
/// wait on the file lock for each other.
pub const BUSY_TIMEOUT: Duration = Duration::from_secs(10);

/// Turns between this program's reads and its writer (#93). Every read
/// holds it shared; each write transaction holds it alone, from `BEGIN` to
/// the commit.
///
/// Without it a read that arrives while the writer commits polls SQLite's
/// busy handler, which sleeps up to 100 ms between tries, while the writer
/// starts its next commit within microseconds of the last. With
/// `journal_mode = DELETE` a commit locks out readers for its whole sync, so
/// during a scan's back-to-back batches a read could miss every gap until
/// [`BUSY_TIMEOUT`] ran out and fail with "database is locked". A lock
/// queues the read instead: it runs as soon as the commit ends.
///
/// Never read inside a read or a write job, or write inside a read: the
/// inner call would wait for the outer one.
pub type Gate = std::sync::Arc<std::sync::RwLock<()>>;

/// The result of opening an album database.
#[derive(Debug)]
pub enum Opened {
    /// The database is open, its schema current, its writer running.
    Ready(AlbumDb),
    /// The database was made by a newer Owlbum. It was not changed, and no
    /// writer was started: `db` is view-only (reads work, writes answer
    /// `view_only`).
    Newer {
        /// The file's schema version.
        version: u32,
        /// The database, open for reading only.
        db: AlbumDb,
    },
}

/// An open album database: its writer thread and its readers.
///
/// A view-only database has readers and no writer; [`AlbumDb::write`]
/// answers `view_only` with the first sentence of the banner in force.
#[derive(Debug)]
pub struct AlbumDb {
    path: Option<PathBuf>,
    album_id: String,
    schema: SchemaState,
    // Field order matters on drop: readers close before the writer's thread
    // is joined, so the last connection to close is the writer's.
    readers: Readers,
    writer: Option<Writer>,
    /// Why writes are refused: the `view_only` error's message.
    view_only: Option<String>,
    /// Set to the other computer's name once the album's lock was taken
    /// over (Open anyway there): writes are refused from then on.
    lost: Option<LostLock>,
    /// The open album's shared state: a write the database could not take
    /// is reported to it, and refuses writes from then on.
    state: Option<Arc<AlbumState>>,
}

/// Shared with the lock's heartbeat ([`super::lock::AlbumLock::lost_to`]):
/// the name of the computer that took the album over, once one has.
pub type LostLock = std::sync::Arc<std::sync::Mutex<Option<String>>>;

impl AlbumDb {
    /// Opens the database of the album at `location`, creating
    /// `.owlbum/owlbum.sqlite` when the root has none, and brings its schema
    /// up to date.
    pub fn open(location: &AlbumLocation) -> AppResult<Opened> {
        Self::open_at(&location.root)
    }

    /// Opens (or creates) the database of the album rooted at `root`.
    pub fn open_at(root: &Path) -> AppResult<Opened> {
        let path = super::root::database_path(root);
        if !path.is_file() {
            create_owlbum_dir(root)?;
        }
        let mut conn = open_writable(&path)?;
        let schema = schema::migrate(&mut conn)?;
        if let SchemaState::Newer { version } = schema {
            drop(conn);
            let db = Self::view_only(&path, text::album::view_only::NEWER)?;
            return Ok(Opened::Newer { version, db });
        }
        let album_id = schema::meta(&conn, "album_id")?
            .ok_or_else(|| AppError::internal(text::album::OPENING_DATABASE))?;
        let readers = Readers::new(&path);
        let writer = Writer::start_gated(conn, readers.gate())?;
        Ok(Opened::Ready(Self {
            readers,
            writer: Some(writer),
            path: Some(path),
            album_id,
            schema,
            view_only: None,
            lost: None,
            state: None,
        }))
    }

    /// Opens the database at `path` for reading only. Nothing is written to
    /// it; writes answer `view_only` with `message`. When the file's
    /// `album_id` cannot be read (a newer or damaged schema), a fresh one
    /// names the album for this session.
    pub fn view_only(path: &Path, message: &str) -> AppResult<Self> {
        let readers = Readers::new(path);
        let (version, album_id) = readers
            .read(|conn| {
                let version = schema::version(conn)?;
                Ok((version, schema::meta(conn, "album_id").ok().flatten()))
            })
            .unwrap_or((0, None));
        let schema = if version > schema::CURRENT_VERSION {
            SchemaState::Newer { version }
        } else {
            SchemaState::Current {
                version,
                from: version,
            }
        };
        Ok(Self {
            path: Some(path.to_path_buf()),
            album_id: album_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            schema,
            readers,
            writer: None,
            view_only: Some(message.to_owned()),
            lost: None,
            state: None,
        })
    }

    /// A view-only database in memory, with the current schema and no rows,
    /// for a folder that has no album and cannot be written. Nothing is
    /// saved.
    pub fn in_memory(message: &str) -> AppResult<Self> {
        let mut conn =
            Connection::open_in_memory().map_err(sqlite_error(text::album::OPENING_DATABASE))?;
        conn.pragma_update(None, "foreign_keys", true)
            .map_err(sqlite_error(text::album::OPENING_DATABASE))?;
        let schema = schema::migrate(&mut conn)?;
        let album_id = schema::meta(&conn, "album_id")?
            .ok_or_else(|| AppError::internal(text::album::OPENING_DATABASE))?;
        Ok(Self {
            path: None,
            album_id,
            schema,
            readers: Readers::shared(conn),
            writer: None,
            view_only: Some(message.to_owned()),
            lost: None,
            state: None,
        })
    }

    /// Refuses writes with the `locked` view-only message once `lost` names
    /// a computer (the album's lock was taken over).
    pub fn refuse_writes_when(&mut self, lost: LostLock) {
        self.lost = Some(lost);
    }

    /// Reports every write the database could not take to `state`, which
    /// turns the album view-only ([`AlbumState::write_answered`]).
    pub fn attach(&mut self, state: Arc<AlbumState>) {
        self.state = Some(state);
    }

    /// The database file; None for an in-memory database.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Why writes are refused right now, as the `view_only` message; None
    /// when the database can be written.
    pub fn view_only_message(&self) -> Option<String> {
        if let Some(message) = &self.view_only {
            return Some(message.clone());
        }
        if self.state.as_ref().is_some_and(|s| s.write_failed()) {
            return Some(text::album::view_only::READ_ONLY.to_owned());
        }
        let lost = self.lost.as_ref()?;
        let host = lost.lock().unwrap_or_else(|p| p.into_inner()).clone()?;
        Some(text::album::view_only::locked(&host))
    }

    /// The album's UUID from `meta`. It names the album in thumbnail keys.
    pub fn album_id(&self) -> &str {
        &self.album_id
    }

    /// The schema version, and the version the file had before this open.
    pub fn schema(&self) -> SchemaState {
        self.schema
    }

    /// Runs `job` in a transaction on the writer thread; answers after the
    /// commit.
    pub fn write<T, F>(&self, job: F) -> AppResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&Transaction<'_>) -> AppResult<T> + Send + 'static,
    {
        match (&self.writer, self.view_only_message()) {
            (Some(writer), None) => self.answered(writer.write(job)),
            (_, message) => Err(AppError::new(
                ErrorCode::ViewOnly,
                message.unwrap_or_else(|| text::album::view_only::READ_ONLY.to_owned()),
            )),
        }
    }

    /// [`AlbumDb::write`], then `after` on the writer thread once the job
    /// has committed and before this answers ([`Writer::write_then`]).
    pub fn write_then<T, F, A>(&self, job: F, after: A) -> AppResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&Transaction<'_>) -> AppResult<T> + Send + 'static,
        A: FnOnce(&Connection, &T) + Send + 'static,
    {
        match (&self.writer, self.view_only_message()) {
            (Some(writer), None) => self.answered(writer.write_then(job, after)),
            (_, message) => Err(AppError::new(
                ErrorCode::ViewOnly,
                message.unwrap_or_else(|| text::album::view_only::READ_ONLY.to_owned()),
            )),
        }
    }

    /// Tells the album's state about a failed write.
    fn answered<T>(&self, result: AppResult<T>) -> AppResult<T> {
        if let (Err(e), Some(state)) = (&result, &self.state) {
            state.write_answered(e);
        }
        result
    }

    /// Runs `f` with a read-only connection.
    pub fn read<T>(&self, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T> {
        self.readers.read(f)
    }
}

/// Opens the album database for writing, creating the file when it does not
/// exist, and sets the connection up as the album database requires.
pub fn open_writable(path: &Path) -> AppResult<Connection> {
    let failed = sqlite_error(text::album::OPENING_DATABASE);
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(&failed)?;
    configure(&conn).map_err(&failed)?;
    Ok(conn)
}

/// The settings every writable connection to an album database gets. They
/// are set outside any transaction, where SQLite allows them to change.
pub fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.busy_timeout(BUSY_TIMEOUT)?;
    // `journal_mode` answers the mode in force; the pragma is a query.
    let _mode: String =
        conn.pragma_update_and_check(None, "journal_mode", "DELETE", |r| r.get(0))?;
    conn.pragma_update(None, "synchronous", "FULL")?;
    conn.pragma_update(None, "foreign_keys", true)?;
    Ok(())
}

/// Creates `<root>/.owlbum` (and nothing else) if it is missing. On Windows
/// the folder is given the hidden attribute when it is created.
fn create_owlbum_dir(root: &Path) -> AppResult<PathBuf> {
    make_owlbum_dir(root).map_err(|e| {
        AppError::internal(text::album::CREATING_DATABASE)
            .with_details(serde_json::json!({ "cause": e.to_string() }))
    })
}

/// [`create_owlbum_dir`] with the I/O error kept, so album safety can tell a
/// folder that cannot be written from other failures.
pub(crate) fn make_owlbum_dir(root: &Path) -> std::io::Result<PathBuf> {
    let dir = root.join(super::root::OWLBUM_DIR);
    if dir.is_dir() {
        return Ok(dir);
    }
    std::fs::create_dir(&dir)?;
    hide(&dir);
    Ok(dir)
}

/// Gives `dir` the hidden attribute. Failing to hide it is not an error: the
/// folder works the same, and the scanner never looks inside it.
#[cfg(windows)]
#[allow(unsafe_code)]
fn hide(dir: &Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_HIDDEN, GetFileAttributesW, INVALID_FILE_ATTRIBUTES, SetFileAttributesW,
    };

    let wide: Vec<u16> = dir.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: `wide` is a NUL-terminated UTF-16 path that outlives both
    // calls; neither call keeps the pointer.
    unsafe {
        let attrs = GetFileAttributesW(wide.as_ptr());
        if attrs != INVALID_FILE_ATTRIBUTES {
            SetFileAttributesW(wide.as_ptr(), attrs | FILE_ATTRIBUTE_HIDDEN);
        }
    }
}

/// On Linux the leading dot already hides `.owlbum`.
#[cfg(not(windows))]
fn hide(_dir: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::album::locate;
    use crate::album::schema::CURRENT_VERSION;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn ready(opened: Opened) -> AlbumDb {
        match opened {
            Opened::Ready(db) => db,
            Opened::Newer { version, .. } => panic!("newer: {version}"),
        }
    }

    fn files_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn insert_photo(tx: &Transaction<'_>, rel_path: &str) -> AppResult<i64> {
        tx.execute(
            "INSERT INTO photos (rel_path, rel_dir, file_name, size, mtime_ms, file_key, thumb_key, kind, format, width, height, scan_version)
             VALUES (?1, '', ?1, 1, 1, 'fk', 'tk', 'image', 'jpeg', 1, 1, 1)",
            [rel_path],
        )
        .map_err(sqlite_error(text::album::WRITING_DATABASE))?;
        Ok(tx.last_insert_rowid())
    }

    #[test]
    fn opening_a_new_folder_creates_the_database_with_the_current_schema() {
        let tmp = tempfile::tempdir().unwrap();
        let loc = locate(tmp.path()).unwrap();
        assert!(!loc.exists);

        let db = ready(AlbumDb::open(&loc).unwrap());
        assert_eq!(db.path(), Some(loc.database_path().as_path()));
        assert!(loc.database_path().is_file());
        assert_eq!(
            db.schema(),
            SchemaState::Current {
                version: CURRENT_VERSION,
                from: 0
            }
        );
        assert!(uuid::Uuid::parse_str(db.album_id()).is_ok());
        assert!(locate(tmp.path()).unwrap().exists);
    }

    #[test]
    fn reopening_keeps_the_album_id_and_the_rows() {
        let tmp = tempfile::tempdir().unwrap();
        let (id, photo) = {
            let db = ready(AlbumDb::open_at(tmp.path()).unwrap());
            let photo = db.write(|tx| insert_photo(tx, "a.jpg")).unwrap();
            (db.album_id().to_owned(), photo)
        };
        let db = ready(AlbumDb::open_at(tmp.path()).unwrap());
        assert_eq!(db.album_id(), id);
        assert_eq!(
            db.schema(),
            SchemaState::Current {
                version: CURRENT_VERSION,
                from: CURRENT_VERSION
            }
        );
        let path: String = db
            .read(|c| {
                c.query_row("SELECT rel_path FROM photos WHERE id = ?1", [photo], |r| {
                    r.get(0)
                })
                .map_err(sqlite_error(text::album::READING_DATABASE))
            })
            .unwrap();
        assert_eq!(path, "a.jpg");
    }

    #[test]
    fn connections_use_the_rollback_journal_full_sync_and_foreign_keys() {
        let tmp = tempfile::tempdir().unwrap();
        let db = ready(AlbumDb::open_at(tmp.path()).unwrap());
        let (mode, sync, fk) = db
            .write(|tx| {
                let q = |p: &str| -> String {
                    tx.pragma_query_value(None, p, |r| r.get::<_, rusqlite::types::Value>(0))
                        .map(|v| match v {
                            rusqlite::types::Value::Integer(i) => i.to_string(),
                            rusqlite::types::Value::Text(t) => t,
                            other => format!("{other:?}"),
                        })
                        .unwrap()
                };
                Ok((q("journal_mode"), q("synchronous"), q("foreign_keys")))
            })
            .unwrap();
        assert_eq!(mode.to_lowercase(), "delete");
        assert_eq!(sync, "2", "FULL");
        assert_eq!(fk, "1");
    }

    #[test]
    fn foreign_keys_cascade_through_the_writer() {
        let tmp = tempfile::tempdir().unwrap();
        let db = ready(AlbumDb::open_at(tmp.path()).unwrap());
        let n = db
            .write(|tx| {
                let id = insert_photo(tx, "a.jpg")?;
                tx.execute(
                    "INSERT INTO photo_labels VALUES ('keyword', 'sea', ?1)",
                    [id],
                )
                .unwrap();
                tx.execute("DELETE FROM photos WHERE id = ?1", [id])
                    .unwrap();
                Ok(tx
                    .query_row("SELECT count(*) FROM photo_labels", [], |r| {
                        r.get::<_, i64>(0)
                    })
                    .unwrap())
            })
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn after_close_the_owlbum_folder_holds_only_the_database() {
        let tmp = tempfile::tempdir().unwrap();
        {
            let db = ready(AlbumDb::open_at(tmp.path()).unwrap());
            db.write(|tx| insert_photo(tx, "a.jpg")).unwrap();
            db.read(|c| {
                c.query_row("SELECT count(*) FROM photos", [], |r| r.get::<_, i64>(0))
                    .map_err(sqlite_error(text::album::READING_DATABASE))
            })
            .unwrap();
        }
        assert_eq!(files_in(&tmp.path().join(".owlbum")), ["owlbum.sqlite"]);
        // Every handle is closed, so the folder can be removed (Windows
        // refuses while a file in it is open).
        std::fs::remove_dir_all(tmp.path().join(".owlbum")).unwrap();
    }

    #[test]
    fn opening_creates_nothing_but_the_owlbum_folder() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.jpg"), b"x").unwrap();
        drop(AlbumDb::open_at(tmp.path()).unwrap());
        assert_eq!(files_in(tmp.path()), [".owlbum", "a.jpg"]);
    }

    #[cfg(windows)]
    #[test]
    fn the_owlbum_folder_is_hidden_on_windows() {
        use std::os::windows::fs::MetadataExt;
        let tmp = tempfile::tempdir().unwrap();
        drop(AlbumDb::open_at(tmp.path()).unwrap());
        let attrs = std::fs::metadata(tmp.path().join(".owlbum"))
            .unwrap()
            .file_attributes();
        assert_ne!(attrs & 0x2, 0, "FILE_ATTRIBUTE_HIDDEN");
    }

    #[test]
    fn an_empty_owlbum_folder_is_reused() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join(".owlbum")).unwrap();
        let db = ready(AlbumDb::open_at(tmp.path()).unwrap());
        assert_eq!(
            db.schema(),
            SchemaState::Current {
                version: CURRENT_VERSION,
                from: 0
            }
        );
    }

    #[test]
    fn a_newer_database_is_not_changed() {
        let tmp = tempfile::tempdir().unwrap();
        drop(AlbumDb::open_at(tmp.path()).unwrap());
        let path = super::super::root::database_path(tmp.path());
        Connection::open(&path)
            .unwrap()
            .pragma_update(None, "user_version", CURRENT_VERSION + 1)
            .unwrap();
        let before = std::fs::read(&path).unwrap();

        match AlbumDb::open_at(tmp.path()).unwrap() {
            Opened::Newer { version, db } => {
                assert_eq!(version, CURRENT_VERSION + 1);
                assert_eq!(count_photos(&db), 0, "reads work");
                let err = db.write(|tx| insert_photo(tx, "a.jpg")).unwrap_err();
                assert_eq!(err.code, crate::ErrorCode::ViewOnly);
                assert_eq!(err.message, text::album::view_only::NEWER);
            }
            Opened::Ready(_) => panic!("a newer database must not open for writing"),
        }
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(files_in(&tmp.path().join(".owlbum")), ["owlbum.sqlite"]);
    }

    fn count_photos(db: &AlbumDb) -> i64 {
        db.read(|c| {
            c.query_row("SELECT count(*) FROM photos", [], |r| r.get(0))
                .map_err(sqlite_error(text::album::READING_DATABASE))
        })
        .unwrap()
    }

    #[test]
    fn a_view_only_database_reads_and_refuses_writes() {
        let tmp = tempfile::tempdir().unwrap();
        let id = {
            let db = ready(AlbumDb::open_at(tmp.path()).unwrap());
            db.write(|tx| insert_photo(tx, "a.jpg")).unwrap();
            db.album_id().to_owned()
        };
        let path = super::super::root::database_path(tmp.path());
        let before = std::fs::read(&path).unwrap();
        let db = AlbumDb::view_only(&path, "No.").unwrap();
        assert_eq!(db.album_id(), id);
        assert_eq!(db.view_only_message().as_deref(), Some("No."));
        assert_eq!(count_photos(&db), 1);
        let err = db.write(|tx| insert_photo(tx, "b.jpg")).unwrap_err();
        assert_eq!(err.code, crate::ErrorCode::ViewOnly);
        assert_eq!(err.message, "No.");
        drop(db);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(files_in(&tmp.path().join(".owlbum")), ["owlbum.sqlite"]);
    }

    #[test]
    fn an_in_memory_database_has_the_schema_and_no_file() {
        let db = AlbumDb::in_memory("Nothing is kept.").unwrap();
        assert_eq!(db.path(), None);
        assert!(uuid::Uuid::parse_str(db.album_id()).is_ok());
        assert_eq!(count_photos(&db), 0);
        // Several threads can read it at once.
        let db = Arc::new(db);
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let db = Arc::clone(&db);
                std::thread::spawn(move || count_photos(&db))
            })
            .collect();
        for t in threads {
            assert_eq!(t.join().unwrap(), 0);
        }
        let err = db.write(|tx| insert_photo(tx, "a.jpg")).unwrap_err();
        assert_eq!(err.code, crate::ErrorCode::ViewOnly);
        assert_eq!(err.message, "Nothing is kept.");
    }

    #[test]
    fn a_lost_lock_refuses_writes_from_then_on() {
        let tmp = tempfile::tempdir().unwrap();
        let mut db = ready(AlbumDb::open_at(tmp.path()).unwrap());
        let lost: LostLock = Default::default();
        db.refuse_writes_when(Arc::clone(&lost));
        db.write(|tx| insert_photo(tx, "a.jpg")).unwrap();
        assert_eq!(db.view_only_message(), None);
        *lost.lock().unwrap() = Some("THERE".into());
        let taken = text::album::view_only::locked("THERE");
        assert_eq!(db.view_only_message(), Some(taken.clone()));
        let err = db.write(|tx| insert_photo(tx, "b.jpg")).unwrap_err();
        assert_eq!(err.message, taken);
        assert_eq!(count_photos(&db), 1);
    }

    #[test]
    fn readers_and_the_writer_work_at_the_same_time() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(ready(AlbumDb::open_at(tmp.path()).unwrap()));
        // Every connection gets a busy timeout far shorter than the writes
        // below take, so a read or a commit that waits on SQLite's lock
        // instead of taking its turn fails here every time, not only on a
        // loaded machine (#93).
        let short = Duration::from_millis(20);
        let pooled = Arc::new(std::sync::Barrier::new(crate::album::read::POOL_SIZE));
        let opening: Vec<_> = (0..crate::album::read::POOL_SIZE)
            .map(|_| {
                let db = Arc::clone(&db);
                let pooled = Arc::clone(&pooled);
                std::thread::spawn(move || {
                    db.read(|c| {
                        c.busy_timeout(short).unwrap();
                        pooled.wait();
                        Ok(())
                    })
                    .unwrap()
                })
            })
            .collect();
        for t in opening {
            t.join().unwrap();
        }
        db.write(move |tx| {
            tx.busy_timeout(short).unwrap();
            Ok(())
        })
        .unwrap();

        let stop = Arc::new(AtomicBool::new(false));
        let readers: Vec<_> = (0..crate::album::read::POOL_SIZE)
            .map(|_| {
                let db = Arc::clone(&db);
                let stop = Arc::clone(&stop);
                std::thread::spawn(move || {
                    let mut reads = 0;
                    while !stop.load(Ordering::Relaxed) {
                        db.read(|c| {
                            c.query_row("SELECT count(*) FROM photos", [], |r| r.get::<_, i64>(0))
                                .map_err(sqlite_error(text::album::READING_DATABASE))
                        })
                        .expect("a read never fails while the writer commits");
                        reads += 1;
                    }
                    reads
                })
            })
            .collect();
        for i in 0..200 {
            db.write(move |tx| insert_photo(tx, &format!("{i}.jpg")))
                .expect("a write never fails while readers read");
        }
        stop.store(true, Ordering::Relaxed);
        for r in readers {
            assert!(r.join().unwrap() > 0);
        }
        let n = db
            .read(|c| {
                c.query_row("SELECT count(*) FROM photos", [], |r| r.get::<_, i64>(0))
                    .map_err(sqlite_error(text::album::READING_DATABASE))
            })
            .unwrap();
        assert_eq!(n, 200);
    }
}
