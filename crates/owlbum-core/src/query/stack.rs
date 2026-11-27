//! Stacks: the bursts of a result folded into one tile each
//! (docs/05-organising.md, "Stacks").
//!
//! Folding runs after the filter and the sort, so a stack only ever holds
//! photos that pass. A burst with two or more members in the sorted list
//! folds; one with a single member passing is a plain tile. The face is the
//! first starred member in sort order, else the burst's keeper if it
//! passed, else the first member in sort order; the stack sits where its
//! face sorts. An open stack (its id in `open_stacks`) shows its other
//! members right after the face, in sort order.
//!
//! A stack's id is its burst's id. Everything here is two linear passes
//! over the sorted ids with tables indexed by id (ids are dense row ids),
//! so folding 100,000 photos costs a few milliseconds.

use std::collections::{HashMap, HashSet};

use super::shape::RowStack;
use crate::index::IndexView;

/// A table slot holding this is empty.
const NONE: u32 = u32::MAX;

/// One stack that folds: two or more members pass.
#[derive(Debug, Clone, Copy)]
struct Stack {
    /// The burst's id, which is the stack's id.
    id: u32,
    /// Shown unfolded.
    open: bool,
    /// Where its members start in [`Stacks::members`]; the face first,
    /// then the other members in sort order.
    start: u32,
    /// How many members pass the filter.
    len: u32,
    /// The face's tile index.
    face_tile: u32,
}

/// The stacks of one result, kept beside its tiles.
#[derive(Debug, Default)]
pub struct Stacks {
    list: Vec<Stack>,
    /// Every member of every stack, stack by stack.
    members: Vec<u32>,
    /// Per tile: the index of its stack in `list`, or [`NONE`]. Empty when
    /// nothing folds.
    tile: Vec<u32>,
    /// Photos folded inside a closed stack with their stack's index in
    /// `list`, sorted by id.
    folded: Vec<(u32, u32)>,
}

/// One id-indexed table, or a map when the ids are too sparse for one.
enum Table {
    Dense(Vec<u32>),
    Sparse(HashMap<u32, u32>),
}

impl Table {
    fn new(largest: u32, len: usize) -> Self {
        if (largest as usize) <= len.saturating_mul(4).saturating_add(65_536) {
            Self::Dense(vec![NONE; largest as usize + 1])
        } else {
            Self::Sparse(HashMap::new())
        }
    }

    fn get(&self, id: u32) -> Option<u32> {
        match self {
            Self::Dense(t) => t.get(id as usize).copied().filter(|&v| v != NONE),
            Self::Sparse(m) => m.get(&id).copied(),
        }
    }

    fn set(&mut self, id: u32, value: u32) {
        match self {
            Self::Dense(t) => t[id as usize] = value,
            Self::Sparse(m) => {
                m.insert(id, value);
            }
        }
    }
}

/// What pass one learns of each burst in the sorted list.
struct Burst {
    id: u32,
    count: u32,
    first: u32,
    starred: u32,
    keeper: u32,
    /// The burst's index in [`Stacks::list`] once it folds; else [`NONE`].
    stack: u32,
}

