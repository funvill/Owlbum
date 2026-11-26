//! Where the current photo and the selection land when a result changes
//! (docs/05-organising.md, "Where the selection goes afterwards";
//! docs/04-browsing.md, "When the result changes").
//!
//! 1. The current photo, if it is still in the result, with its new index.
//! 2. Else the replacement stack rules name. Stacks are not built yet
//!    (#30), so this rule never applies here.
//! 3. Else the tile now in its place: `k` is the number of tiles before it
//!    in the old result that are still present (the old index when the old
//!    result has been dropped); the tile at `k`, or the last one when `k`
//!    is past the end.
//! 4. An empty result has no current photo.
//!
//! The selection is the old selection limited to photos still shown, plus
//! the new current photo. The anchor stays when it is still shown; else it
//! becomes the current photo.

use std::collections::{HashMap, HashSet};

use super::shape::{Keep, Kept, PhotoAt};

/// Settles `keep` against `new`, the new result's tiles. `old` is the
/// tiles of the result `keep.token` named, when it is still held.
pub fn settle(keep: &Keep, old: Option<&[u32]>, new: &[u32]) -> Kept {
    if new.is_empty() {
        return Kept::default();
    }
    let place: HashMap<u32, u32> = new
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, index(i)))
        .collect();
    let find = |id: i64| -> Option<PhotoAt> {
        let tile = u32::try_from(id).ok()?;
        place.get(&tile).map(|&index| PhotoAt { id, index })
    };
    let current = keep
        .current
        .map(|c| find(c.id).unwrap_or_else(|| in_its_place(c, old, new, &place)));
    let anchor = keep.anchor_id.and_then(find).or(current);

    let mut seen = HashSet::new();
    let mut selection: Vec<i64> = keep
        .selection
        .iter()
        .copied()
        .filter(|&id| find(id).is_some() && seen.insert(id))
        .collect();
    if let Some(c) = current
        && seen.insert(c.id)
    {
        selection.push(c.id);
    }
    let view = keep
        .view
        .map(|v| find(v.id).unwrap_or_else(|| in_its_place(v, old, new, &place)));
    Kept {
        current,
        anchor,
        selection,
        view,
    }
}

/// Rules 3 and 4 for an old current photo that left the result.
fn in_its_place(
    gone: PhotoAt,
    old: Option<&[u32]>,
    new: &[u32],
    place: &HashMap<u32, u32>,
) -> PhotoAt {
    let k = match old {
        Some(old) => {
            let at = usize::try_from(gone.index)
                .ok()
                .filter(|&i| old.get(i).is_some_and(|&t| i64::from(t) == gone.id))
                .or_else(|| old.iter().position(|&t| i64::from(t) == gone.id))
                .unwrap_or_else(|| (gone.index as usize).min(old.len()));
            old[..at].iter().filter(|t| place.contains_key(t)).count()
        }
        None => gone.index as usize,
    };
    let at = k.min(new.len() - 1);
    PhotoAt {
        id: i64::from(new[at]),
        index: index(at),
    }
}

fn index(i: usize) -> u32 {
    u32::try_from(i).unwrap_or(u32::MAX)
}
