//! Bursts and their keepers (docs/05-organising.md, "Bursts").
//!
//! A burst is a connected group of near-identical shots: two photos pair
//! when both have a dHash, are close enough in time and colour, and their
//! dHashes are a few bits apart. [`find`] works the groups out over the
//! whole album index without comparing every pair; [`keeper_order`] picks
//! each burst's keeper. The album index runs both (see
//! [`crate::index::AlbumIndex::rebuild`]) and keeps the result in
//! [`crate::index::Derived`].

use std::cmp::Ordering;
use std::collections::HashMap;
use std::num::NonZeroU32;

use super::facts;
use crate::index::Photo;

/// The most minutes two dated photos of a pair may be apart.
pub const MAX_MINUTES: i64 = 120;

/// The most dHash bits two dated photos of a pair may differ by.
pub const DATED_BITS: u32 = 12;

/// The most dHash bits a pair may differ by when either photo has no date.
pub const UNDATED_BITS: u32 = 6;

/// The farthest apart two average colours of a pair may be (Euclidean, in
/// RGB 0 to 255).
pub const MAX_COLOUR: u32 = 70;

/// How many following photos, in date order, each dated photo is compared
/// with at most.
pub const LOOKAHEAD: usize = 2000;

/// The dHash is cut into this many segments for the undated pairs: two
/// hashes at most [`UNDATED_BITS`] apart share at least one segment.
const SEGMENTS: u32 = UNDATED_BITS + 1;

/// True when `a` and `b` pair (docs/05, the table under "Bursts").
pub fn pairs(a: &Photo, b: &Photo) -> bool {
    let (Some(aa), Some(ba)) = (a.analysis(), b.analysis()) else {
        return false;
    };
    let (Some(ha), Some(hb)) = (aa.dhash(), ba.dhash()) else {
        return false;
    };
    let dated = a.date().is_some() && b.date().is_some();
    if dated && a.date.abs_diff(b.date) > (MAX_MINUTES * 60).unsigned_abs() {
        return false;
    }
    if let (Some(ca), Some(cb)) = (aa.avg_colour(), ba.avg_colour()) {
        let d2: u32 = (0..3)
            .map(|i| u32::from(ca[i].abs_diff(cb[i])).pow(2))
            .sum();
        if d2 > MAX_COLOUR * MAX_COLOUR {
            return false;
        }
    }
    let limit = if dated { DATED_BITS } else { UNDATED_BITS };
    (ha ^ hb).count_ones() <= limit
}

/// The burst of every photo in `photos`, in the same order: the burst's id,
/// or None for a photo in no burst.
///
/// Pairs of two dated photos are found by comparing each photo with the
/// ones that follow it in date order (then name order) within
/// [`MAX_MINUTES`], to a limit of [`LOOKAHEAD`]. Pairs with an undated
/// photo are found through an index of the hash segments. A burst's id is
/// the id of its member that sorts first by date (undated last), then by
/// `name_rank`.
pub fn find(photos: &[Photo]) -> Vec<Option<NonZeroU32>> {
    let mut sets = Sets::new(photos.len());
    let hashed: Vec<usize> = (0..photos.len())
        .filter(|&i| dhash(&photos[i]).is_some())
        .collect();

    // Both dated: a sliding window over the date order. The window reads
    // a compact copy of each photo's date and hash, in that order, so its
    // many cheap tests never touch the records themselves (at 100,000
    // photos they took 0.35 to 0.5 s when they did, issue 162).
    let mut dated: Vec<Dated> = hashed
        .iter()
        .filter_map(|&i| {
            let p = &photos[i];
            p.date().is_some().then(|| Dated {
                date: p.date,
                rank: p.name_rank,
                hash: dhash(p).unwrap_or_default(),
                at: i,
            })
        })
        .collect();
    dated.sort_unstable_by_key(|d| (d.date, d.rank));
    for (at, a) in dated.iter().enumerate() {
        for b in dated[at + 1..].iter().take(LOOKAHEAD) {
            if b.date - a.date > MAX_MINUTES * 60 {
                break;
            }
            if (a.hash ^ b.hash).count_ones() <= DATED_BITS && pairs(&photos[a.at], &photos[b.at]) {
                sets.union(a.at, b.at);
            }
        }
    }

    // Either undated: only photos that share a hash segment can pair.
    // Only the undated photos are indexed (a few percent of an album);
    // every hashed photo then looks up its segments among them.
    let mut undated: Vec<(u64, usize)> = hashed
        .iter()
        .filter(|&&i| photos[i].date().is_none())
        .flat_map(|&i| {
            let h = dhash(&photos[i]).unwrap_or_default();
            (0..SEGMENTS).map(move |s| (segment_key(h, s), i))
        })
        .collect();
    if !undated.is_empty() {
        undated.sort_unstable();
        for &i in &hashed {
            let h = dhash(&photos[i]).unwrap_or_default();
            for s in 0..SEGMENTS {
                let key = segment_key(h, s);
                let from = undated.partition_point(|&(k, _)| k < key);
                for &(_, j) in undated[from..].iter().take_while(|&&(k, _)| k == key) {
                    if j != i
                        && close(&photos[i], &photos[j], UNDATED_BITS)
                        && pairs(&photos[i], &photos[j])
                    {
                        sets.union(i, j);
                    }
                }
            }
        }
    }

    // Each group of two or more is a burst, named after its first member.
    let mut first: HashMap<usize, (usize, usize)> = HashMap::new();
    for &i in &hashed {
        let root = sets.find(i);
        let entry = first.entry(root).or_insert((i, 0));
        entry.1 += 1;
        let key = |k: usize| (photos[k].date, photos[k].name_rank);
        if key(i) < key(entry.0) {
            entry.0 = i;
        }
    }
    let mut out = vec![None; photos.len()];
    for &i in &hashed {
        let (lead, size) = first[&sets.find(i)];
        if size > 1 {
            out[i] = NonZeroU32::new(photos[lead].id);
        }
    }
    out
}

