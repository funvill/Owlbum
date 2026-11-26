//! The album index (docs/05-organising.md, "The album index";
//! docs/01-architecture.md, "The two Rust crates").
//!
//! An open album keeps a compact copy of the columns that filters, sorts,
//! scores and stacks read: one fixed-size [`Photo`] per row inside the open
//! scope with `missing = 0` and no `scan_error`. Nothing in it is
//! information that could be lost; it is rebuilt from the database on every
//! open.
//!
//! # Reading
//!
//! [`AlbumIndex::read`] runs a closure over an [`IndexView`] while holding
//! the index's read lock, so one query sees one consistent state. The view
//! gives the records in id order, lookup by id, the folder list, file names
//! and the name order. Filters, sorting, scoring and stacking (#27 to #31)
//! are built on it.
//!
//! # Keeping in step
//!
//! - **On open** [`AlbumIndex::load`] reads every row with one `SELECT`.
//! - **On every write** the writer reports the photo ids it changed, and the
//!   index reads those rows again after the commit, on the writer thread,
//!   before the write answers: see [`crate::album::Album::write_photos`] and
//!   [`Touched`]. A row that is gone, missing, unreadable or outside the
//!   scope leaves the index.
//! - **In a view-only album** there is no writer. Rows that exist only in
//!   memory for the session are put in with [`AlbumIndex::upsert`] and taken
//!   out with [`AlbumIndex::remove`].
//! - **Derived values** (score, flags, bursts) are worked out elsewhere from
//!   a copy of the records and put in whole with
//!   [`AlbumIndex::install_derived`]. A row the rebuild did not cover reads
//!   as unscored.
//!
//! # Memory
//!
//! A [`Photo`] is 136 bytes. The index also keeps every file name once in
//! one string, each folder once, and the name order (4 bytes a photo).
//! [`AlbumIndex::heap_bytes`] counts it all.

pub mod natural;
pub mod record;

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::{RwLock, RwLockWriteGuard};

use rusqlite::{Connection, OptionalExtension, Row};

use crate::album::sqlite_error;
use crate::error::AppResult;
use crate::text;

pub use record::{
    Analysis, ColourBucket, Derived, Kind, NO_DATE, Photo, PhotoFacts, parse_date, quality,
};

/// The photo ids a write changed, reported by the write job so the album
/// index can read those rows again after the commit.
///
/// Report every photo whose `photos` row was inserted, changed or deleted,
/// and every photo whose `photo_people` rows changed. Reporting an id that
/// did not change is harmless.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Touched {
    ids: Vec<i64>,
}

impl Touched {
    /// Reports one photo.
    pub fn photo(&mut self, id: i64) {
        self.ids.push(id);
    }

    /// Reports several photos.
    pub fn photos(&mut self, ids: impl IntoIterator<Item = i64>) {
        self.ids.extend(ids);
    }

    /// The reported ids, in the order reported (with repeats).
    pub fn ids(&self) -> &[i64] {
        &self.ids
    }
}

/// The in-memory album index of one open album. Shared between the
/// album's writer thread and the query threads.
#[derive(Debug)]
pub struct AlbumIndex {
    state: RwLock<State>,
}

#[derive(Debug, Default)]
struct State {
    scope: String,
    generation: u64,
    /// Every photo, sorted by id.
    photos: Vec<Photo>,
    /// Folder paths by [`Photo::folder`].
    folders: Vec<Box<str>>,
    folder_ids: HashMap<Box<str>, u32>,
    /// Every file name, one after another; a photo points into it.
    names: String,
    /// Bytes of `names` no photo points at any more.
    dead_names: usize,
    /// Photo ids in name order.
    name_order: Vec<u32>,
}

/// The columns the index reads, in the order [`facts_from_row`] expects.
/// `?1` is the scope. The scope test is a plain comparison, not `LIKE`,
/// which would ignore ASCII case and read `_` and `%` as wildcards.
const SELECT: &str = "SELECT p.id, p.rel_dir, p.file_name, p.kind, p.captured_at, p.date_override,
        p.star, p.hidden, p.score_adjust, p.width, p.height,
        p.gps_lat, p.gps_lon, p.loc_lat, p.loc_lon, p.edit IS NOT NULL, p.analysis,
        (SELECT count(*) FROM photo_people pp WHERE pp.photo_id = p.id)
   FROM photos p
  WHERE p.missing = 0 AND p.scan_error IS NULL
    AND (?1 = '' OR p.rel_dir = ?1 OR substr(p.rel_dir, 1, length(?1) + 1) = ?1 || '/')";