/// Folds `sorted`, the ids of a result in sort order, into tiles. Answers
/// the tiles and their stacks; `sorted` comes back unchanged with no
/// stacks when no burst has two members in it.
pub fn fold(view: &IndexView<'_>, sorted: Vec<u32>, open_stacks: &[i64]) -> (Vec<u32>, Stacks) {
    // Pass one: each photo's burst, and per burst its count, first member,
    // first starred member and keeper, in sort order.
    let mut burst_of: Vec<u32> = Vec::with_capacity(sorted.len());
    let mut bursts: Vec<Burst> = Vec::new();
    let mut largest = 0;
    let mut any = false;
    for &id in &sorted {
        let b = view
            .get(id)
            .and_then(|p| p.derived.burst)
            .map_or(NONE, |b| b.get());
        if b != NONE {
            largest = largest.max(b);
            any = true;
        }
        burst_of.push(b);
    }
    if !any {
        return (sorted, Stacks::default());
    }
    let mut slot = Table::new(largest, sorted.len());
    for (pos, (&id, &b)) in sorted.iter().zip(&burst_of).enumerate() {
        if b == NONE {
            continue;
        }
        let pos = index(pos);
        let at = match slot.get(b) {
            Some(at) => at as usize,
            None => {
                slot.set(b, index(bursts.len()));
                bursts.push(Burst {
                    id: b,
                    count: 0,
                    first: pos,
                    starred: NONE,
                    keeper: NONE,
                    stack: NONE,
                });
                bursts.len() - 1
            }
        };
        let burst = &mut bursts[at];
        burst.count += 1;
        if let Some(p) = view.get(id) {
            if burst.starred == NONE && p.star() {
                burst.starred = pos;
            }
            if burst.keeper == NONE && p.derived.keeper {
                burst.keeper = pos;
            }
        }
    }

    // The stacks: bursts with two or more members passing, each with its
    // face first among its members.
    let open: HashSet<i64> = open_stacks.iter().copied().collect();
    let mut stacks = Stacks::default();
    let mut face_at: Vec<u32> = Vec::new();
    for burst in &mut bursts {
        if burst.count < 2 {
            continue;
        }
        let face = [burst.starred, burst.keeper, burst.first]
            .into_iter()
            .find(|&p| p != NONE)
            .unwrap_or(burst.first);
        burst.stack = index(stacks.list.len());
        stacks.list.push(Stack {
            id: burst.id,
            open: open.contains(&i64::from(burst.id)),
            start: index(stacks.members.len()),
            len: burst.count,
            face_tile: NONE,
        });
        stacks.members.push(sorted[face as usize]);
        // Room for the other members, filled in sort order below.
        stacks
            .members
            .extend(std::iter::repeat_n(NONE, burst.count as usize - 1));
        face_at.push(face);
    }
    if stacks.list.is_empty() {
        return (sorted, Stacks::default());
    }
    let mut fill: Vec<u32> = stacks.list.iter().map(|s| s.start + 1).collect();
    let stack_at = |pos: usize| -> u32 {
        let b = burst_of[pos];
        if b == NONE {
            return NONE;
        }
        slot.get(b).map_or(NONE, |at| bursts[at as usize].stack)
    };
    for (pos, &id) in sorted.iter().enumerate() {
        let s = stack_at(pos);
        if s != NONE && face_at[s as usize] != index(pos) {
            stacks.members[fill[s as usize] as usize] = id;
            fill[s as usize] += 1;
        }
    }

    // Pass two: the tiles.
    let mut tiles = Vec::with_capacity(sorted.len());
    stacks.tile.reserve(sorted.len());
    for (pos, &id) in sorted.iter().enumerate() {
        let s = stack_at(pos);
        if s == NONE {
            tiles.push(id);
            stacks.tile.push(NONE);
            continue;
        }
        let stack = stacks.list[s as usize];
        if face_at[s as usize] == index(pos) {
            stacks.list[s as usize].face_tile = index(tiles.len());
            tiles.push(id);
            stacks.tile.push(s);
            if stack.open {
                let rest = (stack.start + 1) as usize..(stack.start + stack.len) as usize;
                for &m in &stacks.members[rest] {
                    tiles.push(m);
                    stacks.tile.push(s);
                }
            }
        } else if !stack.open {
            stacks.folded.push((id, s));
        }
    }
    stacks.folded.sort_unstable();
    (tiles, stacks)
}

impl Stacks {
    /// The stack of the tile at `index`, as a row shows it.
    pub fn row(&self, at: usize, id: u32) -> Option<RowStack> {
        let s = self.list[*self.tile.get(at).filter(|&&s| s != NONE)? as usize];
        let place = self
            .members_of(&s)
            .iter()
            .position(|&m| m == id)
            .map_or(1, |p| index(p) + 1);
        Some(RowStack {
            id: i64::from(s.id),
            size: s.len,
            open: s.open,
            face: place == 1,
            place,
        })
    }

    /// The id of the closed stack `id` is folded in.
    pub fn folded_in(&self, id: u32) -> Option<i64> {
        self.folded_stack(id).map(|s| i64::from(s.id))
    }

    /// How many photos the tile at `index` stands for: a closed stack's
    /// size on its face, else one.
    pub fn weight(&self, at: usize) -> u32 {
        match self.tile.get(at) {
            Some(&s) if s != NONE => {
                let s = &self.list[s as usize];
                if s.open || s.face_tile != index(at) {
                    1
                } else {
                    s.len
                }
            }
            _ => 1,
        }
    }

    /// The face tile of the closed stack `id` is folded in.
    pub fn face_of_folded(&self, id: u32) -> Option<(u32, u32)> {
        self.folded_stack(id)
            .map(|s| (self.members[s.start as usize], s.face_tile))
    }

    /// The other members of the stack whose face was `id`, when `id` was a
    /// face in this result.
    pub fn others_if_face(&self, id: u32) -> Option<&[u32]> {
        let s = self
            .list
            .iter()
            .find(|s| self.members[s.start as usize] == id)?;
        Some(&self.members_of(s)[1..])
    }

    /// The face tile of the stack photo `id` is in, folded or shown.
    pub fn face_tile_of(&self, id: u32, tile_index: Option<u32>) -> Option<(u32, u32)> {
        if let Some(face) = self.face_of_folded(id) {
            return Some(face);
        }
        let s = *self
            .tile
            .get(tile_index? as usize)
            .filter(|&&s| s != NONE)?;
        let s = &self.list[s as usize];
        Some((self.members[s.start as usize], s.face_tile))
    }

    fn folded_stack(&self, id: u32) -> Option<&Stack> {
        let at = self.folded.binary_search_by_key(&id, |&(m, _)| m).ok()?;
        Some(&self.list[self.folded[at].1 as usize])
    }

    fn members_of(&self, s: &Stack) -> &[u32] {
        &self.members[s.start as usize..(s.start + s.len) as usize]
    }
}

fn index(i: usize) -> u32 {
    u32::try_from(i).unwrap_or(u32::MAX)
}
