//! Daily backups and restoring one (docs/02-data.md, "Backups and damage").
//!
//! On the first writable open of each calendar day (local time) the
//! database is copied to `.owlbum/backup/owlbum-<YYYY-MM-DD>.sqlite` with
//! SQLite's backup API, before any migration. The copy leaves out the
//! `vectors` table. The newest [`KEEP`] copies are kept, judged by the date
//! in the name.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Local, NaiveDate};

use crate::error::AppResult;

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
    let _ = owlbum_dir;
    unimplemented!("#4")
}

/// Makes today's copy of the database at `db_path` if there is none yet,
/// then keeps the newest [`KEEP`]. Answers the new copy's path, or None when
/// today's copy already existed.
pub fn daily(db_path: &Path, owlbum_dir: &Path, today: NaiveDate) -> AppResult<Option<PathBuf>> {
    let _ = (db_path, owlbum_dir, today);
    unimplemented!("#4")
}

/// The newest copy that passes `quick_check`, or None.
pub fn newest_sound(owlbum_dir: &Path) -> Option<(NaiveDate, PathBuf)> {
    let _ = owlbum_dir;
    unimplemented!("#4")
}

/// Replaces a damaged database with the newest sound copy. The damaged file
/// (and its journal, if any) is renamed `owlbum.damaged-<timestamp>.sqlite`
/// beside it, never deleted. The copy stays in `backup/`. Answers the date
/// of the copy restored, or None (and changes nothing) when there is no
/// sound copy. The caller holds the lock and has no connection open.
pub fn restore(db_path: &Path, now: DateTime<Local>) -> AppResult<Option<NaiveDate>> {
    let _ = (db_path, now);
    unimplemented!("#4")
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
            conn.execute("INSERT INTO vectors VALUES (?1, ?2)", rusqlite::params![i, blob])
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
            count(&owl.join("backup").join("owlbum-2026-11-25.sqlite"), "photos"),
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
        assert!(owl.join("backup").join("owlbum-2026-11-24.sqlite").is_file());
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
