//! Tidy (docs/05-organising.md, "Tidy"): list the rows whose files are
//! gone (`missing = 1`) and delete the ones the user confirms.
//!
//! Only album database rows are deleted; no photo file is ever touched.
//! Missing rows are not in the album index, so deleting them leaves the
//! index, its bursts and its scores as they were. A row whose file is
//! found again is marked `missing = 0` instead, joins the index, and the
//! bursts are worked out again so it lands in its burst.

use std::collections::{BTreeMap, HashSet};
use std::io::ErrorKind;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::album::{Album, sqlite_error};
use crate::error::{AppError, AppResult, ErrorCode};
use crate::scan::order::natural_cmp_text;
use crate::scan::walk::absolute;
use crate::text;
use crate::text::album::tidy as t;

/// One missing file in the Tidy list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TidyFile {
    pub id: i64,
    pub file_name: String,
    /// What the row would lose, as shown beside it: any of "starred",
    /// "hidden", "notes", "description", "N people", "date", "place",
    /// "edit". Empty when it holds nothing typed. Tags are not listed.
    pub keeps: Vec<String>,
}

/// One folder of the Tidy list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TidyFolder {
    /// The folder, relative to the album root; `""` for the top folder.
    pub folder: String,
    /// The folder itself does not exist on disk.
    pub gone: bool,
    /// Its missing files, in the natural order of their names.
    pub files: Vec<TidyFile>,
}

/// `album_tidy_remove`'s answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TidyRemoved {
    /// Rows deleted.
    pub removed: u32,
    /// Rows kept because their files were found again (now `missing = 0`).
    pub kept: u32,
    /// The album's revision after the removal; unchanged when nothing was
    /// written.
    pub revision: u64,
}

/// The rows of the scope, as the album index tests it: a plain comparison,
/// not `LIKE`. `?1` is the scope.
const IN_SCOPE: &str =
    "(?1 = '' OR rel_dir = ?1 OR substr(rel_dir, 1, length(?1) + 1) = ?1 || '/')";

/// The checks both commands make first: not view-only (for a removal),
/// no scan running, and the album root reachable.
fn check(album: &Album, scan_running: bool, writing: bool) -> AppResult<()> {
    if writing && album.is_view_only() {
        return Err(AppError::new(ErrorCode::ViewOnly, text::photo::VIEW_ONLY));
    }
    if scan_running {
        return Err(AppError::new(ErrorCode::ScanRunning, t::SCAN_RUNNING));
    }
    let root = &album.location().root;
    match std::fs::metadata(root) {
        Ok(m) if m.is_dir() => Ok(()),
        _ => Err(AppError::new(ErrorCode::AlbumUnreachable, t::UNREACHABLE)),
    }
}

/// `album_tidy_list()`: the missing rows in scope, grouped by folder in the
/// natural order. `scan_running` when a scan runs (missing is only
/// trustworthy once one has finished), `album_unreachable` when the album
/// root cannot be reached.
pub fn list(album: &Album, scan_running: bool) -> AppResult<Vec<TidyFolder>> {
    check(album, scan_running, false)?;
    let scope = album.location().scope.clone();
    type Raw = (i64, String, String, [bool; 7], i64);
    let rows: Vec<Raw> = album.db().read(move |c| {
        let failed = sqlite_error(text::album::READING_DATABASE);
        let mut st = c
            .prepare(&format!(
                "SELECT id, rel_dir, file_name, star, hidden, notes IS NOT NULL AND notes != '',
                        description IS NOT NULL AND description != '', date_override IS NOT NULL,
                        loc_lat IS NOT NULL OR loc_name IS NOT NULL, edit IS NOT NULL,
                        (SELECT count(*) FROM photo_people pp WHERE pp.photo_id = photos.id)
                   FROM photos WHERE missing = 1 AND {IN_SCOPE}"
            ))
            .map_err(&failed)?;
        st.query_map([&scope], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                [
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                    r.get(9)?,
                ],
                r.get(10)?,
            ))
        })
        .map_err(&failed)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(&failed)
    })?;

    let mut folders: BTreeMap<String, Vec<TidyFile>> = BTreeMap::new();
    for (id, rel_dir, file_name, [star, hidden, notes, description, date, place, edit], people) in
        rows
    {
        let mut keeps: Vec<String> = Vec::new();
        let flags = [
            (star, t::STARRED),
            (hidden, t::HIDDEN),
            (notes, t::NOTES),
            (description, t::DESCRIPTION),
        ];
        keeps.extend(flags.iter().filter(|f| f.0).map(|f| f.1.to_owned()));
        if people > 0 {
            keeps.push(t::people(people));
        }
        let later = [(date, t::DATE), (place, t::PLACE), (edit, t::EDIT)];
        keeps.extend(later.iter().filter(|f| f.0).map(|f| f.1.to_owned()));
        folders.entry(rel_dir).or_default().push(TidyFile {
            id,
            file_name,
            keeps,
        });
    }
    let root = &album.location().root;
    let mut out: Vec<TidyFolder> = folders
        .into_iter()
        .map(|(folder, mut files)| {
            files.sort_by(|a, b| natural_cmp_text(&a.file_name, &b.file_name));
            let gone = !folder.is_empty()
                && matches!(
                    std::fs::metadata(absolute(root, &folder)),
                    Err(e) if e.kind() == ErrorKind::NotFound
                );
            TidyFolder {
                folder,
                gone,
                files,
            }
        })
        .collect();
    out.sort_by(|a, b| natural_cmp_text(&a.folder, &b.folder));
    Ok(out)
}

