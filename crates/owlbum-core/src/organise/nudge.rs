//! Nudges: Up and Down change a photo's score by one
//! (docs/05-organising.md, "Nudges (Up and Down)").

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::album::undo::Step;
use crate::album::{Album, sqlite_error};
use crate::appdata::clock::now_ms;
use crate::error::{AppError, AppResult, ErrorCode};
use crate::text;

/// `score_adjust` stays within this distance of 0 (docs/02-data.md).
pub const LIMIT: i64 = 100_000;

/// `photo_nudge`'s answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Nudged {
    /// The photos whose `score_adjust` changed; empty when every target was
    /// at the limit.
    pub changed: Vec<i64>,
    /// The new score of the current photo (the first id sent); null while
    /// it is not analysed.
    pub total: Option<i32>,
    /// The album's revision after the change; unchanged when nothing
    /// changed.
    pub revision: u64,
}

/// The undo step one nudge makes: each changed photo's `score_adjust`
/// before (docs/05-organising.md, "Undo").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NudgeStep {
    /// `"score"`.
    pub label: &'static str,
    /// `(id, score_adjust before)` for each changed photo.
    pub before: Vec<(i64, i64)>,
}

fn not_found() -> AppError {
    AppError::new(ErrorCode::PhotoNotFound, text::photo::NOT_FOUND)
}

/// `photo_nudge(ids, delta)`: adds `delta` (1 or -1) to the `score_adjust`
/// of every photo in `ids`, in one transaction, and answers the photos
/// changed and the first photo's new score. A photo the nudge would take
/// past ±100,000 is left unchanged. The album index is rescored before this
/// answers. `view_only` in a view-only album; `photo_not_found` when an id
/// is not a photo the album shows; each writes nothing.
pub fn nudge(album: &Album, ids: Vec<i64>, delta: i64) -> AppResult<Nudged> {
    let (nudged, step) = nudge_step(album, ids, delta)?;
    if let Some(step) = step {
        album.undo().push(Step::Score(step));
    }
    Ok(nudged)
}

