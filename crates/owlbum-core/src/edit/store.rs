//! Stored edits: reading `photos.edit`, `edit_set`, `edit_turn`,
//! `edit_revert` and their undo steps (docs/06-editing-and-file-actions.md,
//! "Ending edit mode", "Rotate and flip outside edit mode", "Revert to
//! original"; docs/05-organising.md, "Undo").

use std::collections::HashSet;
use std::sync::{Mutex, PoisonError};

use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::model::{IDENTITY, PhotoEdit, TurnOp, parse_stored, same_edit, stored_value};
use super::plan::{EditPlan, edit_plan};
use crate::album::undo::Step;
use crate::album::{Album, sqlite_error};
use crate::error::{AppError, AppResult, ErrorCode};
use crate::text;

/// The undo step of one edit command: each changed photo's `photos.edit`
/// before, as stored (None for NULL).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditStep {
    /// "edit", "rotate left", "rotate right", "flip" or "revert to original".
    pub label: &'static str,
    pub before: Vec<(i64, Option<String>)>,
}

/// `edit_set`'s answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct EditSet {
    /// False when the edit equals the stored one and nothing was written.
    pub changed: bool,
    /// The plan for the photo's stored edit now; null for none.
    pub plan: Option<EditPlan>,
    /// The album's revision after the change.
    pub revision: u64,
}

/// `edit_turn`'s answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Turned {
    /// The photos turned or flipped.
    pub changed: Vec<i64>,
    /// How many targets cannot be edited (videos, animated pictures).
    pub skipped: u32,
    /// The album's revision after the change.
    pub revision: u64,
}

/// `edit_revert`'s answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Reverted {
    /// The photos whose edit was removed.
    pub changed: Vec<i64>,
    pub revision: u64,
}

/// Photos whose stored edit was invalid and already logged.
static LOGGED: Mutex<Option<HashSet<i64>>> = Mutex::new(None);

/// The stored edit of photo `id` from its `photos.edit` text. An invalid
/// edit counts as none and is logged once per photo.
pub fn read_stored(id: i64, text: Option<&str>) -> Option<PhotoEdit> {
    let text = text?;
    let edit = parse_stored(text);
    if edit.is_none() {
        let mut logged = LOGGED.lock().unwrap_or_else(PoisonError::into_inner);
        if logged.get_or_insert_with(HashSet::new).insert(id) {
            tracing::warn!(target: "edit", "photo {id} has an edit that cannot be read; it is not shown");
        }
    }
    edit
}

/// The plan of the stored edit of a photo of upright size `w × h`; None
/// for no edit, an invalid one, or a size not known yet.
pub fn stored_plan(id: i64, w: u32, h: u32, text: Option<&str>) -> Option<EditPlan> {
    if w == 0 || h == 0 {
        return None;
    }
    read_stored(id, text).map(|e| edit_plan(w, h, &e, false))
}

/// Whether a photo of `kind` with `duration_ms` can be edited; the reason
/// when it cannot.
pub fn not_editable(kind: &str, duration_ms: Option<i64>) -> Option<&'static str> {
    if kind == "video" {
        Some(text::edit::VIDEO)
    } else if duration_ms.is_some() {
        Some(text::edit::ANIMATED)
    } else {
        None
    }
}

fn not_found() -> AppError {
    AppError::new(ErrorCode::PhotoNotFound, text::photo::NOT_FOUND)
}

fn check_writable(album: &Album) -> AppResult<()> {
    if album.is_view_only() {
        return Err(AppError::new(ErrorCode::ViewOnly, text::edit::VIEW_ONLY));
    }
    Ok(())
}

/// Every id is a photo the album shows (in the album index), each once.
fn shown(album: &Album, ids: Vec<i64>) -> AppResult<Vec<i64>> {
    let ok = album.index().read(|v| {
        ids.iter()
            .all(|&id| u32::try_from(id).is_ok_and(|id| v.get(id).is_some()))
    });
    if !ok {
        return Err(not_found());
    }
    let mut seen = HashSet::new();
    Ok(ids.into_iter().filter(|id| seen.insert(*id)).collect())
}

/// What one photo's row holds for editing.
struct Target {
    kind: String,
    duration_ms: Option<i64>,
    width: u32,
    height: u32,
    edit: Option<String>,
}

fn target(tx: &Transaction<'_>, id: i64) -> AppResult<Target> {
    tx.query_row(
        "SELECT kind, duration_ms, width, height, edit FROM photos WHERE id = ?1 AND missing = 0",
        [id],
        |r| {
            Ok(Target {
                kind: r.get(0)?,
                duration_ms: r.get(1)?,
                width: u32::try_from(r.get::<_, i64>(2)?).unwrap_or(0),
                height: u32::try_from(r.get::<_, i64>(3)?).unwrap_or(0),
                edit: r.get(4)?,
            })
        },
    )
    .optional()
    .map_err(sqlite_error(text::album::WRITING_DATABASE))?
    .ok_or_else(not_found)
}

fn write_edit(tx: &Transaction<'_>, id: i64, value: Option<&str>) -> AppResult<()> {
    tx.execute(
        "UPDATE photos SET edit = ?2 WHERE id = ?1",
        params![id, value],
    )
    .map_err(sqlite_error(text::album::WRITING_DATABASE))?;
    Ok(())
}

