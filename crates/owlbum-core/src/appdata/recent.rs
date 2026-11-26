//! The recent list and each folder's saved session (docs/02-data.md,
//! "`app.sqlite`"; docs/04-browsing.md, "Reopening where I was").
//!
//! The list keeps the [`KEEP`] most recently opened folders. Opening a
//! folder or saving its session moves it to the top. Each entry may hold the
//! folder's session as JSON; fields this version does not know are kept when
//! a session is saved again.
//!
//! Whether each folder still exists is checked when the list is read, all at
//! once, with a time limit per check, so an unreachable network path never
//! holds up the start.

use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::appdata::db::sqlite_error;
use crate::error::AppResult;
use crate::text;

/// How many folders the list keeps.
pub const KEEP: usize = 12;

/// How long the check that a recent folder exists may take before the
/// folder counts as not found.
pub const EXISTS_TIMEOUT: Duration = Duration::from_millis(1500);

/// One entry of the recent list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Recent {
    /// The absolute path of the folder that was opened.
    pub root: String,
    /// The folder's own name, for display.
    pub name: String,
    /// When it was last opened, in milliseconds since the epoch.
    pub opened_at: i64,
    /// Whether the folder was found when the list was read.
    pub exists: bool,
}

/// Checks whether a folder exists. It may block; [`list`] gives each call
/// at most a time limit.
pub type Probe = Arc<dyn Fn(&Path) -> bool + Send + Sync>;

/// The real check: the path is a folder.
pub fn folder_exists() -> Probe {
    Arc::new(|p: &Path| p.is_dir())
}

/// The recent list, newest first, without checking that the folders exist
/// (`exists` is true for every entry).
pub fn entries(conn: &Connection) -> AppResult<Vec<Recent>> {
    let read = sqlite_error(text::appdata::READING_SETTINGS);
    let mut stmt = conn
        .prepare("SELECT root, opened_at FROM recent ORDER BY opened_at DESC, root")
        .map_err(&read)?;
    let rows = stmt
        .query_map([], |r| {
            let root: String = r.get(0)?;
            Ok(Recent {
                name: display_name(&root),
                root,
                opened_at: r.get(1)?,
                exists: true,
            })
        })
        .map_err(&read)?;
    rows.collect::<Result<_, _>>().map_err(read)
}

/// The recent list, newest first, with `exists` checked by `probe`. All
/// folders are checked at once; one whose check has not answered within
/// `timeout` counts as not found.
pub fn list(conn: &Connection, probe: &Probe, timeout: Duration) -> AppResult<Vec<Recent>> {
    Ok(with_exists(entries(conn)?, probe, timeout))
}

/// Sets `exists` on each entry of `list` with `probe`, all at once, each
/// check given at most `timeout`. Needs no connection, so a caller can let
/// go of the database while folders are checked.
pub fn with_exists(mut list: Vec<Recent>, probe: &Probe, timeout: Duration) -> Vec<Recent> {
    let found = check_all(
        list.iter().map(|r| r.root.clone()).collect(),
        probe,
        timeout,
    );
    for (entry, exists) in list.iter_mut().zip(found) {
        entry.exists = exists;
    }
    list
}

/// Runs `probe` on every root on its own thread and waits at most `timeout`
/// for all of them. A check that is still running is left to finish on its
/// own; its answer is dropped.
fn check_all(roots: Vec<String>, probe: &Probe, timeout: Duration) -> Vec<bool> {
    let mut found = vec![false; roots.len()];
    let (tx, rx) = mpsc::channel();
    for (i, root) in roots.into_iter().enumerate() {
        let tx = tx.clone();
        let probe = Arc::clone(probe);
        let spawned = std::thread::Builder::new()
            .name("owlbum-recent-exists".into())
            .spawn(move || {
                let _ = tx.send((i, probe(Path::new(&root))));
            });
        if spawned.is_err() {
            // No thread: the folder counts as not found.
            continue;
        }
    }
    drop(tx);
    let deadline = Instant::now() + timeout;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok((i, exists)) => found[i] = exists,
            Err(_) => break,
        }
    }
    found
}