/// Writes back the `score_adjust` values `step` holds, in one transaction,
/// and answers the photos written. A photo whose row no longer exists or is
/// missing is left out. Records no undo step and does not raise the
/// revision; the caller does.
pub fn restore(album: &Album, step: &NudgeStep) -> AppResult<Vec<i64>> {
    let before = step.before.clone();
    album.write_photos(move |tx, touched| {
        let failed = sqlite_error(text::album::WRITING_DATABASE);
        let mut done = Vec::new();
        for (id, old) in before {
            let n = tx
                .execute(
                    "UPDATE photos SET score_adjust = ?2, info_updated_at = ?3
                      WHERE id = ?1 AND missing = 0",
                    params![id, old, now_ms()],
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

/// [`nudge`], also answering the undo step: None when nothing changed.
pub fn nudge_step(
    album: &Album,
    mut ids: Vec<i64>,
    delta: i64,
) -> AppResult<(Nudged, Option<NudgeStep>)> {
    if album.is_view_only() {
        return Err(AppError::new(ErrorCode::ViewOnly, text::photo::VIEW_ONLY));
    }
    if delta != 1 && delta != -1 {
        return Err(AppError::internal(text::score::NUDGING));
    }
    // A photo the album shows is a photo in the album index: not missing,
    // readable and inside the open scope.
    let shown = album.index().read(|v| {
        ids.iter()
            .all(|&id| u32::try_from(id).is_ok_and(|id| v.get(id).is_some()))
    });
    if !shown {
        return Err(not_found());
    }
    let mut seen = std::collections::HashSet::new();
    ids.retain(|id| seen.insert(*id));
    let current = ids.first().copied();

    let before = album.write_photos(move |tx, touched| {
        let failed = sqlite_error(text::album::WRITING_DATABASE);
        let mut before = Vec::new();
        for id in ids {
            let old: i64 = tx
                .query_row("SELECT score_adjust FROM photos WHERE id = ?1", [id], |r| {
                    r.get(0)
                })
                .optional()
                .map_err(&failed)?
                .ok_or_else(not_found)?;
            let new = old + delta;
            if new.abs() > LIMIT {
                continue;
            }
            tx.execute(
                "UPDATE photos SET score_adjust = ?2, info_updated_at = ?3 WHERE id = ?1",
                params![id, new, now_ms()],
            )
            .map_err(&failed)?;
            touched.photo(id);
            before.push((id, old));
        }
        Ok(before)
    })?;

    let total = current.and_then(|id| {
        let id = u32::try_from(id).ok()?;
        album
            .index()
            .read(|v| v.get(id).and_then(|p| p.derived.score))
    });
    let changed: Vec<i64> = before.iter().map(|(id, _)| *id).collect();
    if changed.is_empty() {
        let revision = album.revision();
        return Ok((
            Nudged {
                changed,
                total,
                revision,
            },
            None,
        ));
    }
    let revision = album.next_revision();
    Ok((
        Nudged {
            changed,
            total,
            revision,
        },
        Some(NudgeStep {
            label: "score",
            before,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::album::OpenOptions;
    use crate::info::typed::set_info;
    use crate::organise::score::photo_score;

    /// Adds an analysed photo and answers its id.
    fn photo(album: &Album, name: &str) -> i64 {
        let name = name.to_owned();
        album
            .write_photos(move |tx, touched| {
                tx.execute(
                    r#"INSERT INTO photos (rel_path, rel_dir, file_name, size, mtime_ms, file_key, thumb_key,
                                         kind, format, width, height, scan_version, analysis)
                     VALUES (?1, '', ?1, 1, 1, 'fk', 'tk', 'image', 'jpeg', 4000, 3000, 1,
                             '{"sharpness":10,"mean_luma":0.48,"clipped_shadows":0,"clipped_highlights":0,"saliency_x":0.5,"saliency_y":0.5}')"#,
                    [&name],
                )
                .map_err(sqlite_error(text::album::WRITING_DATABASE))?;
                let id = tx.last_insert_rowid();
                touched.photo(id);
                Ok(id)
            })
            .unwrap()
    }

    fn total(album: &Album, id: i64) -> i32 {
        photo_score(album, id).unwrap().total
    }

    #[test]
    fn up_and_down_move_the_score_by_one() {
        let tmp = tempfile::tempdir().unwrap();
        let album = Album::open(tmp.path(), &OpenOptions::default()).unwrap();
        album.index().rescore();
        let a = photo(&album, "a.jpg");
        album.index().rescore();
        let start = total(&album, a);

        let r = nudge(&album, vec![a], 1).unwrap();
        assert_eq!(r.changed, [a]);
        assert_eq!(r.total, Some(start + 1));
        assert_eq!(total(&album, a), start + 1);
        let line = photo_score(&album, a).unwrap().lines.pop().unwrap();
        assert_eq!((line.label.as_str(), line.points), ("Your nudges", 1));

        let r = nudge(&album, vec![a], -1).unwrap();
        assert_eq!(r.total, Some(start));
        let r = nudge(&album, vec![a], -1).unwrap();
        assert_eq!(r.total, Some(start - 1));
    }

    #[test]
    fn several_photos_are_one_step_and_the_first_gives_the_total() {
        let tmp = tempfile::tempdir().unwrap();
        let album = Album::open(tmp.path(), &OpenOptions::default()).unwrap();
        let a = photo(&album, "a.jpg");
        let b = photo(&album, "b.jpg");
        album.index().rescore();
        let rev = album.revision();
        let (r, step) = nudge_step(&album, vec![b, a, b], 1).unwrap();
        assert_eq!(r.changed, [b, a], "each photo once");
        assert_eq!(r.total, Some(total(&album, b)));
        assert!(r.revision > rev);
        assert_eq!(step.unwrap().before, [(b, 0), (a, 0)]);
    }

    #[test]
    fn a_nudge_past_the_limit_leaves_the_photo_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let album = Album::open(tmp.path(), &OpenOptions::default()).unwrap();
        let a = photo(&album, "a.jpg");
        let b = photo(&album, "b.jpg");
        album
            .write_photos(move |tx, touched| {
                tx.execute("UPDATE photos SET score_adjust = 100000 WHERE id = ?1", [a])
                    .unwrap();
                touched.photo(a);
                Ok(())
            })
            .unwrap();
        let rev = album.revision();
        let (r, step) = nudge_step(&album, vec![a], 1).unwrap();
        assert!(r.changed.is_empty());
        assert!(step.is_none());
        assert_eq!(r.revision, rev, "nothing changed");
        let r = nudge(&album, vec![a, b], 1).unwrap();
        assert_eq!(r.changed, [b]);
        assert_eq!(nudge(&album, vec![a], -1).unwrap().changed, [a]);
    }

    #[test]
    fn a_photo_the_album_does_not_show_or_a_bad_delta_writes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let album = Album::open(tmp.path(), &OpenOptions::default()).unwrap();
        let a = photo(&album, "a.jpg");
        let e = nudge(&album, vec![a, 999], 1).unwrap_err();
        assert_eq!(e.code, ErrorCode::PhotoNotFound);
        assert_eq!(
            nudge(&album, vec![a], 2).unwrap_err().code,
            ErrorCode::Internal
        );
        assert_eq!(
            nudge(&album, vec![a], 1).unwrap().changed,
            [a],
            "a untouched until now"
        );
    }

    #[test]
    fn star_and_hide_score_fifty_either_way() {
        let tmp = tempfile::tempdir().unwrap();
        let album = Album::open(tmp.path(), &OpenOptions::default()).unwrap();
        let a = photo(&album, "a.jpg");
        album.index().rescore();
        let start = total(&album, a);
        set_info(
            &album,
            vec![a],
            serde_json::from_value(json!({ "star": true })).unwrap(),
        )
        .unwrap();
        assert_eq!(total(&album, a), start + 50);
        set_info(
            &album,
            vec![a],
            serde_json::from_value(json!({ "hidden": true })).unwrap(),
        )
        .unwrap();
        assert_eq!(total(&album, a), start);
        let tile = album
            .index()
            .read(|v| v.get(a as u32).unwrap().derived.score);
        assert_eq!(tile, Some(start), "the tile's score follows at once");
    }
}
