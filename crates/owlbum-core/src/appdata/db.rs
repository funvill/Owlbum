//! Opening `app.sqlite`: its schema, its migrations, and what happens when it
//! is damaged (docs/02-data.md, "The app data folder" and "`app.sqlite`").
//!
//! The file follows the album database's patterns: `PRAGMA user_version`
//! holds the schema version, migrations are numbered SQL scripts compiled
//! into the program and run in one transaction, and the connection uses the
//! rollback journal (`journal_mode = DELETE`, `synchronous = FULL`).
//!
//! A damaged file (not a database, or failing `PRAGMA quick_check`) is
//! renamed `app.damaged-<stamp>.sqlite`, never deleted, and a new one is
//! made; only preferences and the recent list are lost. Any other failure
//! (the folder cannot be written, the file is busy) is an error and the file
//! is left alone.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, TransactionBehavior};

use crate::AppError;
use crate::appdata::clock::Utc;
use crate::error::AppResult;
use crate::text;

/// The file's name inside the app data folder.
pub const FILE_NAME: &str = "app.sqlite";

/// How long a connection waits for another copy of Owlbum (or a test) to
/// finish writing.
const BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// One numbered migration script.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    /// The schema version the script brings the file to, from 1 up by one.
    pub version: u32,
    /// The SQL, run as one batch.
    pub sql: &'static str,
}

/// Every migration, in order. Add the next script at the end; never edit one
/// that has shipped. Later versions only add: an older Owlbum must still be
/// able to use a file a newer one has changed.
pub const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    sql: include_str!("migrations/0001_initial.sql"),
}];

/// The newest schema version this program knows.
pub const CURRENT_VERSION: u32 = match MIGRATIONS.last() {
    Some(m) => m.version,
    None => 0,
};

/// An open `app.sqlite`, and what opening it found.
#[derive(Debug)]
pub struct OpenedDb {
    /// The connection, configured and migrated.
    pub conn: Connection,
    /// Where a damaged file was moved to, when one was set aside on this
    /// open. The shell tells the user once.
    pub set_aside: Option<PathBuf>,
    /// The schema version the file had before this open (0 for a new file).
    /// Above [`CURRENT_VERSION`] when a newer Owlbum made it; such a file is
    /// used as it is, without migrating.
    pub found_version: u32,
}

/// Why a try at opening failed.
enum Failure {
    /// The file is damaged: set it aside and start again.
    Damaged,
    /// Anything else. The file is left alone.
    Other(AppError),
}

/// Opens `<dir>/app.sqlite`, creating it when missing and setting it aside
/// when it is damaged. `now_ms` names a set-aside file.
pub fn open(dir: &Path, now_ms: i64) -> AppResult<OpenedDb> {
    open_with(dir, now_ms, MIGRATIONS)
}

/// [`open`] with a given list of migrations, for tests.
pub fn open_with(dir: &Path, now_ms: i64, migrations: &[Migration]) -> AppResult<OpenedDb> {
    let path = dir.join(FILE_NAME);
    match try_open(&path, migrations) {
        Ok((conn, found_version)) => Ok(OpenedDb {
            conn,
            set_aside: None,
            found_version,
        }),
        Err(Failure::Other(e)) => Err(e),
        Err(Failure::Damaged) => {
            // The failed connection is closed by now (`try_open` owned it),
            // so Windows lets the file be renamed.
            let moved = set_aside(&path, now_ms)?;
            match try_open(&path, migrations) {
                Ok((conn, found_version)) => Ok(OpenedDb {
                    conn,
                    set_aside: Some(moved),
                    found_version,
                }),
                Err(Failure::Other(e)) => Err(e),
                Err(Failure::Damaged) => Err(AppError::internal(text::appdata::OPENING_SETTINGS)),
            }
        }
    }
}

