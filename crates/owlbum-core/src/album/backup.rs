//! Daily backups and restoring one (docs/02-data.md, "Backups and damage").
//!
//! On the first writable open of each calendar day (local time) the
//! database is copied to `.owlbum/backup/owlbum-<YYYY-MM-DD>.sqlite` with
//! SQLite's backup API, after the damage check and before any migration.
//! When no migration is waiting, [`super::Album::open`] makes it on a
//! thread of its own after the open answers ([`daily_gated`]), taking turns
//! with the album's writer while it reads. The copy leaves out the
//! `vectors` table. The newest [`KEEP`] copies are kept, judged by the date
//! in the name.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Local, NaiveDate};
use rusqlite::{Connection, MAIN_DB, OpenFlags};

use crate::AppError;
use crate::album::{check, db, root, sqlite_error};
use crate::error::AppResult;
use crate::text;

/// How many daily copies are kept.
pub const KEEP: usize = 3;

/// The folder inside `.owlbum` that holds the copies.
pub const BACKUP_DIR: &str = "backup";

/// `owlbum-<YYYY-MM-DD>.sqlite`.
pub fn file_name(date: NaiveDate) -> String {
    format!("owlbum-{}.sqlite", date.format("%Y-%m-%d"))
}

/// The copies in `<owlbum_dir>/backup`, newest first. Files whose names are
/// not a copy's (a copy being written, for example) are left out.
pub fn list(owlbum_dir: &Path) -> Vec<(NaiveDate, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(owlbum_dir.join(BACKUP_DIR)) else {
        return Vec::new();
    };
    let mut copies: Vec<(NaiveDate, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let date = name.strip_prefix("owlbum-")?.strip_suffix(".sqlite")?;
            let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
            (name == file_name(date)).then(|| (date, e.path()))
        })
        .collect();
    copies.sort_by_key(|c| std::cmp::Reverse(c.0));
    copies
}

/// Makes today's copy of the database at `db_path` if there is none yet,
/// then keeps the newest [`KEEP`]. Answers the new copy's path, or None when
/// today's copy already existed.
pub fn daily(db_path: &Path, owlbum_dir: &Path, today: NaiveDate) -> AppResult<Option<PathBuf>> {
    daily_gated(db_path, owlbum_dir, today, None)
}

/// [`daily`] while the album is open: the database is read with `gate` held
/// shared, as the album's readers do, so the copy and the album's writer
/// take turns rather than meet SQLite's busy handler. Only the read of the
/// database holds it, not the compaction of the copy.
pub fn daily_gated(
    db_path: &Path,
    owlbum_dir: &Path,
    today: NaiveDate,
    gate: Option<&db::Gate>,
) -> AppResult<Option<PathBuf>> {
    let dir = owlbum_dir.join(BACKUP_DIR);
    let path = dir.join(file_name(today));
    if path.exists() {
        return Ok(None);
    }
    let io_failed = |e: std::io::Error| {
        AppError::internal(text::album::BACKING_UP)
            .with_details(serde_json::json!({ "cause": e.to_string() }))
    };
    std::fs::create_dir_all(&dir).map_err(io_failed)?;
    let partial = dir.join(format!("{}.partial", file_name(today)));
    let made = copy_without_vectors(db_path, &partial, gate)
        .and_then(|()| std::fs::rename(&partial, &path).map_err(io_failed));
    if let Err(e) = made {
        let _ = std::fs::remove_file(&partial);
        return Err(e);
    }
    for (_, old) in list(owlbum_dir).into_iter().skip(KEEP) {
        let _ = std::fs::remove_file(old);
    }
    Ok(Some(path))
}

/// Copies the database with SQLite's backup API, then empties `vectors` in
/// the copy and compacts it.
fn copy_without_vectors(db_path: &Path, to: &Path, gate: Option<&db::Gate>) -> AppResult<()> {
    let failed = sqlite_error(text::album::BACKING_UP);
    let _ = std::fs::remove_file(to);
    let src = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(&failed)?;
    src.busy_timeout(db::BUSY_TIMEOUT).map_err(&failed)?;
    {
        let _turn = gate.map(|g| g.read().unwrap_or_else(|p| p.into_inner()));
        src.backup(MAIN_DB, to, None::<fn(rusqlite::backup::Progress)>)
            .map_err(&failed)?;
        drop(src);
    }

    let copy = Connection::open(to).map_err(&failed)?;
    let has_vectors: bool = copy
        .query_row(
            "SELECT count(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = 'vectors'",
            [],
            |r| r.get(0),
        )
        .map_err(&failed)?;
    if has_vectors {
        copy.execute_batch("DELETE FROM vectors; VACUUM;")
            .map_err(&failed)?;
    }
    Ok(())
}