/// When a change re-places more names than one in this many photos, the
/// whole name order is sorted again instead of placing them one by one.
const RESORT_SHARE: usize = 8;

/// The name store is compacted when more than half of it, and more than
/// this many bytes, is no longer used.
const COMPACT_ABOVE: usize = 4096;

/// A consistent view of the index for the length of one read.
#[derive(Debug)]
pub struct IndexView<'a> {
    state: &'a State,
}

impl AlbumIndex {
    /// An empty index for an album opened at `scope` (`""` for the whole
    /// album).
    pub fn empty(scope: &str) -> Self {
        Self {
            state: RwLock::new(State {
                scope: scope.to_owned(),
                ..State::default()
            }),
        }
    }

    /// Loads the index from the album database: every row inside `scope`
    /// with `missing = 0` and no `scan_error`, in one `SELECT`.
    pub fn load(conn: &Connection, scope: &str) -> AppResult<Self> {
        Ok(Self {
            state: RwLock::new(State::load(conn, scope)?),
        })
    }

    /// Replaces everything in the index with a fresh load from `conn`.
    /// Derived values are cleared.
    pub fn reload(&self, conn: &Connection) -> AppResult<()> {
        let scope = self.read(|v| v.scope().to_owned());
        let mut fresh = State::load(conn, &scope)?;
        let mut state = self.write();
        fresh.generation = state.generation + 1;
        *state = fresh;
        Ok(())
    }

    /// Reads the rows of `ids` from `conn` again and applies them: a row
    /// that is eligible is added or replaced; one that is not (deleted,
    /// `missing`, with a `scan_error`, outside the scope) is taken out.
    /// Applied in one step under the write lock. A replaced row keeps its
    /// derived values until the next rebuild.
    pub fn refresh(&self, conn: &Connection, ids: &[i64]) -> AppResult<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let mut ids = ids.to_vec();
        ids.sort_unstable();
        ids.dedup();
        let scope = self.read(|v| v.scope().to_owned());
        let failed = sqlite_error(text::album::READING_DATABASE);
        let mut stmt = conn
            .prepare_cached(&format!("{SELECT} AND p.id = ?2"))
            .map_err(&failed)?;
        let mut found = Vec::new();
        let mut gone = Vec::new();
        for id in ids {
            match stmt
                .query_row((&scope, id), facts_from_row)
                .optional()
                .map_err(&failed)?
            {
                Some(facts) => found.push(facts),
                None => gone.push(id),
            }
        }
        self.write().apply(&found, &gone);
        Ok(())
    }

    /// Adds or replaces rows from facts held in memory (a view-only
    /// album's scan). A row outside the scope is taken out instead.
    pub fn upsert(&self, facts: &[PhotoFacts]) {
        if !facts.is_empty() {
            self.write().apply(facts, &[]);
        }
    }

    /// Takes rows out (a view-only album's missing or unreadable files).
    pub fn remove(&self, ids: &[i64]) {
        if !ids.is_empty() {
            self.write().apply(&[], ids);
        }
    }

    /// Puts a rebuild's derived values in, in one step. `values` are keyed
    /// by photo id; a photo with no entry gets [`Derived::default`] (not
    /// scored), and an entry for a photo no longer in the index is ignored.
    pub fn install_derived(&self, mut values: Vec<(u32, Derived)>) {
        values.sort_by_key(|(id, _)| *id);
        let mut state = self.write();
        let mut next = values.iter().peekable();
        for photo in &mut state.photos {
            while next.next_if(|(id, _)| *id < photo.id).is_some() {}
            photo.derived = match next.peek() {
                Some((id, derived)) if *id == photo.id => *derived,
                _ => Derived::default(),
            };
        }
    }

    /// Runs `f` over a consistent view of the index, holding the read lock
    /// for its length. Do not write to the album from inside `f`.
    pub fn read<T>(&self, f: impl FnOnce(&IndexView<'_>) -> T) -> T {
        let state = self.state.read().unwrap_or_else(|p| p.into_inner());
        f(&IndexView { state: &state })
    }

    /// The number of photos.
    pub fn len(&self) -> usize {
        self.read(|v| v.len())
    }

    /// True when there are no photos.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Goes up by one each time the photos' facts change (not the derived
    /// values). A rebuild can compare it to tell whether it is out of date.
    pub fn generation(&self) -> u64 {
        self.read(|v| v.generation())
    }

    /// The bytes the index holds on the heap, by capacity: the records, the
    /// file names, the folders and the name order.
    pub fn heap_bytes(&self) -> usize {
        self.read(|v| {
            let s = v.state;
            let folder_text: usize = s.folders.iter().map(|f| f.len()).sum();
            s.photos.capacity() * size_of::<Photo>()
                + s.names.capacity()
                + s.name_order.capacity() * size_of::<u32>()
                + s.folders.capacity() * size_of::<Box<str>>()
                + s.folder_ids.capacity() * (size_of::<Box<str>>() + size_of::<u32>() + 1)
                + 2 * folder_text
        })
    }

    fn write(&self) -> RwLockWriteGuard<'_, State> {
        self.state.write().unwrap_or_else(|p| p.into_inner())
    }
}