/// `edit_set(photo_id, edit)`: stores `edit` (None or identity stores
/// NULL) and records one undo step labelled "edit" holding the edit as it
/// was. When the edit is the same as the stored one nothing is written.
pub fn set(album: &Album, id: i64, edit: Option<PhotoEdit>) -> AppResult<EditSet> {
    check_writable(album)?;
    shown(album, vec![id])?;
    let edit = edit.unwrap_or(IDENTITY);
    let (changed, plan, before) = album.write_photos(move |tx, touched| {
        let t = target(tx, id)?;
        if let Some(reason) = not_editable(&t.kind, t.duration_ms) {
            return Err(AppError::new(ErrorCode::EditNotEditable, reason));
        }
        let old = read_stored(id, t.edit.as_deref()).unwrap_or(IDENTITY);
        let plan = |e: &PhotoEdit| {
            stored_value(e).map(|_| edit_plan(t.width.max(1), t.height.max(1), e, false))
        };
        // An unreadable stored edit counts as none, but is still replaced.
        if same_edit(&old, &edit) && (t.edit.is_none() || stored_value(&old).is_some()) {
            return Ok((false, plan(&old), None));
        }
        let value = stored_value(&edit);
        write_edit(tx, id, value.as_deref())?;
        touched.photo(id);
        Ok((true, plan(&edit), Some(t.edit)))
    })?;
    let revision = if changed {
        album.undo().push(Step::Edit(EditStep {
            label: "edit",
            before: vec![(id, before.flatten())],
        }));
        album.next_revision()
    } else {
        album.revision()
    };
    Ok(EditSet {
        changed,
        plan,
        revision,
    })
}

/// `edit_turn(photo_ids, op)`: applies `op` to each photo's stored edit
/// (or identity) in one transaction, as one undo step. Photos that cannot
/// be edited are skipped and counted.
pub fn turn(album: &Album, ids: Vec<i64>, op: TurnOp) -> AppResult<Turned> {
    check_writable(album)?;
    let ids = shown(album, ids)?;
    let (before, skipped) = album.write_photos(move |tx, touched| {
        let mut before = Vec::new();
        let mut skipped = 0u32;
        for id in ids {
            let t = target(tx, id)?;
            if not_editable(&t.kind, t.duration_ms).is_some() {
                skipped += 1;
                continue;
            }
            let old = read_stored(id, t.edit.as_deref()).unwrap_or(IDENTITY);
            let value = stored_value(&op.apply(&old));
            write_edit(tx, id, value.as_deref())?;
            touched.photo(id);
            before.push((id, t.edit));
        }
        Ok((before, skipped))
    })?;
    let changed: Vec<i64> = before.iter().map(|(id, _)| *id).collect();
    let revision = if changed.is_empty() {
        album.revision()
    } else {
        album.undo().push(Step::Edit(EditStep {
            label: op.label(),
            before,
        }));
        album.next_revision()
    };
    Ok(Turned {
        changed,
        skipped,
        revision,
    })
}

/// `edit_revert(photo_ids)`: removes the stored edit of each photo that
/// has one, in one transaction, as one undo step labelled "revert to
/// original".
pub fn revert(album: &Album, ids: Vec<i64>) -> AppResult<Reverted> {
    check_writable(album)?;
    let ids = shown(album, ids)?;
    let before = album.write_photos(move |tx, touched| {
        let mut before = Vec::new();
        for id in ids {
            let t = target(tx, id)?;
            if t.edit.is_none() {
                continue;
            }
            write_edit(tx, id, None)?;
            touched.photo(id);
            before.push((id, t.edit));
        }
        Ok(before)
    })?;
    let changed: Vec<i64> = before.iter().map(|(id, _)| *id).collect();
    let revision = if changed.is_empty() {
        album.revision()
    } else {
        album.undo().push(Step::Edit(EditStep {
            label: "revert to original",
            before,
        }));
        album.next_revision()
    };
    Ok(Reverted { changed, revision })
}

/// Writes back the edits `step` holds, in one transaction, and answers the
/// photos written. A photo whose row no longer exists or is missing is
/// left out. Records no undo step and does not raise the revision.
pub fn restore(album: &Album, step: &EditStep) -> AppResult<Vec<i64>> {
    let before = step.before.clone();
    album.write_photos(move |tx, touched| {
        let failed = sqlite_error(text::album::WRITING_DATABASE);
        let mut done = Vec::new();
        for (id, old) in before {
            let n = tx
                .execute(
                    "UPDATE photos SET edit = ?2 WHERE id = ?1 AND missing = 0",
                    params![id, old],
                )
                .map_err(&failed)?;
            if n > 0 {
                touched.photo(id);
                done.push(id);
            }
        }
        Ok(done)
    })
}

/// The upright size of photo `id`, for `edit_plan`.
pub fn photo_size(album: &Album, id: i64) -> AppResult<(u32, u32)> {
    shown(album, vec![id])?;
    album.db().read(|conn| {
        conn.query_row(
            "SELECT width, height FROM photos WHERE id = ?1",
            [id],
            |r| {
                Ok((
                    u32::try_from(r.get::<_, i64>(0)?).unwrap_or(0),
                    u32::try_from(r.get::<_, i64>(1)?).unwrap_or(0),
                ))
            },
        )
        .optional()
        .map_err(sqlite_error(text::edit::READING))?
        .ok_or_else(not_found)
    })
}

#[cfg(test)]
mod tests;
