//! Search by meaning (docs/07-tagging.md, "Search by meaning"): the rules,
//! the vector cache and the scan that scores it.
//!
//! The cache holds, for the open album and one keywords model, every
//! stored vector in one block of `count × dim` numbers beside a list of
//! photo ids. It is filled by the first search, on the blocking pool, and
//! then kept in step without reading the table again: every write that can
//! touch a vector marks its photos stale ([`VectorCache::stale`]), and the
//! next search reads only those rows again before it scans. A photo that
//! left the album keeps its slot until then; the query's own checks drop
//! it, since it is no longer in the album index.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, RwLock};
use std::time::Duration;

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::results::vector_from_bytes;
use crate::album::{Album, sqlite_error};
use crate::error::AppResult;
use crate::text;

/// The fewest Unicode letters the free text needs.
pub const MIN_LETTERS: usize = 3;
/// The most photos one search adds.
pub const MOST: usize = 200;
/// How long turning the search text into a vector may take, the model's
/// load included.
pub const TEXT_LIMIT: Duration = Duration::from_secs(15);

/// A slot whose photo's vector was deleted.
const GONE: u32 = u32::MAX;

/// How many Unicode letters `text` holds: `se` has 2, `  a 1 2 b  ` has 2.
pub fn letters(text: &str) -> usize {
    text.chars().filter(|c| c.is_alphabetic()).count()
}

/// How a search by meaning went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum MeaningState {
    /// It ran; `meaning_start` says whether anything was added.
    Ok,
    /// The free text has fewer than 3 letters.
    TooShort,
    /// No keywords model is active.
    Off,
    /// The text could not be turned into a vector (the model would not
    /// load, failed, or took longer than 15 s).
    Unavailable,
}

/// `search_meaning`'s reply. `count` and `photos` are there only when
/// `state` is `ok`; `meaning_start` is null when nothing was added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MeaningResult {
    pub state: MeaningState,
    /// The new length of the token's list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub count: Option<u32>,
    /// The new X of "Showing X of Y": photos, stacks unfolded, meaning
    /// matches included.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub photos: Option<u32>,
    /// The index of the first photo found by meaning; null when none was
    /// added.
    pub meaning_start: Option<u32>,
}

impl MeaningResult {
    /// A reply that adds nothing and gives no counts.
    pub fn nothing(state: MeaningState) -> Self {
        Self {
            state,
            count: None,
            photos: None,
            meaning_start: None,
        }
    }
}

/// The vectors of one model, in one block.
#[derive(Debug, Default)]
struct Block {
    model: String,
    dim: usize,
    /// Photo id per slot; [`GONE`] for a deleted vector.
    ids: Vec<u32>,
    /// `ids.len() × dim` numbers.
    data: Vec<f32>,
    /// Photo id to slot.
    slot: HashMap<u32, usize>,
}

impl Block {
    fn put(&mut self, id: u32, vector: Option<Vec<f32>>) {
        match (self.slot.get(&id).copied(), vector) {
            (Some(s), Some(v)) => {
                self.data[s * self.dim..(s + 1) * self.dim].copy_from_slice(&v);
                self.ids[s] = id;
            }
            (None, Some(v)) => {
                self.slot.insert(id, self.ids.len());
                self.ids.push(id);
                self.data.extend_from_slice(&v);
            }
            (Some(s), None) => {
                self.slot.remove(&id);
                self.ids[s] = GONE;
            }
            (None, None) => {}
        }
    }
}

#[derive(Debug, Default)]
struct Stale {
    /// Whether a block is held or being filled; until then nothing needs
    /// to be kept in step.
    tracking: bool,
    ids: HashSet<u32>,
}

/// The vector cache of one open album (docs/07-tagging.md, "Performance
/// for 100,000 vectors"). It dies with the album.
#[derive(Debug, Default)]
pub struct VectorCache {
    block: RwLock<Option<Block>>,
    stale: Mutex<Stale>,
}