impl IndexView<'_> {
    /// Every photo, in id order.
    pub fn photos(&self) -> &[Photo] {
        &self.state.photos
    }

    /// The number of photos.
    pub fn len(&self) -> usize {
        self.photos().len()
    }

    /// True when there are no photos.
    pub fn is_empty(&self) -> bool {
        self.photos().is_empty()
    }

    /// The photo with `id`.
    pub fn get(&self, id: u32) -> Option<&Photo> {
        self.state.get(id)
    }

    /// The photo ids in name order (docs/05-organising.md, "Sorting"):
    /// `name_order()[p.name_rank] == p.id`.
    pub fn name_order(&self) -> &[u32] {
        &self.state.name_order
    }

    /// The folder (`rel_dir`) of a [`Photo::folder`] index.
    ///
    /// Panics when `folder` is not an index this view gave out.
    pub fn folder(&self, folder: u32) -> &str {
        &self.state.folders[folder as usize]
    }

    /// The number of folders. Folder indexes run from 0 to this; a folder
    /// whose photos have all left the index may still be listed.
    pub fn folder_count(&self) -> usize {
        self.state.folders.len()
    }

    /// A photo's `file_name`.
    pub fn file_name(&self, photo: &Photo) -> &str {
        self.state.name(photo)
    }

    /// The scope the album was opened at; `""` for the whole album.
    pub fn scope(&self) -> &str {
        &self.state.scope
    }

    /// See [`AlbumIndex::generation`].
    pub fn generation(&self) -> u64 {
        self.state.generation
    }
}

impl State {
    fn load(conn: &Connection, scope: &str) -> AppResult<Self> {
        let failed = sqlite_error(text::album::READING_DATABASE);
        let mut state = Self {
            scope: scope.to_owned(),
            ..Self::default()
        };
        let mut stmt = conn
            .prepare(&format!("{SELECT} ORDER BY p.id"))
            .map_err(&failed)?;
        let mut rows = stmt.query([scope]).map_err(&failed)?;
        while let Some(row) = rows.next().map_err(&failed)? {
            let facts = facts_from_row(row).map_err(&failed)?;
            let folder = state.intern_folder(&facts.rel_dir);
            // An id beyond 32 bits cannot be in a result; it is skipped
            // rather than cut short to someone else's id.
            if let Some(mut photo) = record::build(&facts, folder) {
                state.place_name(&mut photo, &facts.file_name);
                state.photos.push(photo);
            }
        }
        state.sort_names();
        state.photos.shrink_to_fit();
        state.names.shrink_to_fit();
        Ok(state)
    }

    fn position(&self, id: u32) -> Result<usize, usize> {
        self.photos.binary_search_by_key(&id, |p| p.id)
    }