/// Puts `root` at the top of the list, opened at `now_ms`, and trims the
/// list to [`KEEP`] entries. Its session, if any, is kept.
pub fn touch(conn: &Connection, root: &str, now_ms: i64) -> AppResult<()> {
    let write = sqlite_error(text::appdata::SAVING_SETTINGS);
    let tx = conn.unchecked_transaction().map_err(&write)?;
    touch_in(&tx, root, now_ms)?;
    tx.commit().map_err(write)
}

fn touch_in(conn: &Connection, root: &str, now_ms: i64) -> AppResult<()> {
    let write = sqlite_error(text::appdata::SAVING_SETTINGS);
    // The newest entry must sort first even when two opens share a
    // millisecond, or the clock went back.
    conn.execute(
        "INSERT INTO recent (root, opened_at)
           VALUES (?1, max(?2, coalesce((SELECT max(opened_at) + 1 FROM recent WHERE root <> ?1), ?2)))
         ON CONFLICT (root) DO UPDATE SET opened_at = excluded.opened_at",
        (root, now_ms),
    )
    .map_err(&write)?;
    conn.execute(
        "DELETE FROM recent WHERE root NOT IN
           (SELECT root FROM recent ORDER BY opened_at DESC, root LIMIT ?1)",
        [i64::try_from(KEEP).unwrap_or(i64::MAX)],
    )
    .map_err(&write)?;
    Ok(())
}

/// Removes `root` from the list. Nothing else is touched. Removing a folder
/// that is not listed does nothing.
pub fn remove(conn: &Connection, root: &str) -> AppResult<()> {
    conn.execute("DELETE FROM recent WHERE root = ?1", [root])
        .map_err(sqlite_error(text::appdata::SAVING_SETTINGS))?;
    Ok(())
}

/// The saved session of `root`, or `None` when it has none (or is not on
/// the list, or its stored session is not JSON).
pub fn session(conn: &Connection, root: &str) -> AppResult<Option<Value>> {
    let text: Option<Option<String>> = conn
        .query_row("SELECT session FROM recent WHERE root = ?1", [root], |r| {
            r.get(0)
        })
        .optional()
        .map_err(sqlite_error(text::appdata::READING_SETTINGS))?;
    Ok(text.flatten().and_then(|t| serde_json::from_str(&t).ok()))
}

/// Saves `session` for `root` and moves it to the top of the list. When
/// both the saved and the new session are JSON objects, fields of the saved
/// one that `session` does not have are kept, so a field written by a newer
/// Owlbum survives a save by this one.
pub fn save_session(conn: &Connection, root: &str, session: &Value, now_ms: i64) -> AppResult<()> {
    let write = sqlite_error(text::appdata::SAVING_SETTINGS);
    let tx = conn.unchecked_transaction().map_err(&write)?;
    let merged = match (self::session(&tx, root)?, session) {
        (Some(Value::Object(mut old)), Value::Object(new)) => {
            for (k, v) in new {
                old.insert(k.clone(), v.clone());
            }
            Value::Object(old)
        }
        _ => session.clone(),
    };
    touch_in(&tx, root, now_ms)?;
    tx.execute(
        "UPDATE recent SET session = ?2 WHERE root = ?1",
        (root, merged.to_string()),
    )
    .map_err(&write)?;
    tx.commit().map_err(write)
}