impl VectorCache {
    /// Photos whose vectors may have been written or deleted. Called after
    /// every write that can touch the `vectors` table.
    pub fn stale(&self, ids: impl IntoIterator<Item = i64>) {
        let mut s = self.stale.lock().unwrap_or_else(|p| p.into_inner());
        if !s.tracking {
            return;
        }
        s.ids
            .extend(ids.into_iter().filter_map(|id| u32::try_from(id).ok()));
    }

    /// Drops the block (the keywords model changed).
    pub fn clear(&self) {
        let mut s = self.stale.lock().unwrap_or_else(|p| p.into_inner());
        *self.block.write().unwrap_or_else(|p| p.into_inner()) = None;
        s.tracking = false;
        s.ids.clear();
    }

    /// Every photo of `model` whose vector scores at least `threshold`
    /// against the unit vector `text`, best first, then by id. Fills the
    /// cache for `model` first if it holds another model or none.
    pub fn scores(
        &self,
        album: &Album,
        model: &str,
        text: &[f32],
        threshold: f32,
    ) -> AppResult<Vec<(u32, f32)>> {
        let dim = text.len();
        let filled = self
            .block
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .is_some_and(|b| b.model == model && b.dim == dim);
        if !filled {
            self.fill(album, model, dim)?;
        }
        self.follow(album, model, dim)?;
        let block = self.block.read().unwrap_or_else(|p| p.into_inner());
        let Some(b) = block.as_ref() else {
            return Ok(Vec::new());
        };
        let mut hits: Vec<(u32, f32)> = b
            .ids
            .iter()
            .zip(b.data.chunks_exact(dim))
            .filter(|&(&id, _)| id != GONE)
            .filter_map(|(&id, v)| {
                let s = dot(text, v);
                (s >= threshold).then_some((id, s))
            })
            .collect();
        hits.sort_unstable_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        Ok(hits)
    }

    /// Reads every vector of `model` with `dim` numbers.
    fn fill(&self, album: &Album, model: &str, dim: usize) -> AppResult<()> {
        {
            // Writes from now on are kept in step: anything committed after
            // the read below starts is read again by `follow`.
            let mut s = self.stale.lock().unwrap_or_else(|p| p.into_inner());
            s.tracking = true;
            s.ids.clear();
        }
        let mut b = Block {
            model: model.to_owned(),
            dim,
            ..Block::default()
        };
        let mut skipped = 0usize;
        album.db().read(|conn| {
            let failed = sqlite_error(text::query::READING_ROWS);
            let mut stmt = conn
                .prepare("SELECT photo_id, dim, vector FROM vectors WHERE model_id = ?1")
                .map_err(&failed)?;
            let mut rows = stmt.query([model]).map_err(&failed)?;
            while let Some(r) = rows.next().map_err(&failed)? {
                let id: i64 = r.get(0).map_err(&failed)?;
                let d: i64 = r.get(1).map_err(&failed)?;
                let bytes = r.get_ref(2).map_err(&failed)?.as_blob().unwrap_or(&[]);
                match (u32::try_from(id), usable(d, bytes, dim)) {
                    (Ok(id), Some(v)) if id != GONE => b.put(id, Some(v)),
                    _ => skipped += 1,
                }
            }
            Ok(())
        })?;
        if skipped > 0 {
            tracing::info!(target: "tagging", "search by meaning skipped {skipped} vectors of {model} that are not {dim} numbers");
        }
        tracing::debug!(target: "tagging", "the vector cache holds {} vectors of {model}", b.ids.len());
        *self.block.write().unwrap_or_else(|p| p.into_inner()) = Some(b);
        Ok(())
    }