/// `album_tidy_remove(ids)`: deletes the confirmed rows in one
/// transaction (docs/05-organising.md, "Safety rules").
///
/// - Each id's file is looked for once more. If it is there, the row is
///   kept and marked `missing = 0`. Only a file that is surely not found
///   counts as gone; on any other error the row is left alone.
/// - An id whose row is not missing, or is outside the scope, is skipped,
///   whatever the page sent. The `DELETE` itself tests both.
/// - People links, tags, vectors and labels go with the rows (foreign
///   keys), the rows leave the `search` table, and names left on no photo
///   leave `people`.
/// - Undo steps whose photos were all deleted are dropped.
///
/// `view_only`, `scan_running` and `album_unreachable` as for [`list`].
pub fn remove(album: &Album, ids: Vec<i64>, scan_running: bool) -> AppResult<TidyRemoved> {
    check(album, scan_running, true)?;
    let mut ids = ids;
    ids.sort_unstable();
    ids.dedup();
    let scope = album.location().scope.clone();

    // The candidates: rows that are missing and in scope, with their paths.
    let candidates: Vec<(i64, String)> = {
        let ids = ids.clone();
        let scope = scope.clone();
        album.db().read(move |c| {
            let failed = sqlite_error(text::album::READING_DATABASE);
            let mut st = c
                .prepare_cached(&format!(
                    "SELECT rel_path FROM photos WHERE id = ?2 AND missing = 1 AND {IN_SCOPE}"
                ))
                .map_err(&failed)?;
            let mut out = Vec::new();
            for id in ids {
                let found: Option<String> = rusqlite::OptionalExtension::optional(
                    st.query_row(rusqlite::params![&scope, id], |r| r.get(0)),
                )
                .map_err(&failed)?;
                if let Some(path) = found {
                    out.push((id, path));
                }
            }
            Ok(out)
        })?
    };

    // Look for each file once more, outside the transaction.
    let root = album.location().root.clone();
    let mut delete = Vec::new();
    let mut back = Vec::new();
    for (id, rel_path) in candidates {
        match std::fs::symlink_metadata(absolute(&root, &rel_path)) {
            Ok(_) => back.push(id),
            Err(e) if e.kind() == ErrorKind::NotFound => delete.push(id),
            Err(e) => {
                tracing::warn!(target: "album", "tidy kept row {id}: its file could not be looked for ({e})");
            }
        }
    }
    if delete.is_empty() && back.is_empty() {
        return Ok(TidyRemoved {
            removed: 0,
            kept: 0,
            revision: album.revision(),
        });
    }

    let (removed, kept) = album.write_photos(move |tx, touched| {
        let failed = sqlite_error(t::REMOVING);
        let mut removed = Vec::new();
        {
            let mut del = tx
                .prepare_cached(&format!(
                    "DELETE FROM photos WHERE id = ?2 AND missing = 1 AND {IN_SCOPE}"
                ))
                .map_err(&failed)?;
            let mut search = tx
                .prepare_cached("DELETE FROM search WHERE rowid = ?1")
                .map_err(&failed)?;
            for id in delete {
                if del
                    .execute(rusqlite::params![&scope, id])
                    .map_err(&failed)?
                    == 1
                {
                    search.execute([id]).map_err(&failed)?;
                    removed.push(id);
                }
            }
        }
        let mut kept = Vec::new();
        {
            let mut found = tx
                .prepare_cached(&format!(
                    "UPDATE photos SET missing = 0 WHERE id = ?2 AND missing = 1 AND {IN_SCOPE}"
                ))
                .map_err(&failed)?;
            for id in back {
                if found
                    .execute(rusqlite::params![&scope, id])
                    .map_err(&failed)?
                    == 1
                {
                    kept.push(id);
                }
            }
        }
        if !removed.is_empty() {
            tx.execute(
                "DELETE FROM people WHERE id NOT IN (SELECT person_id FROM photo_people)",
                [],
            )
            .map_err(&failed)?;
        }
        touched.photos(removed.iter().copied());
        touched.photos(kept.iter().copied());
        Ok((removed, kept))
    })?;

    if !kept.is_empty() {
        // A row found again joins the index in no burst: work the bursts,
        // keepers and scores out again so it lands in its own.
        album.index().rebuild();
    }
    if !removed.is_empty() {
        album
            .undo()
            .drop_deleted(&removed.iter().copied().collect::<HashSet<i64>>());
    }
    let revision = if removed.is_empty() && kept.is_empty() {
        album.revision()
    } else {
        album.next_revision()
    };
    Ok(TidyRemoved {
        removed: u32::try_from(removed.len()).unwrap_or(u32::MAX),
        kept: u32::try_from(kept.len()).unwrap_or(u32::MAX),
        revision,
    })
}

#[cfg(test)]
mod tests;