/// The newest copy that passes `quick_check`, or None.
pub fn newest_sound(owlbum_dir: &Path) -> Option<(NaiveDate, PathBuf)> {
    list(owlbum_dir)
        .into_iter()
        .find(|(_, path)| check::quick_check(path, false).unwrap_or(false))
}

/// Replaces a damaged database with the newest sound copy. The damaged file
/// (and its journal, if any) is renamed `owlbum.damaged-<timestamp>.sqlite`
/// beside it, never deleted. The copy stays in `backup/`. Answers the date
/// of the copy restored, or None (and changes nothing) when there is no
/// sound copy. The caller holds the lock and has no connection open.
pub fn restore(db_path: &Path, now: DateTime<Local>) -> AppResult<Option<NaiveDate>> {
    let failed = |e: std::io::Error| {
        AppError::internal(text::album::RESTORING_BACKUP)
            .with_details(serde_json::json!({ "cause": e.to_string() }))
    };
    let Some(owlbum_dir) = db_path.parent() else {
        return Ok(None);
    };
    let Some((date, copy)) = newest_sound(owlbum_dir) else {
        return Ok(None);
    };
    let restoring = owlbum_dir.join(format!("{}.restoring", root::DATABASE_FILE));
    std::fs::copy(&copy, &restoring).map_err(failed)?;

    let damaged = format!(
        "{}{}.sqlite",
        check::DAMAGED_PREFIX,
        now.format("%Y%m%d-%H%M%S")
    );
    let journal = owlbum_dir.join(format!("{}-journal", root::DATABASE_FILE));
    if journal.exists() {
        // A journal belongs to the damaged file; left in place it would be
        // played back into the restored one.
        std::fs::rename(&journal, owlbum_dir.join(format!("{damaged}-journal"))).map_err(failed)?;
    }
    std::fs::rename(db_path, owlbum_dir.join(&damaged)).map_err(failed)?;
    std::fs::rename(&restoring, db_path).map_err(failed)?;
    Ok(Some(date))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    /// An album-like database: photos and a big vectors table.
    fn make_album(owlbum_dir: &Path) -> PathBuf {
        std::fs::create_dir_all(owlbum_dir).unwrap();
        let path = owlbum_dir.join("owlbum.sqlite");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "PRAGMA journal_mode = DELETE;
             CREATE TABLE photos (id INTEGER PRIMARY KEY, name TEXT);
             CREATE TABLE vectors (photo_id INTEGER, vector BLOB);
             INSERT INTO photos (name) VALUES ('a.jpg'), ('b.jpg');",
        )
        .unwrap();
        let blob = vec![1u8; 64 * 1024];
        for i in 0..40 {
            conn.execute(
                "INSERT INTO vectors VALUES (?1, ?2)",
                rusqlite::params![i, blob],
            )
            .unwrap();
        }
        conn.pragma_update(None, "user_version", 1).unwrap();
        path
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    fn count(path: &Path, table: &str) -> i64 {
        Connection::open(path)
            .unwrap()
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn the_first_open_of_a_day_makes_a_copy_without_vectors() {
        let tmp = tempfile::tempdir().unwrap();
        let owl = tmp.path().join(".owlbum");
        let db = make_album(&owl);
        let copy = daily(&db, &owl, date("2026-11-25")).unwrap().unwrap();
        assert_eq!(copy, owl.join("backup").join("owlbum-2026-11-25.sqlite"));
        assert_eq!(count(&copy, "photos"), 2);
        assert_eq!(count(&copy, "vectors"), 0, "the table is there, empty");
        let small = std::fs::metadata(&copy).unwrap().len();
        let big = std::fs::metadata(&db).unwrap().len();
        assert!(small * 4 < big, "{small} vs {big}");
        assert_eq!(count(&db, "vectors"), 40, "the album keeps its vectors");
        assert_eq!(names(&owl.join("backup")), ["owlbum-2026-11-25.sqlite"]);
    }

    #[test]
    fn a_second_open_the_same_day_makes_no_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let owl = tmp.path().join(".owlbum");
        let db = make_album(&owl);
        daily(&db, &owl, date("2026-11-25")).unwrap();
        Connection::open(&db)
            .unwrap()
            .execute("INSERT INTO photos (name) VALUES ('c.jpg')", [])
            .unwrap();
        assert_eq!(daily(&db, &owl, date("2026-11-25")).unwrap(), None);
        assert_eq!(
            count(
                &owl.join("backup").join("owlbum-2026-11-25.sqlite"),
                "photos"
            ),
            2
        );
    }

    #[test]
    fn the_newest_three_are_kept_by_the_date_in_the_name() {
        let tmp = tempfile::tempdir().unwrap();
        let owl = tmp.path().join(".owlbum");
        let db = make_album(&owl);
        for d in ["2026-11-20", "2026-11-02", "2026-10-30", "2026-11-21"] {
            daily(&db, &owl, date(d)).unwrap();
        }
        std::fs::write(owl.join("backup").join("notes.txt"), b"keep me").unwrap();
        daily(&db, &owl, date("2026-11-25")).unwrap();
        assert_eq!(
            names(&owl.join("backup")),
            [
                "notes.txt",
                "owlbum-2026-11-20.sqlite",
                "owlbum-2026-11-21.sqlite",
                "owlbum-2026-11-25.sqlite",
            ]
        );
        let listed: Vec<NaiveDate> = list(&owl).into_iter().map(|(d, _)| d).collect();
        assert_eq!(
            listed,
            [date("2026-11-25"), date("2026-11-21"), date("2026-11-20")]
        );
    }

    #[test]
    fn list_is_empty_without_a_backup_folder() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(list(tmp.path()).is_empty());
        assert_eq!(newest_sound(tmp.path()), None);
    }

    #[test]
    fn restoring_keeps_the_damaged_file_and_its_journal() {
        let tmp = tempfile::tempdir().unwrap();
        let owl = tmp.path().join(".owlbum");
        let db = make_album(&owl);
        daily(&db, &owl, date("2026-11-24")).unwrap();
        std::fs::write(&db, vec![9u8; 8192]).unwrap();
        std::fs::write(owl.join("owlbum.sqlite-journal"), b"junk").unwrap();
        let now = Local::now();
        let restored = restore(&db, now).unwrap();
        assert_eq!(restored, Some(date("2026-11-24")));
        assert_eq!(count(&db, "photos"), 2);
        let stamp = now.format("%Y%m%d-%H%M%S").to_string();
        assert_eq!(
            names(&owl),
            [
                "backup".to_owned(),
                format!("owlbum.damaged-{stamp}.sqlite"),
                format!("owlbum.damaged-{stamp}.sqlite-journal"),
                "owlbum.sqlite".to_owned(),
            ]
        );
        assert_eq!(
            std::fs::read(owl.join(format!("owlbum.damaged-{stamp}.sqlite"))).unwrap(),
            vec![9u8; 8192]
        );
        assert!(
            owl.join("backup")
                .join("owlbum-2026-11-24.sqlite")
                .is_file()
        );
    }

    #[test]
    fn a_damaged_copy_is_passed_over() {
        let tmp = tempfile::tempdir().unwrap();
        let owl = tmp.path().join(".owlbum");
        let db = make_album(&owl);
        daily(&db, &owl, date("2026-11-23")).unwrap();
        let newest = daily(&db, &owl, date("2026-11-24")).unwrap().unwrap();
        std::fs::write(&newest, vec![9u8; 8192]).unwrap();
        assert_eq!(newest_sound(&owl).unwrap().0, date("2026-11-23"));
    }

    #[test]
    fn restoring_without_a_copy_changes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let owl = tmp.path().join(".owlbum");
        let db = make_album(&owl);
        std::fs::write(&db, vec![9u8; 8192]).unwrap();
        assert_eq!(restore(&db, Local::now()).unwrap(), None);
        assert_eq!(names(&owl), ["owlbum.sqlite"]);
        assert_eq!(std::fs::read(&db).unwrap(), vec![9u8; 8192]);
    }
}