    fn get(&self, id: u32) -> Option<&Photo> {
        self.position(id).ok().map(|i| &self.photos[i])
    }

    fn name(&self, photo: &Photo) -> &str {
        let at = photo.name_at as usize;
        &self.names[at..at + usize::from(photo.name_len)]
    }

    /// The same rule as the scope test in [`SELECT`].
    fn in_scope(&self, rel_dir: &str) -> bool {
        let scope = self.scope.as_str();
        scope.is_empty()
            || rel_dir == scope
            || (rel_dir.starts_with(scope) && rel_dir.as_bytes().get(scope.len()) == Some(&b'/'))
    }

    fn intern_folder(&mut self, rel_dir: &str) -> u32 {
        if let Some(&id) = self.folder_ids.get(rel_dir) {
            return id;
        }
        let id = u32::try_from(self.folders.len()).unwrap_or(u32::MAX);
        self.folders.push(rel_dir.into());
        self.folder_ids.insert(rel_dir.into(), id);
        id
    }

    /// Stores `name` in the name store and points `photo` at it. A name
    /// longer than 65,535 bytes (no file system allows one) is cut at a
    /// character boundary.
    fn place_name(&mut self, photo: &mut Photo, name: &str) {
        let mut len = name.len().min(usize::from(u16::MAX));
        while !name.is_char_boundary(len) {
            len -= 1;
        }
        photo.name_at = u32::try_from(self.names.len()).unwrap_or(u32::MAX);
        photo.name_len = u16::try_from(len).unwrap_or(u16::MAX);
        self.names.push_str(&name[..len]);
    }

    /// The name order: file name, then folder, then id.
    fn compare_names(&self, a: &Photo, b: &Photo) -> Ordering {
        natural::compare(self.name(a), self.name(b))
            .then_with(|| {
                natural::compare(
                    &self.folders[a.folder as usize],
                    &self.folders[b.folder as usize],
                )
            })
            .then_with(|| a.id.cmp(&b.id))
    }

    /// Sorts the whole name order again and renumbers every rank.
    fn sort_names(&mut self) {
        let mut order: Vec<usize> = (0..self.photos.len()).collect();
        order.sort_unstable_by(|&a, &b| self.compare_names(&self.photos[a], &self.photos[b]));
        for (rank, &at) in order.iter().enumerate() {
            self.photos[at].name_rank = u32::try_from(rank).unwrap_or(u32::MAX);
        }
        self.name_order = order.into_iter().map(|at| self.photos[at].id).collect();
    }

    /// Renumbers every rank from the name order.
    fn renumber(&mut self) {
        for rank in 0..self.name_order.len() {
            if let Ok(at) = self.position(self.name_order[rank]) {
                self.photos[at].name_rank = u32::try_from(rank).unwrap_or(u32::MAX);
            }
        }
    }