fn try_open(path: &Path, migrations: &[Migration]) -> Result<(Connection, u32), Failure> {
    let mut conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(classify(text::appdata::OPENING_SETTINGS))?;
    configure(&conn).map_err(classify(text::appdata::OPENING_SETTINGS))?;
    let check: String = conn
        .query_row("PRAGMA quick_check", [], |r| r.get(0))
        .map_err(classify(text::appdata::OPENING_SETTINGS))?;
    if check != "ok" {
        return Err(Failure::Damaged);
    }
    let found = migrate(&mut conn, migrations)?;
    Ok((conn, found))
}

/// The settings every connection to `app.sqlite` gets.
fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.busy_timeout(BUSY_TIMEOUT)?;
    let _mode: String =
        conn.pragma_update_and_check(None, "journal_mode", "DELETE", |r| r.get(0))?;
    conn.pragma_update(None, "synchronous", "FULL")?;
    Ok(())
}

/// Runs every migration newer than the file's version in one transaction.
/// Answers the version the file had before.
fn migrate(conn: &mut Connection, migrations: &[Migration]) -> Result<u32, Failure> {
    let known = migrations.last().map_or(0, |m| m.version);
    let read_version = |c: &Connection| -> Result<u32, Failure> {
        c.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .map(|v| u32::try_from(v).unwrap_or(u32::MAX))
            .map_err(classify(text::appdata::OPENING_SETTINGS))
    };
    let from = read_version(conn)?;
    if from >= known {
        return Ok(from);
    }
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(classify(text::appdata::OPENING_SETTINGS))?;
    // Another copy may have migrated between the read above and the write
    // lock; read again inside the transaction.
    let from = read_version(&tx)?;
    if from >= known {
        return Ok(from);
    }
    for m in migrations.iter().filter(|m| m.version > from) {
        tx.execute_batch(m.sql)
            .map_err(classify(text::appdata::OPENING_SETTINGS))?;
    }
    tx.pragma_update(None, "user_version", known)
        .map_err(classify(text::appdata::OPENING_SETTINGS))?;
    tx.commit()
        .map_err(classify(text::appdata::OPENING_SETTINGS))?;
    Ok(from)
}

/// Sorts a SQLite error into "damaged" and everything else.
fn classify(what: &'static str) -> impl Fn(rusqlite::Error) -> Failure {
    move |e| {
        if is_damage(&e) {
            Failure::Damaged
        } else {
            Failure::Other(sqlite_error(what)(e))
        }
    }
}

/// True for the errors that mean the file itself is bad.
fn is_damage(e: &rusqlite::Error) -> bool {
    matches!(
        e,
        rusqlite::Error::SqliteFailure(f, _)
            if matches!(
                f.code,
                rusqlite::ErrorCode::NotADatabase | rusqlite::ErrorCode::DatabaseCorrupt
            )
    )
}

/// Maps a SQLite error to an `internal` error that says what was being done;
/// SQLite's own text goes in `details.cause` for the log.
pub(crate) fn sqlite_error(what: &'static str) -> impl Fn(rusqlite::Error) -> AppError {
    move |e| AppError::internal(what).with_details(serde_json::json!({ "cause": e.to_string() }))
}

/// Renames a damaged `app.sqlite` (and its journal, if any) to
/// `app.damaged-<stamp>.sqlite`, adding `-2`, `-3`, … if that name is taken.
/// Answers the new path.
fn set_aside(path: &Path, now_ms: i64) -> AppResult<PathBuf> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let stamp = Utc::from_ms(now_ms).stamp();
    let target = (1..)
        .map(|n| {
            let name = if n == 1 {
                format!("app.damaged-{stamp}.sqlite")
            } else {
                format!("app.damaged-{stamp}-{n}.sqlite")
            };
            dir.join(name)
        })
        .find(|p| !p.exists())
        .unwrap_or_else(|| dir.join(format!("app.damaged-{stamp}.sqlite")));
    let failed = |e: std::io::Error| {
        AppError::internal(text::appdata::RESETTING_SETTINGS)
            .with_details(serde_json::json!({ "cause": e.to_string() }))
    };
    std::fs::rename(path, &target).map_err(failed)?;
    let journal = journal_of(path);
    if journal.exists() {
        std::fs::rename(&journal, journal_of(&target)).map_err(failed)?;
    }
    Ok(target)
}

