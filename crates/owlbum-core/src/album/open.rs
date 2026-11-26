//! Opening an album safely (docs/02-data.md, "Opening, locking and
//! view-only" and "Backups and damage").
//!
//! [`Album::open`] runs the steps in this order, so that a view-only open
//! writes nothing in the album folder:
//!
//! 1. Find the root ([`super::locate`]) and look for a sync tool's copy of
//!    the database beside it ([`check::conflict_file`]).
//! 2. Read the database file's header. A newer schema opens view-only
//!    (`newer`) without touching the lock.
//! 3. A database file that cannot be written opens view-only
//!    (`read_only`).
//! 4. Judge the lock file ([`lock::judge`]): a live lock of another Owlbum
//!    opens view-only (`locked`); an old one is asked about
//!    (`album_lock_stale`) unless the options already answer. Otherwise take
//!    the lock. A folder where `.owlbum` or the lock cannot be created opens
//!    view-only (`read_only`), in memory when there is no database.
//! 5. With the lock held, run `PRAGMA quick_check`. A damaged database is
//!    answered with `album_damaged`, or restored from the newest sound
//!    backup when asked.
//! 6. Make the day's backup ([`backup::daily`]), then open the database and
//!    migrate it ([`AlbumDb::open`]).
//!
//! When several reasons for view-only apply, the first in that order wins:
//! `newer`, `read_only`, `locked`, `chosen`, `damaged`.

use std::io;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use chrono::{DateTime, Local, TimeZone};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::album::check::{self, Header};
use crate::album::db::{self, AlbumDb, Opened};
use crate::album::lock::{self, AlbumLock, Owner, Verdict};
use crate::album::root::AlbumLocation;
use crate::album::schema::CURRENT_VERSION;
use crate::album::{backup, locate};
use crate::error::AppResult;
use crate::{AppError, ErrorCode, text};

/// The choices `album_open` can carry after a question
/// (docs/04-browsing.md, "Opening a folder").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct OpenOptions {
    /// Open anyway: replace an old lock of another computer.
    pub take_lock: bool,
    /// Open view-only (`chosen`, or `damaged` for a damaged database).
    pub view_only: bool,
    /// Restore the newest backup of a damaged database.
    pub restore_backup: bool,
}

/// Why an album is view-only (`AlbumStatus.view_only`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ViewOnly {
    /// The folder or the database cannot be written.
    ReadOnly,
    /// Another computer has the album open.
    Locked,
    /// The user picked Open view-only.
    Chosen,
    /// The database was made by a newer Owlbum.
    Newer,
    /// The database failed its damage check.
    Damaged,
}

/// What album safety found, for `AlbumStatus`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Safety {
    /// Null when the album can be changed.
    pub view_only: Option<ViewOnly>,
    /// True when there is no database on disk and nothing is saved.
    pub in_memory: bool,
    /// The other computer, for `locked` and `chosen`.
    pub lock_host: Option<String>,
    /// A sync tool's copy of the database beside it, by file name.
    pub conflict_file: Option<String>,
}

/// The world an open runs in. [`Env::real`] is this process; tests pretend
/// to be other computers and other days.
#[derive(Clone)]
pub struct Env {
    /// Who is opening.
    pub owner: Owner,
    /// The local time now.
    pub clock: Arc<dyn Fn() -> DateTime<Local> + Send + Sync>,
    /// Whether a process of this computer is running.
    pub is_running: fn(u32) -> bool,
    /// How often the lock's heartbeat is rewritten.
    pub heartbeat_every: Duration,
    /// Behave as if the album folder could not be written (tests on
    /// Windows, where a folder cannot simply be made read-only).
    pub pretend_read_only: bool,
}

impl Env {
    /// This process, the real clock and the real 30-second heartbeat.
    pub fn real() -> Self {
        Self {
            owner: Owner::this_process(),
            clock: Arc::new(Local::now),
            is_running: lock::is_running,
            heartbeat_every: lock::HEARTBEAT_EVERY,
            pretend_read_only: false,
        }
    }
}

impl std::fmt::Debug for Env {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Env")
            .field("owner", &self.owner)
            .field("heartbeat_every", &self.heartbeat_every)
            .field("pretend_read_only", &self.pretend_read_only)
            .finish_non_exhaustive()
    }
}

/// An open album: where it is, what safety found, and its database.
#[derive(Debug)]
pub struct Album {
    location: AlbumLocation,
    safety: Safety,
    // Field order matters on drop: the database (writer and readers) closes
    // before the lock file is removed.
    db: AlbumDb,
    lock: Option<AlbumLock>,
    /// Raised on every change the page should hear of (`album_changed`).
    revision: AtomicU64,
}