    /// Adds or replaces the rows of `facts` and takes out the rows of
    /// `remove`, as one change. When an id is in both, the facts win; when
    /// facts name an id twice, the last wins.
    fn apply(&mut self, facts: &[PhotoFacts], remove: &[i64]) {
        let mut seen = HashSet::new();
        let mut latest: Vec<&PhotoFacts> =
            facts.iter().rev().filter(|f| seen.insert(f.id)).collect();
        latest.reverse();
        let mut gone: HashSet<u32> = remove
            .iter()
            .filter(|id| !seen.contains(*id))
            .filter_map(|&id| u32::try_from(id).ok())
            .filter(|&id| self.position(id).is_ok())
            .collect();

        // Ids that need a (new) place in the name order.
        let mut placed: Vec<u32> = Vec::new();
        let mut added: Vec<Photo> = Vec::new();
        for f in latest {
            let Ok(id) = u32::try_from(f.id) else {
                continue;
            };
            if !self.in_scope(&f.rel_dir) {
                if self.position(id).is_ok() {
                    gone.insert(id);
                }
                continue;
            }
            let folder = self.intern_folder(&f.rel_dir);
            let Some(mut photo) = record::build(f, folder) else {
                continue;
            };
            match self.position(id) {
                Ok(at) => {
                    let old = self.photos[at];
                    photo.derived = old.derived;
                    photo.name_rank = old.name_rank;
                    if old.folder == folder && self.name(&old) == f.file_name {
                        photo.name_at = old.name_at;
                        photo.name_len = old.name_len;
                    } else {
                        self.dead_names += usize::from(old.name_len);
                        self.place_name(&mut photo, &f.file_name);
                        placed.push(id);
                    }
                    self.photos[at] = photo;
                }
                Err(_) => {
                    self.place_name(&mut photo, &f.file_name);
                    placed.push(id);
                    added.push(photo);
                }
            }
        }
        if gone.is_empty() && placed.is_empty() && facts.is_empty() {
            return;
        }
        self.generation += 1;

        if !gone.is_empty() {
            let mut dead = 0;
            self.photos.retain(|p| {
                let keep = !gone.contains(&p.id);
                if !keep {
                    dead += usize::from(p.name_len);
                }
                keep
            });
            self.dead_names += dead;
        }
        if !added.is_empty() {
            let in_order = added.is_sorted_by_key(|p| p.id)
                && self.photos.last().is_none_or(|last| last.id < added[0].id);
            self.photos.extend(added);
            if !in_order {
                self.photos.sort_by_key(|p| p.id);
            }
        }

        if placed.len() * RESORT_SHARE > self.photos.len() {
            self.sort_names();
        } else if !gone.is_empty() || !placed.is_empty() {
            let moved: HashSet<u32> = placed.iter().copied().collect();
            self.name_order
                .retain(|id| !gone.contains(id) && !moved.contains(id));
            for id in placed {
                let Some(photo) = self.get(id).copied() else {
                    continue;
                };
                let at = self.name_order.partition_point(|&other| {
                    self.get(other)
                        .is_some_and(|o| self.compare_names(o, &photo) == Ordering::Less)
                });
                self.name_order.insert(at, id);
            }
            self.renumber();
        }
        self.compact_names();
    }

    /// Rebuilds the name store when most of it is no longer used.
    fn compact_names(&mut self) {
        if self.dead_names <= COMPACT_ABOVE || self.dead_names * 2 <= self.names.len() {
            return;
        }
        let mut names = String::with_capacity(self.names.len() - self.dead_names);
        for photo in &mut self.photos {
            let at = photo.name_at as usize;
            let name = &self.names[at..at + usize::from(photo.name_len)];
            photo.name_at = u32::try_from(names.len()).unwrap_or(u32::MAX);
            names.push_str(name);
        }
        self.names = names;
        self.dead_names = 0;
    }
}

/// Reads one row of [`SELECT`]. Text and real columns that hold something
/// else read as absent, so one odd row cannot stop the load.
fn facts_from_row(row: &Row<'_>) -> rusqlite::Result<PhotoFacts> {
    let text = |i: usize| -> rusqlite::Result<Option<String>> {
        Ok(row.get_ref(i)?.as_str().ok().map(str::to_owned))
    };
    let real = |i: usize| -> rusqlite::Result<Option<f64>> {
        Ok(match row.get_ref(i)? {
            rusqlite::types::ValueRef::Real(x) => Some(x),
            rusqlite::types::ValueRef::Integer(n) => Some(n as f64),
            _ => None,
        })
    };
    Ok(PhotoFacts {
        id: row.get(0)?,
        rel_dir: text(1)?.unwrap_or_default(),
        file_name: text(2)?.unwrap_or_default(),
        kind: text(3)?.unwrap_or_default(),
        captured_at: text(4)?,
        date_override: text(5)?,
        star: row.get::<_, i64>(6)? != 0,
        hidden: row.get::<_, i64>(7)? != 0,
        score_adjust: row.get(8)?,
        width: row.get(9)?,
        height: row.get(10)?,
        gps: real(11)?.zip(real(12)?),
        loc: real(13)?.zip(real(14)?),
        has_edit: row.get::<_, i64>(15)? != 0,
        analysis: text(16)?,
        people: u32::try_from(row.get::<_, i64>(17)?).unwrap_or(u32::MAX),
    })
}

#[cfg(test)]
mod tests;