/// The folder's own name: the last part of the path, whichever separator it
/// uses, or the whole path for a drive or file-system root.
pub fn display_name(root: &str) -> String {
    root.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty() && !s.ends_with(':'))
        .map_or_else(|| root.to_owned(), str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(crate::appdata::db::MIGRATIONS[0].sql)
            .unwrap();
        c
    }

    fn roots(c: &Connection) -> Vec<String> {
        entries(c).unwrap().into_iter().map(|r| r.root).collect()
    }

    fn always(answer: bool) -> Probe {
        Arc::new(move |_: &Path| answer)
    }

    #[test]
    fn an_empty_list_is_empty() {
        assert!(entries(&conn()).unwrap().is_empty());
    }

    #[test]
    fn the_newest_is_first() {
        let c = conn();
        touch(&c, "/photos/a", 1000).unwrap();
        touch(&c, "/photos/b", 2000).unwrap();
        touch(&c, "/photos/c", 3000).unwrap();
        assert_eq!(roots(&c), ["/photos/c", "/photos/b", "/photos/a"]);
        touch(&c, "/photos/a", 4000).unwrap();
        assert_eq!(roots(&c), ["/photos/a", "/photos/c", "/photos/b"]);
        assert_eq!(entries(&c).unwrap()[0].opened_at, 4000);
    }

    #[test]
    fn opens_in_the_same_millisecond_keep_their_order() {
        let c = conn();
        for root in ["/z", "/a", "/m", "/a", "/b"] {
            touch(&c, root, 5000).unwrap();
        }
        assert_eq!(roots(&c), ["/b", "/a", "/m", "/z"]);
    }

    #[test]
    fn a_clock_that_went_back_still_puts_the_newest_first() {
        let c = conn();
        touch(&c, "/a", 9000).unwrap();
        touch(&c, "/b", 1000).unwrap();
        assert_eq!(roots(&c), ["/b", "/a"]);
    }

    #[test]
    fn the_list_keeps_twelve() {
        let c = conn();
        for i in 0..20 {
            touch(&c, &format!("/photos/{i:02}"), 1000 + i).unwrap();
        }
        let r = roots(&c);
        assert_eq!(r.len(), KEEP);
        assert_eq!(KEEP, 12);
        assert_eq!(r.first().unwrap(), "/photos/19");
        assert_eq!(r.last().unwrap(), "/photos/08");
    }

    #[test]
    fn remove_takes_out_one_entry_only() {
        let c = conn();
        touch(&c, "/a", 1).unwrap();
        touch(&c, "/b", 2).unwrap();
        save_session(&c, "/a", &json!({ "view": "map" }), 3).unwrap();
        remove(&c, "/b").unwrap();
        assert_eq!(roots(&c), ["/a"]);
        assert_eq!(session(&c, "/a").unwrap(), Some(json!({ "view": "map" })));
        remove(&c, "/not-listed").unwrap();
        assert_eq!(roots(&c), ["/a"]);
    }

    #[test]
    fn names_are_the_folders_own_names() {
        assert_eq!(display_name("/home/sam/Photos/Trip 2026"), "Trip 2026");
        assert_eq!(display_name(r"C:\Users\sam\Pictures\"), "Pictures");
        assert_eq!(display_name(r"\\nas\share\photos"), "photos");
        assert_eq!(display_name(r"D:\"), r"D:\");
        assert_eq!(display_name("/"), "/");
        let c = conn();
        touch(&c, "/home/sam/Trip", 1).unwrap();
        assert_eq!(entries(&c).unwrap()[0].name, "Trip");
    }

    #[test]
    fn list_reports_which_folders_exist() {
        let tmp = tempfile::tempdir().unwrap();
        let here = tmp.path().join("here");
        std::fs::create_dir(&here).unwrap();
        let gone = tmp.path().join("gone");
        let c = conn();
        touch(&c, &here.to_string_lossy(), 1).unwrap();
        touch(&c, &gone.to_string_lossy(), 2).unwrap();
        let l = list(&c, &folder_exists(), EXISTS_TIMEOUT).unwrap();
        assert_eq!(
            l.iter().map(|r| r.exists).collect::<Vec<_>>(),
            [false, true]
        );
    }

    #[test]
    fn a_file_is_not_a_folder_that_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("a.jpg");
        std::fs::write(&file, b"x").unwrap();
        assert!(!folder_exists()(&file));
        assert!(folder_exists()(tmp.path()));
    }

    #[test]
    fn a_check_that_hangs_counts_as_not_found_and_does_not_hold_up_the_list() {
        let c = conn();
        for (i, root) in ["/fast", "/slow-1", "/slow-2", "/slow-3"]
            .iter()
            .enumerate()
        {
            touch(&c, root, i64::try_from(i).unwrap()).unwrap();
        }
        let probe: Probe = Arc::new(|p: &Path| {
            if p.to_string_lossy().contains("slow") {
                std::thread::sleep(Duration::from_secs(5));
            }
            true
        });
        let start = Instant::now();
        let l = list(&c, &probe, Duration::from_millis(300)).unwrap();
        let took = start.elapsed();
        assert!(
            took < Duration::from_millis(1500),
            "checks run at once, so the wait is one limit, not four: {took:?}"
        );
        let exists: Vec<(&str, bool)> = l.iter().map(|r| (r.root.as_str(), r.exists)).collect();
        assert_eq!(
            exists,
            [
                ("/slow-3", false),
                ("/slow-2", false),
                ("/slow-1", false),
                ("/fast", true)
            ]
        );
    }

    #[test]
    fn list_with_every_folder_present_answers_without_waiting_for_the_limit() {
        let c = conn();
        touch(&c, "/a", 1).unwrap();
        let start = Instant::now();
        let l = list(&c, &always(true), Duration::from_secs(10)).unwrap();
        assert!(l[0].exists);
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_session_round_trips_and_moves_the_folder_to_the_top() {
        let c = conn();
        touch(&c, "/a", 1).unwrap();
        touch(&c, "/b", 2).unwrap();
        let s = json!({ "view": "timeline", "sort": "date", "filter": {}, "selected": "2026/a.jpg", "scroll": 40 });
        save_session(&c, "/a", &s, 3).unwrap();
        assert_eq!(session(&c, "/a").unwrap(), Some(s));
        assert_eq!(roots(&c), ["/a", "/b"]);
        assert_eq!(session(&c, "/b").unwrap(), None);
        assert_eq!(session(&c, "/nowhere").unwrap(), None);
    }

    #[test]
    fn saving_a_session_for_a_new_folder_adds_it() {
        let c = conn();
        save_session(&c, "/new", &json!({ "view": "grid" }), 7).unwrap();
        assert_eq!(roots(&c), ["/new"]);
    }

    #[test]
    fn touching_keeps_the_session() {
        let c = conn();
        save_session(&c, "/a", &json!({ "view": "map" }), 1).unwrap();
        touch(&c, "/a", 2).unwrap();
        assert_eq!(session(&c, "/a").unwrap(), Some(json!({ "view": "map" })));
    }

    #[test]
    fn unknown_session_fields_are_kept() {
        let c = conn();
        save_session(
            &c,
            "/a",
            &json!({ "view": "map", "scroll": 3, "future": { "zoom": 2 } }),
            1,
        )
        .unwrap();
        save_session(&c, "/a", &json!({ "view": "grid", "scroll": 9 }), 2).unwrap();
        assert_eq!(
            session(&c, "/a").unwrap(),
            Some(json!({ "view": "grid", "scroll": 9, "future": { "zoom": 2 } }))
        );
    }

    #[test]
    fn a_stored_session_that_is_not_json_reads_as_none() {
        let c = conn();
        c.execute(
            "INSERT INTO recent (root, opened_at, session) VALUES ('/a', 1, '{oops')",
            [],
        )
        .unwrap();
        assert_eq!(session(&c, "/a").unwrap(), None);
        save_session(&c, "/a", &json!({ "view": "map" }), 2).unwrap();
        assert_eq!(session(&c, "/a").unwrap(), Some(json!({ "view": "map" })));
    }
}