fn journal_of(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push("-journal");
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_795_683_903_007; // 2026-11-26T09:05:03.007Z

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    fn tables(conn: &Connection) -> Vec<String> {
        let mut s = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .unwrap();
        s.query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    #[test]
    fn migrations_are_numbered_from_one_without_gaps() {
        for (i, m) in MIGRATIONS.iter().enumerate() {
            assert_eq!(m.version as usize, i + 1);
        }
        assert_eq!(CURRENT_VERSION, 1);
    }

    #[test]
    fn a_new_file_gets_the_whole_schema() {
        let tmp = tempfile::tempdir().unwrap();
        let db = open(tmp.path(), NOW).unwrap();
        assert_eq!(db.found_version, 0);
        assert!(db.set_aside.is_none());
        assert_eq!(tables(&db.conn), ["cache_entries", "recent", "settings"]);
        let v: i64 = db
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, i64::from(CURRENT_VERSION));
        drop(db);
        assert_eq!(names(tmp.path()), [FILE_NAME]);
    }

    #[test]
    fn the_connection_uses_the_rollback_journal_and_full_sync() {
        let tmp = tempfile::tempdir().unwrap();
        let db = open(tmp.path(), NOW).unwrap();
        let mode: String = db
            .conn
            .pragma_query_value(None, "journal_mode", |r| r.get(0))
            .unwrap();
        let sync: i64 = db
            .conn
            .pragma_query_value(None, "synchronous", |r| r.get(0))
            .unwrap();
        assert_eq!(mode.to_lowercase(), "delete");
        assert_eq!(sync, 2, "FULL");
    }

    #[test]
    fn reopening_keeps_the_rows() {
        let tmp = tempfile::tempdir().unwrap();
        open(tmp.path(), NOW)
            .unwrap()
            .conn
            .execute("INSERT INTO settings VALUES ('k', '1')", [])
            .unwrap();
        let db = open(tmp.path(), NOW).unwrap();
        assert_eq!(db.found_version, CURRENT_VERSION);
        let v: String = db
            .conn
            .query_row("SELECT value FROM settings WHERE key = 'k'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(v, "1");
    }

    #[test]
    fn later_migrations_run_in_order_and_keep_the_rows() {
        const V2: &[Migration] = &[
            MIGRATIONS[0],
            Migration {
                version: 2,
                sql: "ALTER TABLE recent ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;",
            },
        ];
        let tmp = tempfile::tempdir().unwrap();
        open(tmp.path(), NOW)
            .unwrap()
            .conn
            .execute("INSERT INTO recent (root, opened_at) VALUES ('/a', 1)", [])
            .unwrap();
        let db = open_with(tmp.path(), NOW, V2).unwrap();
        assert_eq!(db.found_version, 1);
        let pinned: i64 = db
            .conn
            .query_row("SELECT pinned FROM recent WHERE root = '/a'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(pinned, 0);
    }

    #[test]
    fn a_failing_migration_changes_nothing() {
        const BROKEN: &[Migration] = &[
            MIGRATIONS[0],
            Migration {
                version: 2,
                sql: "CREATE TABLE fine (x); THIS IS NOT SQL;",
            },
        ];
        let tmp = tempfile::tempdir().unwrap();
        drop(open(tmp.path(), NOW).unwrap());
        let e = open_with(tmp.path(), NOW, BROKEN).unwrap_err();
        assert_eq!(e.code, crate::ErrorCode::Internal);
        let db = open(tmp.path(), NOW).unwrap();
        assert_eq!(db.found_version, 1);
        assert!(!tables(&db.conn).contains(&"fine".to_owned()));
        drop(db);
        assert_eq!(
            names(tmp.path()),
            [FILE_NAME],
            "a failed migration is not damage"
        );
    }

    #[test]
    fn a_file_from_a_newer_owlbum_is_used_as_it_is() {
        let tmp = tempfile::tempdir().unwrap();
        {
            let db = open(tmp.path(), NOW).unwrap();
            db.conn
                .execute("INSERT INTO settings VALUES ('thumb_size', '200')", [])
                .unwrap();
            db.conn
                .pragma_update(None, "user_version", CURRENT_VERSION + 3)
                .unwrap();
        }
        let db = open(tmp.path(), NOW).unwrap();
        assert_eq!(db.found_version, CURRENT_VERSION + 3);
        assert!(db.set_aside.is_none());
        let v: i64 = db
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, i64::from(CURRENT_VERSION + 3), "not lowered");
        let n: i64 = db
            .conn
            .query_row("SELECT count(*) FROM settings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn a_file_that_is_not_a_database_is_set_aside_and_replaced() {
        let tmp = tempfile::tempdir().unwrap();
        let garbage = b"this is not a database, just some bytes that are long enough to look like a header at least".repeat(20);
        std::fs::write(tmp.path().join(FILE_NAME), &garbage).unwrap();

        let db = open(tmp.path(), NOW).unwrap();
        let moved = db.set_aside.clone().expect("set aside");
        assert_eq!(moved, tmp.path().join("app.damaged-20261126-090503.sqlite"));
        assert_eq!(std::fs::read(&moved).unwrap(), garbage, "kept, not deleted");
        assert_eq!(db.found_version, 0);
        assert_eq!(tables(&db.conn), ["cache_entries", "recent", "settings"]);
        drop(db);
        assert_eq!(
            names(tmp.path()),
            ["app.damaged-20261126-090503.sqlite", FILE_NAME]
        );
    }

    #[test]
    fn a_file_with_corrupt_pages_is_set_aside() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(FILE_NAME);
        {
            let db = open(tmp.path(), NOW).unwrap();
            let tx = db.conn.unchecked_transaction().unwrap();
            for i in 0..2000 {
                tx.execute(
                    "INSERT INTO recent (root, opened_at, session) VALUES (?1, ?2, ?3)",
                    (format!("/photos/{i:05}"), i, "x".repeat(100)),
                )
                .unwrap();
            }
            tx.commit().unwrap();
        }
        // Scribble over every page after the first two, leaving the header
        // and the schema page alone.
        let mut bytes = std::fs::read(&path).unwrap();
        assert!(bytes.len() > 16 * 4096, "{}", bytes.len());
        for b in &mut bytes[2 * 4096..] {
            *b = 0x5a;
        }
        std::fs::write(&path, &bytes).unwrap();

        let db = open(tmp.path(), NOW).unwrap();
        assert!(db.set_aside.is_some());
        let n: i64 = db
            .conn
            .query_row("SELECT count(*) FROM recent", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn a_second_damaged_file_in_the_same_second_gets_its_own_name() {
        let tmp = tempfile::tempdir().unwrap();
        for _ in 0..2 {
            std::fs::write(tmp.path().join(FILE_NAME), b"not a database".repeat(100)).unwrap();
            drop(open(tmp.path(), NOW).unwrap());
            std::fs::remove_file(tmp.path().join(FILE_NAME)).unwrap();
        }
        assert_eq!(
            names(tmp.path()),
            [
                "app.damaged-20261126-090503-2.sqlite",
                "app.damaged-20261126-090503.sqlite"
            ]
        );
    }

    #[test]
    fn a_folder_that_cannot_hold_the_file_is_an_error_not_damage() {
        let tmp = tempfile::tempdir().unwrap();
        // A folder where the file should be: SQLite cannot open it, and it
        // must not be renamed as if it were damaged.
        std::fs::create_dir(tmp.path().join(FILE_NAME)).unwrap();
        let e = open(tmp.path(), NOW).unwrap_err();
        assert_eq!(e.code, crate::ErrorCode::Internal);
        assert_eq!(names(tmp.path()), [FILE_NAME]);
        assert!(tmp.path().join(FILE_NAME).is_dir());
    }
}
