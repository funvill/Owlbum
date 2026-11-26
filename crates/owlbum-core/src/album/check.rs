//! Checks made on every open before anything is written (docs/02-data.md,
//! "Backups and damage"): the file's header, `PRAGMA quick_check`, and
//! copies of the database left by a sync tool.

use std::io::{self, Read};
use std::path::Path;

use rusqlite::{Connection, ErrorCode, OpenFlags};

use crate::album::{db, sqlite_error};
use crate::error::AppResult;
use crate::text;

/// What the first 100 bytes of a database file say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Header {
    /// An SQLite database (or an empty file, which SQLite treats as a new
    /// database) at this `user_version`.
    Version(u32),
    /// Not an SQLite database at all: damaged.
    NotADatabase,
}

/// Reads the header of the database file at `path` without opening it with
/// SQLite, so a newer album is recognised before any lock or journal is
/// touched.
pub fn read_header(path: &Path) -> io::Result<Header> {
    /// The first 16 bytes of every SQLite database.
    const MAGIC: &[u8; 16] = b"SQLite format 3\0";
    let mut header = Vec::with_capacity(100);
    std::fs::File::open(path)?
        .take(100)
        .read_to_end(&mut header)?;
    if header.is_empty() {
        return Ok(Header::Version(0));
    }
    if header.len() < 100 || &header[..16] != MAGIC {
        return Ok(Header::NotADatabase);
    }
    // `user_version` is the big-endian integer at offset 60.
    let version = i32::from_be_bytes([header[60], header[61], header[62], header[63]]);
    Ok(Header::Version(u32::try_from(version).unwrap_or(u32::MAX)))
}

/// Runs `PRAGMA quick_check` on the database at `path`. Answers false when
/// the database is damaged: the check reports a problem, or SQLite says the
/// file is corrupt or not a database. Other failures (the file cannot be
/// opened at all) are errors.
///
/// `writable` opens the connection for writing, so a journal left by a
/// crash is rolled back first; the caller must hold the lock.
pub fn quick_check(path: &Path, writable: bool) -> AppResult<bool> {
    let failed = sqlite_error(text::album::CHECKING_DATABASE);
    let flags = if writable {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    } | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = Connection::open_with_flags(path, flags).map_err(&failed)?;
    conn.busy_timeout(db::BUSY_TIMEOUT).map_err(&failed)?;
    let mut answer = Vec::new();
    let result = conn.prepare("PRAGMA quick_check").and_then(|mut stmt| {
        stmt.query_map([], |r| r.get::<_, String>(0))?
            .try_for_each(|row| row.map(|line| answer.push(line)))
    });
    match result {
        Ok(()) => Ok(answer.len() == 1 && answer[0] == "ok"),
        Err(e) if is_damage(&e) => Ok(false),
        Err(e) => Err(failed(e)),
    }
}

/// True for the SQLite errors that mean the file itself is bad.
fn is_damage(e: &rusqlite::Error) -> bool {
    matches!(
        e.sqlite_error_code(),
        Some(ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase)
    )
}

/// The name of a copy of the database that a sync tool left beside it in
/// `.owlbum` (`owlbum (conflicted copy).sqlite`, `owlbum-PC-NAME.sqlite`,
/// `owlbum.sync-conflict-â€¦.sqlite`), or None. When there are several, the
/// first by name. Owlbum's own files never match.
pub fn conflict_file(owlbum_dir: &Path) -> Option<String> {
    let mut found: Vec<String> = std::fs::read_dir(owlbum_dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| is_conflict_name(name))
        .collect();
    found.sort();
    found.into_iter().next()
}

/// A sync tool names its copy after the original: it starts with `owlbum`
/// and ends with `.sqlite`. The database itself and the damaged files that
/// Owlbum keeps after a restore (`owlbum.damaged-…`) are not copies.
fn is_conflict_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.starts_with("owlbum")
        && lower.ends_with(".sqlite")
        && lower != super::root::DATABASE_FILE
        && !lower.starts_with(DAMAGED_PREFIX)
}

/// The start of the name a damaged database is renamed to.
pub const DAMAGED_PREFIX: &str = "owlbum.damaged-";

#[cfg(test)]
mod tests {
    use super::*;