/// What the dated window reads of one photo.
struct Dated {
    date: i64,
    rank: u32,
    hash: u64,
    /// The photo's place in the slice given to [`find`].
    at: usize,
}

/// The keeper order (docs/05, "The keeper"): the member that sorts first is
/// the keeper. Reads [`crate::index::Derived::quality`], never the score.
pub fn keeper_order(a: &Photo, b: &Photo) -> Ordering {
    a.hidden()
        .cmp(&b.hidden())
        .then_with(|| b.star().cmp(&a.star()))
        .then_with(|| b.derived.quality.total_cmp(&a.derived.quality))
        .then_with(|| a.date.cmp(&b.date))
        .then_with(|| a.name_rank.cmp(&b.name_rank))
}

/// Sets [`crate::index::Derived::keeper`] for the members of the bursts
/// `only` names (every burst when None; sorted, so a bulk star of many
/// bursts stays fast), from the photos' flags as they are now. When `only`
/// is None, a photo in no burst is set as its own keeper.
pub fn pick_keepers(photos: &mut [Photo], only: Option<&[NonZeroU32]>) {
    let wanted = |b: NonZeroU32| only.is_none_or(|list| list.binary_search(&b).is_ok());
    let mut best: HashMap<NonZeroU32, usize> = HashMap::new();
    for (i, p) in photos.iter().enumerate() {
        let Some(b) = p.derived.burst.filter(|&b| wanted(b)) else {
            continue;
        };
        best.entry(b)
            .and_modify(|k| {
                if keeper_order(p, &photos[*k]) == Ordering::Less {
                    *k = i;
                }
            })
            .or_insert(i);
    }
    for (i, p) in photos.iter_mut().enumerate() {
        if let Some(b) = p.derived.burst.filter(|&b| wanted(b)) {
            p.derived.keeper = best.get(&b) == Some(&i);
        } else if p.derived.burst.is_none() && only.is_none() {
            p.derived.keeper = true;
        }
    }
}

/// `quality` as the keeper order reads it ([`facts::quality`]), 0 for a
/// photo that is not analysed.
pub fn quality(p: &Photo, sharpness: &[f32]) -> f32 {
    facts::quality(p, sharpness).unwrap_or(0.0) as f32
}

fn dhash(p: &Photo) -> Option<u64> {
    p.analysis().and_then(|a| a.dhash())
}

/// The cheap first test before [`pairs`]: the hashes are at most `bits`
/// apart.
fn close(a: &Photo, b: &Photo, bits: u32) -> bool {
    match (dhash(a), dhash(b)) {
        (Some(x), Some(y)) => (x ^ y).count_ones() <= bits,
        _ => false,
    }
}

/// Segment `s` of the 64 bits cut into [`SEGMENTS`] nearly equal parts
/// (at most 10 bits each), with `s` above it so segments never mix.
fn segment_key(h: u64, s: u32) -> u64 {
    let start = 64 * s / SEGMENTS;
    let end = 64 * (s + 1) / SEGMENTS;
    (u64::from(s) << 32) | ((h >> start) & ((1_u64 << (end - start)) - 1))
}

/// Union-find with path halving and union by size.
struct Sets {
    parent: Vec<usize>,
    size: Vec<u32>,
}

impl Sets {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            size: vec![1; n],
        }
    }

    fn find(&mut self, mut i: usize) -> usize {
        while self.parent[i] != i {
            self.parent[i] = self.parent[self.parent[i]];
            i = self.parent[i];
        }
        i
    }

    fn union(&mut self, a: usize, b: usize) {
        let (mut a, mut b) = (self.find(a), self.find(b));
        if a == b {
            return;
        }
        if self.size[a] < self.size[b] {
            std::mem::swap(&mut a, &mut b);
        }
        self.parent[b] = a;
        self.size[a] += self.size[b];
    }
}

#[cfg(test)]
mod tests;