    /// Reads the rows of the photos marked stale again.
    fn follow(&self, album: &Album, model: &str, dim: usize) -> AppResult<()> {
        let ids: Vec<u32> = {
            let mut s = self.stale.lock().unwrap_or_else(|p| p.into_inner());
            s.ids.drain().collect()
        };
        if ids.is_empty() {
            return Ok(());
        }
        let rows = album.db().read(|conn| {
            let failed = sqlite_error(text::query::READING_ROWS);
            let mut stmt = conn
                .prepare_cached(
                    "SELECT dim, vector FROM vectors WHERE photo_id = ?1 AND model_id = ?2",
                )
                .map_err(&failed)?;
            ids.iter()
                .map(|&id| {
                    let row = stmt
                        .query_row(params![i64::from(id), model], |r| {
                            Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?))
                        })
                        .optional()
                        .map_err(&failed)?;
                    Ok((id, row.and_then(|(d, bytes)| usable(d, &bytes, dim))))
                })
                .collect::<AppResult<Vec<_>>>()
        })?;
        let mut block = self.block.write().unwrap_or_else(|p| p.into_inner());
        if let Some(b) = block.as_mut().filter(|b| b.model == model && b.dim == dim) {
            for (id, v) in rows {
                b.put(id, v);
            }
        }
        Ok(())
    }
}

/// The vector in `bytes` when its `dim` is the one searched and it is
/// whole and finite.
fn usable(d: i64, bytes: &[u8], dim: usize) -> Option<Vec<f32>> {
    if usize::try_from(d).ok()? != dim {
        return None;
    }
    vector_from_bytes(bytes, dim).filter(|v| v.iter().all(|x| x.is_finite()))
}

/// The dot product, in eight running sums so the compiler can use the
/// processor's vector instructions.
pub fn dot(a: &[f32], b: &[f32]) -> f32 {
    let mut sums = [0f32; 8];
    let (ac, ar) = a.as_chunks::<8>();
    let (bc, br) = b.as_chunks::<8>();
    for (x, y) in ac.iter().zip(bc) {
        for k in 0..8 {
            sums[k] += x[k] * y[k];
        }
    }
    let tail: f32 = ar.iter().zip(br).map(|(x, y)| x * y).sum();
    sums.iter().sum::<f32>() + tail
}

/// `v` made unit length; None when it has no length or is not finite.
pub fn unit(mut v: Vec<f32>) -> Option<Vec<f32>> {
    let len = dot(&v, &v).sqrt();
    if !len.is_finite() || len == 0.0 {
        return None;
    }
    for x in &mut v {
        *x /= len;
    }
    Some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_count_unicode_letters_only() {
        assert_eq!(letters("se"), 2);
        assert_eq!(letters("  a 1 2 b  "), 2);
        assert_eq!(letters("sea"), 3);
        assert_eq!(letters("été"), 3);
    }

    #[test]
    fn dot_matches_a_plain_sum_at_every_length() {
        for n in [0, 1, 7, 8, 9, 512, 515] {
            let a: Vec<f32> = (0..n).map(|i| (i as f32 * 0.37).sin()).collect();
            let b: Vec<f32> = (0..n).map(|i| (i as f32 * 0.11).cos()).collect();
            let plain: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
            assert!((dot(&a, &b) - plain).abs() < 1e-3, "{n}");
        }
    }

    #[test]
    fn unit_refuses_a_zero_vector() {
        assert!(unit(vec![0.0; 4]).is_none());
        let v = unit(vec![3.0, 4.0]).unwrap();
        assert!((v[0] - 0.6).abs() < 1e-6 && (v[1] - 0.8).abs() < 1e-6);
    }

    #[test]
    fn a_block_overwrites_and_forgets_slots() {
        let mut b = Block {
            dim: 2,
            ..Block::default()
        };
        b.put(5, Some(vec![1.0, 0.0]));
        b.put(6, Some(vec![0.0, 1.0]));
        b.put(5, Some(vec![0.5, 0.5]));
        assert_eq!(b.ids, [5, 6]);
        assert_eq!(&b.data[..2], &[0.5, 0.5]);
        b.put(6, None);
        assert_eq!(b.ids, [5, GONE]);
        b.put(6, Some(vec![1.0, 1.0]));
        assert_eq!(b.ids, [5, GONE, 6]);
    }
}