impl Album {
    /// Opens the album for `folder` as this process.
    ///
    /// Errors: `not_a_folder` and `folder_unreadable` from finding the root,
    /// `album_lock_stale` (`details: { host, last_seen }`, `last_seen` in ms
    /// since the epoch) and `album_damaged` (`details: { backup_date }`,
    /// `YYYY-MM-DD` or null).
    pub fn open(folder: &Path, options: &OpenOptions) -> AppResult<Self> {
        Self::open_with(folder, options, &Env::real())
    }

    /// [`Album::open`] in a given [`Env`].
    pub fn open_with(folder: &Path, options: &OpenOptions, env: &Env) -> AppResult<Self> {
        let location = locate(folder)?;
        let owl = location.owlbum_dir();
        let db_path = location.database_path();
        let safety = Safety {
            conflict_file: check::conflict_file(&owl),
            ..Safety::default()
        };

        // A newer schema is read from the header, before the lock or a
        // journal is touched.
        let mut known_damaged = false;
        if location.exists {
            match check::read_header(&db_path) {
                Ok(Header::Version(v)) if v > CURRENT_VERSION => {
                    return Self::view_only(location, safety, ViewOnly::Newer, None);
                }
                Ok(Header::Version(_)) => {}
                Ok(Header::NotADatabase) => known_damaged = true,
                Err(e) if is_read_only(&e) => {
                    return Self::view_only(location, safety, ViewOnly::ReadOnly, None);
                }
                Err(e) => return Err(io_error(text::album::OPENING_DATABASE, &e)),
            }
        }

        if env.pretend_read_only || (location.exists && !file_writable(&db_path)) {
            return Self::view_only(location, safety, ViewOnly::ReadOnly, None);
        }

        let now_ms: lock::NowMs = {
            let clock = Arc::clone(&env.clock);
            Arc::new(move || clock().timestamp_millis())
        };
        let mut raced = false;
        let lock = loop {
            let existing = match read_lock_settled(&owl) {
                Ok(existing) => existing,
                Err(e) if is_read_only(&e) => {
                    return Self::view_only(location, safety, ViewOnly::ReadOnly, None);
                }
                Err(e) => return Err(io_error(text::album::LOCKING, &e)),
            };
            match lock::judge(existing.as_ref(), &env.owner, now_ms(), env.is_running) {
                Verdict::Live { host } => {
                    return Self::view_only(location, safety, ViewOnly::Locked, Some(host));
                }
                Verdict::Old { host, .. } if options.view_only => {
                    return Self::view_only(location, safety, ViewOnly::Chosen, Some(host));
                }
                Verdict::Old { host, last_seen } if !options.take_lock => {
                    return Err(stale_lock(&host, last_seen));
                }
                Verdict::Old { .. } | Verdict::Free => {}
            }
            if options.view_only {
                let damaged =
                    location.exists && (known_damaged || !check::quick_check(&db_path, false)?);
                let reason = if damaged {
                    ViewOnly::Damaged
                } else {
                    ViewOnly::Chosen
                };
                return Self::view_only(location, safety, reason, None);
            }

            match db::make_owlbum_dir(&location.root) {
                Ok(_) => {}
                Err(e) if is_read_only(&e) => {
                    return Self::view_only(location, safety, ViewOnly::ReadOnly, None);
                }
                Err(e) => return Err(io_error(text::album::CREATING_DATABASE, &e)),
            }
            // Replace whatever is there (a stale lock, or a file that is not
            // a lock); create only when there is nothing, so two computers
            // opening at once cannot both take it.
            let replace = owl.join(lock::LOCK_FILE).exists();
            match AlbumLock::take(
                &owl,
                &env.owner,
                replace,
                env.heartbeat_every,
                Arc::clone(&now_ms),
            ) {
                Ok(lock) => break lock,
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists && !raced => raced = true,
                Err(e) if is_read_only(&e) => {
                    return Self::view_only(location, safety, ViewOnly::ReadOnly, None);
                }
                Err(e) => return Err(io_error(text::album::LOCKING, &e)),
            }
        };
        // From here on an early return drops `lock`, which deletes the file.

        if location.exists {
            let sound = !known_damaged && check::quick_check(&db_path, true)?;
            if !sound {
                let restored =
                    options.restore_backup && backup::restore(&db_path, (env.clock)())?.is_some();
                if !restored {
                    return Err(damaged(&owl));
                }
            }
            // A failed backup does not stop the album opening: nothing is
            // lost, and tomorrow's open tries again. (Logged once #5 brings
            // the log.)
            let _ = backup::daily(&db_path, &owl, (env.clock)().date_naive());
        }

        let mut db = match AlbumDb::open(&location)? {
            Opened::Ready(db) => db,
            Opened::Newer { db, .. } => {
                drop(lock);
                return Ok(Self {
                    location,
                    safety: Safety {
                        view_only: Some(ViewOnly::Newer),
                        ..safety
                    },
                    db,
                    lock: None,
                    revision: AtomicU64::new(0),
                });
            }
        };
        db.refuse_writes_when(lock.lost_to());
        Ok(Self {
            location,
            safety,
            db,
            lock: Some(lock),
            revision: AtomicU64::new(0),
        })
    }