    fn make_db(path: &Path, rows: usize) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            "PRAGMA journal_mode = DELETE; CREATE TABLE t (n INTEGER, s TEXT); CREATE INDEX t_s ON t (s);",
        )
        .unwrap();
        let tx = conn.unchecked_transaction().unwrap();
        for i in 0..rows as i64 {
            tx.execute(
                "INSERT INTO t VALUES (?1, ?2)",
                rusqlite::params![i, format!("row {i} {}", "x".repeat(200))],
            )
            .unwrap();
        }
        tx.commit().unwrap();
        conn.pragma_update(None, "user_version", 7).unwrap();
    }

    /// Overwrites the middle of the file with junk, past the header.
    fn scribble(path: &Path) {
        let mut bytes = std::fs::read(path).unwrap();
        let start = bytes.len() / 2;
        for b in &mut bytes[start..start + 4096] {
            *b = 0x5a;
        }
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn the_header_gives_the_user_version() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("a.sqlite");
        make_db(&path, 1);
        assert_eq!(read_header(&path).unwrap(), Header::Version(7));
    }

    #[test]
    fn an_empty_file_is_a_new_database() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("a.sqlite");
        std::fs::write(&path, b"").unwrap();
        assert_eq!(read_header(&path).unwrap(), Header::Version(0));
    }

    #[test]
    fn junk_is_not_a_database() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("a.sqlite");
        std::fs::write(&path, b"hello").unwrap();
        assert_eq!(read_header(&path).unwrap(), Header::NotADatabase);
        std::fs::write(&path, vec![7u8; 8192]).unwrap();
        assert_eq!(read_header(&path).unwrap(), Header::NotADatabase);
    }

    #[test]
    fn a_sound_database_passes() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("a.sqlite");
        make_db(&path, 500);
        assert!(quick_check(&path, false).unwrap());
        assert!(quick_check(&path, true).unwrap());
    }

    #[test]
    fn a_scribbled_database_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("a.sqlite");
        make_db(&path, 500);
        scribble(&path);
        assert!(!quick_check(&path, false).unwrap());
    }

    #[test]
    fn a_file_that_is_not_a_database_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("a.sqlite");
        std::fs::write(&path, vec![7u8; 8192]).unwrap();
        assert!(!quick_check(&path, false).unwrap());
        assert!(!quick_check(&path, true).unwrap());
    }

    #[test]
    fn checking_read_only_changes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("a.sqlite");
        make_db(&path, 50);
        let before = std::fs::read(&path).unwrap();
        quick_check(&path, false).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 1);
    }

    #[test]
    fn sync_conflict_copies_are_found() {
        for name in [
            "owlbum (conflicted copy).sqlite",
            "owlbum-PC-NAME.sqlite",
            "owlbum (Steve's conflicted copy 2026-11-25).sqlite",
            "owlbum.sync-conflict-20261126-101010-ABCDEFG.sqlite",
            "owlbum (1).sqlite",
            "Owlbum-LAPTOP.SQLITE",
        ] {
            let tmp = tempfile::tempdir().unwrap();
            std::fs::write(tmp.path().join("owlbum.sqlite"), b"").unwrap();
            std::fs::write(tmp.path().join(name), b"").unwrap();
            assert_eq!(conflict_file(tmp.path()).as_deref(), Some(name), "{name}");
        }
    }

    #[test]
    fn owlbums_own_files_are_not_conflicts() {
        let tmp = tempfile::tempdir().unwrap();
        for name in [
            "owlbum.sqlite",
            "owlbum.sqlite-journal",
            "owlbum.damaged-20261126-101010.sqlite",
            "owlbum.damaged-20261126-101010.sqlite-journal",
            "lock",
            "lock.tmp",
        ] {
            std::fs::write(tmp.path().join(name), b"").unwrap();
        }
        std::fs::create_dir(tmp.path().join("backup")).unwrap();
        std::fs::write(
            tmp.path().join("backup").join("owlbum-2026-11-25.sqlite"),
            b"",
        )
        .unwrap();
        assert_eq!(conflict_file(tmp.path()), None);
    }

    #[test]
    fn a_missing_folder_has_no_conflict() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(conflict_file(&tmp.path().join("nope")), None);
    }

    #[test]
    fn the_first_conflict_by_name_is_named() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("owlbum-ZED.sqlite"), b"").unwrap();
        std::fs::write(tmp.path().join("owlbum-ALPHA.sqlite"), b"").unwrap();
        assert_eq!(
            conflict_file(tmp.path()).as_deref(),
            Some("owlbum-ALPHA.sqlite")
        );
    }
}
