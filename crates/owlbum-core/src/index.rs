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

use std::sync::RwLock;

use rusqlite::Connection;

use crate::error::AppResult;

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
}

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
        unimplemented!("{conn:?}{scope}")
    }

    /// Replaces everything in the index with a fresh load from `conn`.
    /// Derived values are cleared.
    pub fn reload(&self, conn: &Connection) -> AppResult<()> {
        unimplemented!("{conn:?}")
    }

    /// Reads the rows of `ids` from `conn` again and applies them: a row
    /// that is eligible is added or replaced; one that is not (deleted,
    /// `missing`, with a `scan_error`, outside the scope) is taken out.
    /// Applied in one step under the write lock. A replaced row keeps its
    /// derived values until the next rebuild.
    pub fn refresh(&self, conn: &Connection, ids: &[i64]) -> AppResult<()> {
        unimplemented!("{conn:?}{ids:?}")
    }

    /// Adds or replaces rows from facts held in memory (a view-only
    /// album's scan). A row outside the scope is taken out instead.
    pub fn upsert(&self, facts: &[PhotoFacts]) {
        unimplemented!("{facts:?}")
    }

    /// Takes rows out (a view-only album's missing or unreadable files).
    pub fn remove(&self, ids: &[i64]) {
        unimplemented!("{ids:?}")
    }

    /// Puts a rebuild's derived values in, in one step. `values` are keyed
    /// by photo id; a photo with no entry gets [`Derived::default`] (not
    /// scored), and an entry for a photo no longer in the index is ignored.
    pub fn install_derived(&self, values: Vec<(u32, Derived)>) {
        unimplemented!("{values:?}")
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
        unimplemented!()
    }
}

impl IndexView<'_> {
    /// Every photo, in id order.
    pub fn photos(&self) -> &[Photo] {
        unimplemented!()
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
        unimplemented!("{id}")
    }

    /// The photo ids in name order (docs/05-organising.md, "Sorting"):
    /// `name_order()[p.name_rank] == p.id`.
    pub fn name_order(&self) -> &[u32] {
        unimplemented!()
    }

    /// The folder (`rel_dir`) of a [`Photo::folder`] index.
    pub fn folder(&self, folder: u32) -> &str {
        unimplemented!("{folder}")
    }

    /// The number of folders. Folder indexes run from 0 to this; a folder
    /// whose photos have all left the index may still be listed.
    pub fn folder_count(&self) -> usize {
        unimplemented!()
    }

    /// A photo's `file_name`.
    pub fn file_name(&self, photo: &Photo) -> &str {
        unimplemented!("{photo:?}")
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

#[cfg(test)]
mod tests;