    /// A view-only album: the database read-only, or in memory when there is
    /// none on disk. Nothing is written in the album folder.
    fn view_only(
        location: AlbumLocation,
        safety: Safety,
        reason: ViewOnly,
        host: Option<String>,
    ) -> AppResult<Self> {
        use text::album::view_only as says;
        let in_memory = !location.exists;
        let message = match reason {
            ViewOnly::ReadOnly if in_memory => says::IN_MEMORY.to_owned(),
            ViewOnly::ReadOnly => says::READ_ONLY.to_owned(),
            ViewOnly::Locked => says::locked(host.as_deref().unwrap_or_default()),
            ViewOnly::Chosen => says::chosen(host.as_deref()),
            ViewOnly::Newer => says::NEWER.to_owned(),
            ViewOnly::Damaged => says::DAMAGED.to_owned(),
        };
        let db = if in_memory {
            AlbumDb::in_memory(&message)?
        } else {
            AlbumDb::view_only(&location.database_path(), &message)?
        };
        Ok(Self {
            location,
            safety: Safety {
                view_only: Some(reason),
                in_memory,
                lock_host: host,
                ..safety
            },
            db,
            lock: None,
            revision: AtomicU64::new(0),
        })
    }

    /// Where the album is: root and scope.
    pub fn location(&self) -> &AlbumLocation {
        &self.location
    }

    /// What album safety found. Once another computer has replaced the lock
    /// (Open anyway there), the album reports `locked` from then on.
    pub fn safety(&self) -> Safety {
        let lost_to = self.lock.as_ref().and_then(|lock| {
            lock.lost_to()
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone()
        });
        match lost_to {
            Some(host) => Safety {
                view_only: Some(ViewOnly::Locked),
                lock_host: Some(host),
                ..self.safety.clone()
            },
            None => self.safety.clone(),
        }
    }

    /// True when the album cannot be changed.
    pub fn is_view_only(&self) -> bool {
        self.safety().view_only.is_some()
    }

    /// The album database.
    pub fn db(&self) -> &AlbumDb {
        &self.db
    }

    /// The album's revision: 0 when opened, raised by every change the
    /// page is told of with `album_changed`.
    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::SeqCst)
    }

    /// Raises the revision and answers the new one.
    pub fn next_revision(&self) -> u64 {
        self.revision.fetch_add(1, Ordering::SeqCst) + 1
    }
}

/// How long a lock file that is not (yet) a lock is given to be finished
/// by the Owlbum that is creating it, before it is treated as junk.
const UNFINISHED_LOCK_WAIT: Duration = Duration::from_millis(250);

/// Reads the lock. A lock file that exists but does not read as a lock may
/// be one another computer has just created and is still writing (a new lock
/// is created empty with `create_new`, then filled), so it is read once more
/// after [`UNFINISHED_LOCK_WAIT`].
fn read_lock_settled(owl: &Path) -> io::Result<Option<lock::LockInfo>> {
    let first = lock::read(owl)?;
    if first.is_some() || !owl.join(lock::LOCK_FILE).exists() {
        return Ok(first);
    }
    std::thread::sleep(UNFINISHED_LOCK_WAIT);
    lock::read(owl)
}

/// True for the I/O errors that mean "this cannot be written here".
fn is_read_only(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem
    )
}

/// False when the file cannot be opened for writing (a read-only file, or
/// one the user may not change). Nothing is written.
fn file_writable(path: &Path) -> bool {
    match std::fs::OpenOptions::new().write(true).open(path) {
        Ok(_) => true,
        Err(e) => !is_read_only(&e),
    }
}

fn io_error(what: &'static str, e: &io::Error) -> AppError {
    AppError::internal(what).with_details(serde_json::json!({ "cause": e.to_string() }))
}

/// `album_lock_stale`, with `details: { host, last_seen }`.
fn stale_lock(host: &str, last_seen: i64) -> AppError {
    let shown = Local
        .timestamp_millis_opt(last_seen)
        .single()
        .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default();
    AppError::new(
        ErrorCode::AlbumLockStale,
        text::album::lock_stale(host, &shown),
    )
    .with_details(serde_json::json!({ "host": host, "last_seen": last_seen }))
}

/// `album_damaged`, with `details: { backup_date }` naming the newest sound
/// backup, or null.
fn damaged(owlbum_dir: &Path) -> AppError {
    match backup::newest_sound(owlbum_dir) {
        Some((date, _)) => {
            let date = date.format("%Y-%m-%d").to_string();
            AppError::new(
                ErrorCode::AlbumDamaged,
                text::album::damaged_with_backup(&date),
            )
            .with_details(serde_json::json!({ "backup_date": date }))
        }
        None => AppError::new(ErrorCode::AlbumDamaged, text::album::DAMAGED_NO_BACKUP)
            .with_details(serde_json::json!({ "backup_date": null })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::album::lock::{LIVE_FOR_MS, LOCK_FILE, LockInfo};
    use crate::album::schema::CURRENT_VERSION;
    use crate::album::{backup, sqlite_error};
    use crate::{ErrorCode, text};
    use chrono::TimeZone;
    use rusqlite::Connection;
    use std::path::PathBuf;
    use std::sync::Mutex;

    /// An env for `host` whose clock reads `day` (local noon) until moved.
    fn env_on(host: &str, day: &str) -> (Env, Arc<Mutex<DateTime<Local>>>) {
        let date = chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").unwrap();
        let noon = Local
            .from_local_datetime(&date.and_hms_opt(12, 0, 0).unwrap())
            .unwrap();
        let now = Arc::new(Mutex::new(noon));
        let clock = {
            let now = Arc::clone(&now);
            Arc::new(move || *now.lock().unwrap())
        };
        (
            Env {
                owner: Owner {
                    host: host.into(),
                    pid: std::process::id(),
                    user: "me".into(),
                },
                clock,
                is_running: |_| false,
                heartbeat_every: lock::HEARTBEAT_EVERY,
                pretend_read_only: false,
            },
            now,
        )
    }

    fn env() -> Env {
        env_on("HERE", "2026-11-25").0
    }

    fn none() -> OpenOptions {
        OpenOptions::default()
    }

    fn owl(root: &Path) -> PathBuf {
        root.join(".owlbum")
    }

    /// Every file under `dir` with its bytes, for "nothing changed".
    fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        fn walk(dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
            for e in std::fs::read_dir(dir).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    out.push((p.clone(), Vec::new()));
                    walk(&p, out);
                } else {
                    out.push((p.clone(), std::fs::read(&p).unwrap()));
                }
            }
        }
        let mut out = Vec::new();
        walk(dir, &mut out);
        out.sort();
        out
    }

    fn insert_photo(album: &Album, name: &str) -> AppResult<i64> {
        let name = name.to_owned();
        album.db().write(move |tx| {
            tx.execute(
                "INSERT INTO photos (rel_path, rel_dir, file_name, size, mtime_ms, file_key, thumb_key, kind, format, width, height, scan_version)
                 VALUES (?1, '', ?1, 1, 1, 'fk', 'tk', 'image', 'jpeg', 1, 1, 1)",
                [&name],
            )
            .map_err(sqlite_error(text::album::WRITING_DATABASE))?;
            Ok(tx.last_insert_rowid())
        })
    }

    fn photos(album: &Album) -> i64 {
        album
            .db()
            .read(|c| {
                c.query_row("SELECT count(*) FROM photos", [], |r| r.get(0))
                    .map_err(sqlite_error(text::album::READING_DATABASE))
            })
            .unwrap()
    }

    /// An album with one photo, closed again.
    fn existing_album(root: &Path) {
        let album = Album::open_with(root, &none(), &env()).unwrap();
        insert_photo(&album, "a.jpg").unwrap();
    }

    fn write_lock(root: &Path, host: &str, pid: u32, heartbeat: i64) -> LockInfo {
        let info = LockInfo {
            host: host.into(),
            pid,
            user: "them".into(),
            app_version: "0.1.0".into(),
            opened_at: heartbeat - 1000,
            heartbeat,
        };
        std::fs::create_dir_all(owl(root)).unwrap();
        std::fs::write(
            owl(root).join(LOCK_FILE),
            serde_json::to_vec(&info).unwrap(),
        )
        .unwrap();
        info
    }

    fn now_ms(env: &Env) -> i64 {
        (env.clock)().timestamp_millis()
    }

    fn damage(root: &Path) {
        let db = owl(root).join("owlbum.sqlite");
        let mut bytes = std::fs::read(&db).unwrap();
        let len = bytes.len();
        for b in &mut bytes[len / 2..] {
            *b = 0x5a;
        }
        // Keep the header (and with it the version) so only quick_check
        // can tell.
        std::fs::write(&db, bytes).unwrap();
    }

    fn assert_view_only_refuses_writes(album: &Album, message: &str) {
        let err = insert_photo(album, "new.jpg").unwrap_err();
        assert_eq!(err.code, ErrorCode::ViewOnly);
        assert_eq!(err.message, message);
    }

    // No lock file: take the lock; open for writing.

    #[test]
    fn a_new_folder_opens_for_writing_and_holds_the_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let album = Album::open_with(tmp.path(), &none(), &env()).unwrap();
        assert_eq!(album.safety(), Safety::default());
        assert!(!album.is_view_only());
        insert_photo(&album, "a.jpg").unwrap();
        let held = lock::read(&owl(tmp.path())).unwrap().unwrap();
        assert_eq!(held.host, "HERE");
        assert_eq!(held.pid, std::process::id());
        drop(album);
        assert!(
            !owl(tmp.path()).join(LOCK_FILE).exists(),
            "deleted on close"
        );
    }

    #[test]
    fn a_new_album_gets_no_backup_on_the_day_it_is_made() {
        let tmp = tempfile::tempdir().unwrap();
        drop(Album::open_with(tmp.path(), &none(), &env()).unwrap());
        assert!(!owl(tmp.path()).join("backup").exists());
    }

    #[test]
    fn a_scoped_open_reports_root_and_scope() {
        let tmp = tempfile::tempdir().unwrap();
        existing_album(tmp.path());
        let sub = tmp.path().join("2024").join("Trip");
        std::fs::create_dir_all(&sub).unwrap();
        let album = Album::open_with(&sub, &none(), &env()).unwrap();
        assert_eq!(album.location().root, tmp.path());
        assert_eq!(album.location().scope, "2024/Trip");
        assert!(!album.is_view_only());
    }

    // Backups.

    #[test]
    fn the_first_writable_open_of_a_day_backs_up_and_later_ones_do_not() {
        let tmp = tempfile::tempdir().unwrap();
        existing_album(tmp.path());
        let (env, now) = env_on("HERE", "2026-11-26");
        drop(Album::open_with(tmp.path(), &none(), &env).unwrap());
        let copies = backup::list(&owl(tmp.path()));
        assert_eq!(copies.len(), 1);
        assert_eq!(copies[0].0.to_string(), "2026-11-26");
        let first = std::fs::read(&copies[0].1).unwrap();

        let album = Album::open_with(tmp.path(), &none(), &env).unwrap();
        insert_photo(&album, "b.jpg").unwrap();
        drop(album);
        drop(Album::open_with(tmp.path(), &none(), &env).unwrap());
        assert_eq!(std::fs::read(&copies[0].1).unwrap(), first, "same day");

        *now.lock().unwrap() += chrono::Duration::days(1);
        drop(Album::open_with(tmp.path(), &none(), &env).unwrap());
        assert_eq!(backup::list(&owl(tmp.path())).len(), 2);
    }

    // Lock from this host whose process is no longer running: stale,
    // replace it; open for writing.

    #[test]
    fn a_lock_left_by_a_finished_process_here_is_replaced() {
        let tmp = tempfile::tempdir().unwrap();
        existing_album(tmp.path());
        let env = env();
        write_lock(tmp.path(), "HERE", 4242, now_ms(&env) - 1000);
        let album = Album::open_with(tmp.path(), &none(), &env).unwrap();
        assert!(!album.is_view_only());
        assert_eq!(
            lock::read(&owl(tmp.path())).unwrap().unwrap().pid,
            std::process::id()
        );
    }

    // Lock from another host with a heartbeat under 2 minutes old: open
    // view-only, naming the other computer.

    #[test]
    fn a_live_lock_of_another_computer_opens_view_only() {
        let tmp = tempfile::tempdir().unwrap();
        existing_album(tmp.path());
        let env = env();
        write_lock(tmp.path(), "THERE", 7, now_ms(&env) - LIVE_FOR_MS + 5000);
        let before = snapshot(tmp.path());

        let album = Album::open_with(tmp.path(), &none(), &env).unwrap();
        assert_eq!(album.safety().view_only, Some(ViewOnly::Locked));
        assert_eq!(album.safety().lock_host.as_deref(), Some("THERE"));
        assert_eq!(photos(&album), 1, "reads work");
        assert_view_only_refuses_writes(&album, &text::album::view_only::locked("THERE"));
        drop(album);
        assert_eq!(snapshot(tmp.path()), before, "nothing written");
    }

    #[test]
    fn a_lock_being_written_by_another_computer_is_waited_for() {
        let tmp = tempfile::tempdir().unwrap();
        existing_album(tmp.path());
        let env = env();
        let lock_path = owl(tmp.path()).join(LOCK_FILE);
        std::fs::write(&lock_path, b"").unwrap();
        let filler = {
            let root = tmp.path().to_path_buf();
            let seen = now_ms(&env);
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(50));
                write_lock(&root, "THERE", 7, seen);
            })
        };
        let album = Album::open_with(tmp.path(), &none(), &env).unwrap();
        filler.join().unwrap();
        assert_eq!(album.safety().view_only, Some(ViewOnly::Locked));
        assert_eq!(lock::read(&owl(tmp.path())).unwrap().unwrap().host, "THERE");
    }

    #[test]
    fn a_lock_file_that_stays_junk_is_replaced() {
        let tmp = tempfile::tempdir().unwrap();
        existing_album(tmp.path());
        std::fs::write(owl(tmp.path()).join(LOCK_FILE), b"{\"host\":").unwrap();
        let album = Album::open_with(tmp.path(), &none(), &env()).unwrap();
        assert!(!album.is_view_only());
        assert_eq!(lock::read(&owl(tmp.path())).unwrap().unwrap().host, "HERE");
    }

    #[test]
    fn a_second_owlbum_sees_the_first_ones_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let first = Album::open_with(tmp.path(), &none(), &env()).unwrap();
        let (other, _) = env_on("THERE", "2026-11-25");
        let second = Album::open_with(tmp.path(), &none(), &other).unwrap();
        assert_eq!(second.safety().view_only, Some(ViewOnly::Locked));
        assert_eq!(second.safety().lock_host.as_deref(), Some("HERE"));
        drop(second);
        drop(first);
    }

    // Lock from another host with an older heartbeat: ask.

    #[test]
    fn an_old_lock_of_another_computer_is_asked_about() {
        let tmp = tempfile::tempdir().unwrap();
        existing_album(tmp.path());
        let env = env();
        let seen = now_ms(&env) - LIVE_FOR_MS - 60_000;
        write_lock(tmp.path(), "THERE", 7, seen);
        let before = snapshot(tmp.path());

        let err = Album::open_with(tmp.path(), &none(), &env).unwrap_err();
        assert_eq!(err.code, ErrorCode::AlbumLockStale);
        assert_eq!(
            err.details,
            Some(serde_json::json!({ "host": "THERE", "last_seen": seen }))
        );
        assert!(
            err.message
                .starts_with("This album looks open on THERE, last seen ")
        );
        assert_eq!(snapshot(tmp.path()), before, "nothing written");
    }

    #[test]
    fn open_anyway_replaces_an_old_lock() {
        let tmp = tempfile::tempdir().unwrap();
        existing_album(tmp.path());
        let env = env();
        write_lock(tmp.path(), "THERE", 7, now_ms(&env) - LIVE_FOR_MS - 60_000);
        let opts = OpenOptions {
            take_lock: true,
            ..none()
        };
        let album = Album::open_with(tmp.path(), &opts, &env).unwrap();
        assert!(!album.is_view_only());
        assert_eq!(lock::read(&owl(tmp.path())).unwrap().unwrap().host, "HERE");
        insert_photo(&album, "b.jpg").unwrap();
    }

    #[test]
    fn open_anyway_does_not_replace_a_live_lock() {
        let tmp = tempfile::tempdir().unwrap();
        existing_album(tmp.path());
        let env = env();
        write_lock(tmp.path(), "THERE", 7, now_ms(&env));
        let opts = OpenOptions {
            take_lock: true,
            ..none()
        };
        let album = Album::open_with(tmp.path(), &opts, &env).unwrap();
        assert_eq!(album.safety().view_only, Some(ViewOnly::Locked));
        assert_eq!(lock::read(&owl(tmp.path())).unwrap().unwrap().host, "THERE");
    }

    #[test]
    fn open_view_only_at_an_old_lock_is_chosen() {
        let tmp = tempfile::tempdir().unwrap();
        existing_album(tmp.path());
        let env = env();
        write_lock(tmp.path(), "THERE", 7, now_ms(&env) - LIVE_FOR_MS - 60_000);
        let before = snapshot(tmp.path());
        let opts = OpenOptions {
            view_only: true,
            ..none()
        };
        let album = Album::open_with(tmp.path(), &opts, &env).unwrap();
        assert_eq!(album.safety().view_only, Some(ViewOnly::Chosen));
        assert_eq!(album.safety().lock_host.as_deref(), Some("THERE"));
        assert_view_only_refuses_writes(&album, &text::album::view_only::chosen(Some("THERE")));
        drop(album);
        assert_eq!(snapshot(tmp.path()), before, "nothing written");
    }

    #[test]
    fn a_lock_taken_over_elsewhere_turns_the_album_view_only() {
        let tmp = tempfile::tempdir().unwrap();
        let mut env = env();
        env.heartbeat_every = Duration::from_millis(20);
        let album = Album::open_with(tmp.path(), &none(), &env).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !album.is_view_only() {
            assert!(std::time::Instant::now() < deadline, "loss not noticed");
            // The other computer beats too. A beat of ours can land on one
            // of theirs; a later one of theirs is then seen.
            write_lock(tmp.path(), "THERE", 7, now_ms(&env));
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(album.safety().view_only, Some(ViewOnly::Locked));
        assert_eq!(album.safety().lock_host.as_deref(), Some("THERE"));
        assert_view_only_refuses_writes(&album, &text::album::view_only::locked("THERE"));
        drop(album);
        assert_eq!(lock::read(&owl(tmp.path())).unwrap().unwrap().host, "THERE");
    }

    // The folder or the database cannot be written: view-only.

    #[test]
    fn a_read_only_database_file_opens_view_only() {
        let tmp = tempfile::tempdir().unwrap();
        existing_album(tmp.path());
        let db = owl(tmp.path()).join("owlbum.sqlite");
        let mut perms = std::fs::metadata(&db).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&db, perms.clone()).unwrap();
        if std::fs::OpenOptions::new().write(true).open(&db).is_ok() {
            eprintln!("skipped: running with rights that ignore read-only files");
        } else {
            let before = snapshot(tmp.path());
            let album = Album::open_with(tmp.path(), &none(), &env()).unwrap();
            assert_eq!(album.safety().view_only, Some(ViewOnly::ReadOnly));
            assert!(!album.safety().in_memory);
            assert_eq!(photos(&album), 1);
            assert_view_only_refuses_writes(&album, text::album::view_only::READ_ONLY);
            drop(album);
            assert_eq!(snapshot(tmp.path()), before, "nothing written");
        }
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        std::fs::set_permissions(&db, perms).unwrap();
    }

    #[test]
    fn a_folder_that_cannot_be_written_with_an_album_opens_view_only() {
        let tmp = tempfile::tempdir().unwrap();
        existing_album(tmp.path());
        let before = snapshot(tmp.path());
        let mut env = env();
        env.pretend_read_only = true;
        let album = Album::open_with(tmp.path(), &none(), &env).unwrap();
        assert_eq!(album.safety().view_only, Some(ViewOnly::ReadOnly));
        assert!(!album.safety().in_memory);
        assert_eq!(photos(&album), 1);
        drop(album);
        assert_eq!(snapshot(tmp.path()), before);
    }

    // No database and the folder cannot be written: view-only with an
    // in-memory database.

    #[test]
    fn a_folder_that_cannot_be_written_without_an_album_opens_in_memory() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.jpg"), b"x").unwrap();
        let mut env = env();
        env.pretend_read_only = true;
        let album = Album::open_with(tmp.path(), &none(), &env).unwrap();
        assert_eq!(album.safety().view_only, Some(ViewOnly::ReadOnly));
        assert!(album.safety().in_memory);
        assert_eq!(photos(&album), 0, "the schema is there");
        assert_view_only_refuses_writes(&album, text::album::view_only::IN_MEMORY);
        drop(album);
        assert_eq!(
            std::fs::read_dir(tmp.path()).unwrap().count(),
            1,
            "nothing created"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_read_only_folder_without_an_album_opens_in_memory() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("photos");
        std::fs::create_dir(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        if std::fs::create_dir(dir.join("probe")).is_ok() {
            eprintln!("skipped: running as root");
        } else {
            let album = Album::open_with(&dir, &none(), &env()).unwrap();
            assert_eq!(album.safety().view_only, Some(ViewOnly::ReadOnly));
            assert!(album.safety().in_memory);
        }
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    // A newer database: view-only.

    #[test]
    fn a_newer_database_opens_view_only_and_takes_no_lock() {
        let tmp = tempfile::tempdir().unwrap();
        existing_album(tmp.path());
        let db = owl(tmp.path()).join("owlbum.sqlite");
        Connection::open(&db)
            .unwrap()
            .pragma_update(None, "user_version", CURRENT_VERSION + 1)
            .unwrap();
        let before = snapshot(tmp.path());
        let album = Album::open_with(tmp.path(), &none(), &env()).unwrap();
        assert_eq!(album.safety().view_only, Some(ViewOnly::Newer));
        assert_eq!(photos(&album), 1);
        assert_view_only_refuses_writes(&album, text::album::view_only::NEWER);
        drop(album);
        assert_eq!(snapshot(tmp.path()), before, "nothing written");
    }

    // Damage.

    #[test]
    fn a_damaged_database_is_reported_with_the_backup_date() {
        let tmp = tempfile::tempdir().unwrap();
        existing_album(tmp.path());
        let (next_day, _) = env_on("HERE", "2026-11-26");
        drop(Album::open_with(tmp.path(), &none(), &next_day).unwrap());
        damage(tmp.path());
        let before = snapshot(tmp.path());

        let (later, _) = env_on("HERE", "2026-11-27");
        let err = Album::open_with(tmp.path(), &none(), &later).unwrap_err();
        assert_eq!(err.code, ErrorCode::AlbumDamaged);
        assert_eq!(
            err.details,
            Some(serde_json::json!({ "backup_date": "2026-11-26" }))
        );
        assert_eq!(err.message, text::album::damaged_with_backup("2026-11-26"));
        assert_eq!(snapshot(tmp.path()), before, "no lock left, no backup made");
    }

    #[test]
    fn a_damaged_database_without_a_backup_says_so() {
        let tmp = tempfile::tempdir().unwrap();
        existing_album(tmp.path());
        damage(tmp.path());
        let err = Album::open_with(tmp.path(), &none(), &env()).unwrap_err();
        assert_eq!(err.code, ErrorCode::AlbumDamaged);
        assert_eq!(
            err.details,
            Some(serde_json::json!({ "backup_date": null }))
        );
        assert_eq!(err.message, text::album::DAMAGED_NO_BACKUP);
    }

    #[test]
    fn restore_backup_keeps_the_damaged_file_and_opens_for_writing() {
        let tmp = tempfile::tempdir().unwrap();
        existing_album(tmp.path());
        let (next_day, _) = env_on("HERE", "2026-11-26");
        drop(Album::open_with(tmp.path(), &none(), &next_day).unwrap());
        damage(tmp.path());
        let damaged = std::fs::read(owl(tmp.path()).join("owlbum.sqlite")).unwrap();

        let opts = OpenOptions {
            restore_backup: true,
            ..none()
        };
        let album = Album::open_with(tmp.path(), &opts, &next_day).unwrap();
        assert_eq!(album.safety(), Safety::default());
        assert_eq!(photos(&album), 1);
        insert_photo(&album, "b.jpg").unwrap();
        let kept: Vec<_> = std::fs::read_dir(owl(tmp.path()))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("owlbum.damaged-")
            })
            .collect();
        assert_eq!(kept.len(), 1);
        assert_eq!(std::fs::read(&kept[0]).unwrap(), damaged);
        assert_eq!(album.safety().conflict_file, None);
    }

    #[test]
    fn open_view_only_at_a_damaged_database() {
        let tmp = tempfile::tempdir().unwrap();
        existing_album(tmp.path());
        damage(tmp.path());
        let before = snapshot(tmp.path());
        let opts = OpenOptions {
            view_only: true,
            ..none()
        };
        let album = Album::open_with(tmp.path(), &opts, &env()).unwrap();
        assert_eq!(album.safety().view_only, Some(ViewOnly::Damaged));
        assert_view_only_refuses_writes(&album, text::album::view_only::DAMAGED);
        drop(album);
        assert_eq!(snapshot(tmp.path()), before, "nothing written");
    }

    #[test]
    fn a_file_that_is_not_a_database_is_damaged() {
        let tmp = tempfile::tempdir().unwrap();
        existing_album(tmp.path());
        std::fs::write(owl(tmp.path()).join("owlbum.sqlite"), vec![7u8; 8192]).unwrap();
        let err = Album::open_with(tmp.path(), &none(), &env()).unwrap_err();
        assert_eq!(err.code, ErrorCode::AlbumDamaged);
    }

    #[test]
    fn open_view_only_on_a_sound_album_is_chosen() {
        let tmp = tempfile::tempdir().unwrap();
        existing_album(tmp.path());
        let opts = OpenOptions {
            view_only: true,
            ..none()
        };
        let album = Album::open_with(tmp.path(), &opts, &env()).unwrap();
        assert_eq!(album.safety().view_only, Some(ViewOnly::Chosen));
        assert_eq!(album.safety().lock_host, None);
        assert!(!owl(tmp.path()).join(LOCK_FILE).exists());
    }

    // Sync conflict copies.

    #[test]
    fn a_conflict_copy_beside_the_database_is_named() {
        let tmp = tempfile::tempdir().unwrap();
        existing_album(tmp.path());
        std::fs::write(owl(tmp.path()).join("owlbum (conflicted copy).sqlite"), b"").unwrap();
        let album = Album::open_with(tmp.path(), &none(), &env()).unwrap();
        assert!(!album.is_view_only(), "only a warning");
        assert_eq!(
            album.safety().conflict_file.as_deref(),
            Some("owlbum (conflicted copy).sqlite")
        );
    }

    #[test]
    fn a_conflict_copy_is_named_in_a_view_only_album_too() {
        let tmp = tempfile::tempdir().unwrap();
        existing_album(tmp.path());
        std::fs::write(owl(tmp.path()).join("owlbum-PC-NAME.sqlite"), b"").unwrap();
        let env = env();
        write_lock(tmp.path(), "THERE", 7, now_ms(&env));
        let album = Album::open_with(tmp.path(), &none(), &env).unwrap();
        assert_eq!(album.safety().view_only, Some(ViewOnly::Locked));
        assert_eq!(
            album.safety().conflict_file.as_deref(),
            Some("owlbum-PC-NAME.sqlite")
        );
    }

    // The option types are what the page sends and reads.

    #[test]
    fn options_and_reasons_have_their_documented_names() {
        let opts: OpenOptions = serde_json::from_str(r#"{ "take_lock": true }"#).unwrap();
        assert_eq!(
            opts,
            OpenOptions {
                take_lock: true,
                ..none()
            }
        );
        let names: Vec<serde_json::Value> = [
            ViewOnly::ReadOnly,
            ViewOnly::Locked,
            ViewOnly::Chosen,
            ViewOnly::Newer,
            ViewOnly::Damaged,
        ]
        .iter()
        .map(|r| serde_json::to_value(r).unwrap())
        .collect();
        assert_eq!(names, ["read_only", "locked", "chosen", "newer", "damaged"]);
        let codes: Vec<serde_json::Value> = [ErrorCode::AlbumDamaged, ErrorCode::AlbumLockStale]
            .iter()
            .map(|c| serde_json::to_value(c).unwrap())
            .collect();
        assert_eq!(codes, ["album_damaged", "album_lock_stale"]);
    }
}
