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
//! - **Bursts** (with `quality` and the keepers) are worked out on load
//!   and by [`AlbumIndex::rebuild`] (a scan, at most every 2 seconds and
//!   at its end; a changed date), from a copy of the records outside the
//!   lock. A star or hide of a burst member picks that burst's keeper
//!   again and scores its members as soon as the row is refreshed.
//! - **Other derived values** can be put in whole with
//!   [`AlbumIndex::install_derived`]. A row the rebuild did not cover reads
//!   as unscored.
//! - **The score** is worked out here: for every photo on load and on
//!   [`AlbumIndex::rescore`] (a scan's batches), and for a replaced row as
//!   soon as it is refreshed, against the sharpness list of the last
//!   rescore. A row added since then reads as unscored until the next
//!   rescore.
//!
//! # Memory
//!
//! A [`Photo`] is 136 bytes. The index also keeps every file name once in
//! one string, each folder once, and the name order (4 bytes a photo).
//! [`AlbumIndex::heap_bytes`] counts it all.

pub mod record;

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::{RwLock, RwLockWriteGuard};

use rusqlite::{Connection, OpenFlags, OptionalExtension, Row};

use crate::album::sqlite_error;
use crate::error::{AppError, AppResult};
use crate::organise::{bursts, facts, score};
use crate::scan::natural_cmp_text;
use crate::text;

pub use record::{
    Analysis, ColourBucket, Derived, Kind, NO_DATE, NO_FORMAT, Photo, PhotoFacts, parse_date,
    quality,
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
    /// How many photos each folder holds, by [`Photo::folder`]; a folder
    /// whose photos all left keeps its index with 0.
    folder_photos: Vec<u32>,
    /// Every file name, one after another; a photo points into it.
    names: String,
    /// Bytes of `names` no photo points at any more.
    dead_names: usize,
    /// Photo ids in name order.
    name_order: Vec<u32>,
    /// The sharpness of every analysed photo at the last rescore, sorted
    /// ascending: the `S` of the photo score's Focus line.
    sharpness: Vec<f32>,
}

/// The columns the index reads, in the order [`facts_from_row`] expects.
/// `?1` is the scope. The scope test is a plain comparison, not `LIKE`,
/// which would ignore ASCII case and read `_` and `%` as wildcards.
const SELECT: &str = "SELECT p.id, p.rel_dir, p.file_name, p.kind, p.captured_at, p.date_override,
        p.star, p.hidden, p.score_adjust, p.width, p.height,
        p.gps_lat, p.gps_lon, p.loc_lat, p.loc_lon, p.edit IS NOT NULL, p.analysis,
        (SELECT count(*) FROM photo_people pp WHERE pp.photo_id = p.id),
        p.format, p.duration_ms IS NOT NULL
   FROM photos p
  WHERE p.missing = 0 AND p.scan_error IS NULL
    AND (?1 = '' OR p.rel_dir = ?1 OR substr(p.rel_dir, 1, length(?1) + 1) = ?1 || '/')";

/// The rows of `scope` in id order, those with ids in `ids` only when
/// given.
fn read_rows(
    conn: &Connection,
    scope: &str,
    ids: Option<(i64, i64)>,
) -> AppResult<Vec<PhotoFacts>> {
    let failed = sqlite_error(text::album::READING_DATABASE);
    let (from, to) = ids.unwrap_or((i64::MIN, i64::MAX));
    let mut stmt = conn
        .prepare(&format!(
            "{SELECT} AND p.id BETWEEN ?2 AND ?3 ORDER BY p.id"
        ))
        .map_err(&failed)?;
    let mut rows = stmt.query((scope, from, to)).map_err(&failed)?;
    let mut all = Vec::new();
    while let Some(row) = rows.next().map_err(&failed)? {
        all.push(facts_from_row(row).map_err(&failed)?);
    }
    Ok(all)
}

/// Rows per thread at least when a load reads in parallel.
const READ_SHARE: i64 = 8192;

/// The rows of `scope` in id order, read from the database at `path` on
/// several read-only connections at once, each a range of ids: reading
/// and copying the rows was most of an open's 1 s at 100,000 photos
/// (issue 162). Only for a database no one writes to while it reads (the
/// open holds the album's lock and has not started its writer).
fn read_rows_parallel(path: &Path, scope: &str) -> AppResult<Vec<PhotoFacts>> {
    let failed = sqlite_error(text::album::READING_DATABASE);
    let open = || {
        let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let conn = Connection::open_with_flags(path, flags).map_err(&failed)?;
        conn.busy_timeout(crate::album::db::BUSY_TIMEOUT)
            .map_err(&failed)?;
        Ok::<_, AppError>(conn)
    };
    let conn = open()?;
    let (lo, hi): (Option<i64>, Option<i64>) = conn
        .query_row("SELECT min(id), max(id) FROM photos", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .map_err(&failed)?;
    let (Some(lo), Some(hi)) = (lo, hi) else {
        return Ok(Vec::new());
    };
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let span = hi.saturating_sub(lo).saturating_add(1);
    let parts = i64::try_from(threads.min(8))
        .unwrap_or(1)
        .min(span / READ_SHARE)
        .max(1);
    if parts == 1 {
        return read_rows(&conn, scope, None);
    }
    let step = (span + parts - 1) / parts;
    let ranges: Vec<(i64, i64)> = (0..parts)
        .map(|k| {
            let from = lo.saturating_add(k * step);
            let to = if k == parts - 1 {
                i64::MAX
            } else {
                from + step - 1
            };
            (if k == 0 { i64::MIN } else { from }, to)
        })
        .collect();
    let read: Vec<AppResult<Vec<PhotoFacts>>> = std::thread::scope(|s| {
        let handles: Vec<_> = ranges
            .iter()
            .map(|&range| s.spawn(move || read_rows(&open()?, scope, Some(range))))
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
            .collect()
    });
    let mut all = Vec::new();
    for part in read {
        all.extend(part?);
    }
    Ok(all)
}

/// Below this many rows a load builds its records on one thread.
const BUILD_ALONE: usize = 4096;

/// [`record::build`] of every row, in order, split over the processors:
/// at 100,000 photos one thread took about 0.3 s, mostly reading the
/// `analysis` JSON (issue 162).
fn build_all(all: &[PhotoFacts], folders: &[u32]) -> Vec<Option<Photo>> {
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    if all.len() < BUILD_ALONE || threads < 2 {
        return all
            .iter()
            .zip(folders)
            .map(|(f, &folder)| record::build(f, folder))
            .collect();
    }
    let chunk = all.len().div_ceil(threads.min(8));
    std::thread::scope(|s| {
        let parts: Vec<_> = all
            .chunks(chunk)
            .zip(folders.chunks(chunk))
            .map(|(facts, folders)| {
                s.spawn(move || {
                    facts
                        .iter()
                        .zip(folders)
                        .map(|(f, &folder)| record::build(f, folder))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        parts
            .into_iter()
            .flat_map(|p| p.join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
            .collect()
    })
}

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
        let mut state = State::load(conn, scope)?;
        state.rebuild();
        Ok(Self {
            state: RwLock::new(state),
        })
    }

    /// [`AlbumIndex::load`] from the database file at `path`, read on
    /// several read-only connections at once. Only for a database no one
    /// writes to meanwhile: the album open, before its writer starts.
    pub fn load_file(path: &Path, scope: &str) -> AppResult<Self> {
        let all = read_rows_parallel(path, scope)?;
        let mut state = State::from_rows(scope, &all);
        drop(all);
        state.rebuild();
        Ok(Self {
            state: RwLock::new(state),
        })
    }

    /// Replaces everything in the index with a fresh load from `conn`.
    /// Derived values are cleared, then bursts and scores worked out again.
    pub fn reload(&self, conn: &Connection) -> AppResult<()> {
        let scope = self.read(|v| v.scope().to_owned());
        let mut fresh = State::load(conn, &scope)?;
        fresh.rebuild();
        let mut state = self.write();
        fresh.generation = state.generation + 1;
        *state = fresh;
        Ok(())
    }

    /// Reads the rows of `ids` from `conn` again and applies them: a row
    /// that is eligible is added or replaced; one that is not (deleted,
    /// `missing`, with a `scan_error`, outside the scope) is taken out.
    /// Applied in one step under the write lock. A replaced row keeps its
    /// derived values until the next rebuild, except its score, which is
    /// worked out again at once (see [`AlbumIndex::rescore`]). A star or
    /// hidden change of a burst member picks the burst's keeper again and
    /// scores every member; a changed date rebuilds the bursts.
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
        let redated = self.write().apply(&found, &gone);
        if redated {
            self.rebuild();
        }
        Ok(())
    }

    /// Adds or replaces rows from facts held in memory (a view-only
    /// album's scan). A row outside the scope is taken out instead.
    pub fn upsert(&self, facts: &[PhotoFacts]) {
        if !facts.is_empty() {
            let redated = self.write().apply(facts, &[]);
            if redated {
                self.rebuild();
            }
        }
    }

    /// Takes rows out (a view-only album's missing or unreadable files).
    pub fn remove(&self, ids: &[i64]) {
        if !ids.is_empty() {
            // Taking rows out moves no date: nothing to rebuild.
            let _ = self.write().apply(&[], ids);
        }
    }

    /// Puts a rebuild's derived values in, in one step. `values` are keyed
    /// by photo id; a photo with no entry gets [`Derived::default`], and an
    /// entry for a photo no longer in the index is ignored. The score in
    /// `values` is not used: each photo's score is worked out again here,
    /// since its Burst line reads the burst and keeper just put in.
    pub fn install_derived(&self, mut values: Vec<(u32, Derived)>) {
        values.sort_by_key(|(id, _)| *id);
        let mut state = self.write();
        let state = &mut *state;
        let mut next = values.iter().peekable();
        for photo in &mut state.photos {
            while next.next_if(|(id, _)| *id < photo.id).is_some() {}
            photo.derived = match next.peek() {
                Some((id, derived)) if *id == photo.id => *derived,
                _ => Derived::default(),
            };
            photo.derived.score = score::score(photo, &state.sharpness);
        }
    }

    /// Works out the bursts again (docs/05-organising.md, "Bursts"): the
    /// sharpness list, every photo's `quality` and quality flags, the bursts and their
    /// keepers, then every score. The work runs on a copy of the records
    /// outside the lock; only putting the result in holds the write lock,
    /// so queries are not held up. A photo added while it ran is in no
    /// burst until the next rebuild; star and hidden are read when the
    /// result is put in, so a change that lands meanwhile is not lost.
    /// Does not change [`AlbumIndex::generation`].
    pub fn rebuild(&self) {
        let photos = self.read(|v| v.photos().to_vec());
        let sharpness = sharpness_of(&photos);
        let found = bursts::find(&photos);
        let worked: Vec<(u32, f32, u8, Option<NonZeroU32>)> = photos
            .iter()
            .zip(found)
            .map(|(p, b)| (p.id, bursts::quality(p, &sharpness), facts::flags(p, &sharpness), b))
            .collect();
        drop(photos);
        let mut state = self.write();
        let state = &mut *state;
        let mut next = worked.iter().peekable();
        for photo in &mut state.photos {
            while next.next_if(|(id, ..)| *id < photo.id).is_some() {}
            let (quality, flags, burst) = match next.peek() {
                Some((id, q, f, b)) if *id == photo.id => (*q, *f, *b),
                _ => (0.0, 0, None),
            };
            photo.derived.quality = quality;
            photo.derived.quality_flags = flags;
            photo.derived.burst = burst;
        }
        bursts::pick_keepers(&mut state.photos, None);
        state.sharpness = sharpness;
        state.score_all();
    }

    /// Works out every photo's score again (docs/05-organising.md, "Photo
    /// score"): the sharpness list from every analysed photo, then each
    /// photo's score from it. Called on load, and by a scan after it adds,
    /// removes or re-reads photos, which can change the sharpness list. A
    /// change of star, hidden, nudge or people needs no rescore: the
    /// refresh after the write scores the changed rows against the kept
    /// list. Only [`Derived::score`] is written; the other derived values
    /// are left as they are.
    pub fn rescore(&self) {
        self.write().rescore();
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
    /// file names, the folders, the name order and the sharpness list.
    pub fn heap_bytes(&self) -> usize {
        self.read(|v| {
            let s = v.state;
            let folder_text: usize = s.folders.iter().map(|f| f.len()).sum();
            s.photos.capacity() * size_of::<Photo>()
                + s.names.capacity()
                + s.name_order.capacity() * size_of::<u32>()
                + s.sharpness.capacity() * size_of::<f32>()
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

    /// Whether the folder `folder` holds a photo now: one kept count per
    /// folder, so `query_parse` need not pass over every photo (issue 162).
    pub fn folder_has_photos(&self, folder: u32) -> bool {
        self.state
            .folder_photos
            .get(folder as usize)
            .is_some_and(|&n| n > 0)
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

    /// The sharpness of every analysed photo at the last rescore, sorted
    /// ascending (the photo score's `S`).
    pub fn sharpness(&self) -> &[f32] {
        &self.state.sharpness
    }
}

impl State {
    fn load(conn: &Connection, scope: &str) -> AppResult<Self> {
        let all = read_rows(conn, scope, None)?;
        Ok(Self::from_rows(scope, &all))
    }

    /// The state of `all`, the rows of the scope in id order.
    fn from_rows(scope: &str, all: &[PhotoFacts]) -> Self {
        let mut state = Self {
            scope: scope.to_owned(),
            ..Self::default()
        };
        // Folders are numbered in row order, before the records are built
        // on several threads (reading each `analysis` is most of the work).
        let folders: Vec<u32> = all
            .iter()
            .map(|f| state.intern_folder(&f.rel_dir))
            .collect();
        let built = build_all(all, &folders);
        state.photos.reserve(all.len());
        for (facts, photo) in all.iter().zip(built) {
            // An id beyond 32 bits cannot be in a result; it is skipped
            // rather than cut short to someone else's id.
            if let Some(mut photo) = photo {
                state.place_name(&mut photo, &facts.file_name);
                state.count_folder(photo.folder, true);
                state.photos.push(photo);
            }
        }
        state.sort_names();
        state.photos.shrink_to_fit();
        state.names.shrink_to_fit();
        state
    }

    fn rescore(&mut self) {
        self.sharpness = sharpness_of(&self.photos);
        self.score_all();
    }

    /// Every photo's score against the kept sharpness list.
    fn score_all(&mut self) {
        for photo in &mut self.photos {
            photo.derived.score = score::score(photo, &self.sharpness);
        }
    }

    /// The whole rebuild in place, for a state no one else can see yet.
    fn rebuild(&mut self) {
        self.sharpness = sharpness_of(&self.photos);
        let found = bursts::find(&self.photos);
        for (photo, burst) in self.photos.iter_mut().zip(found) {
            photo.derived.quality = bursts::quality(photo, &self.sharpness);
            photo.derived.quality_flags = facts::flags(photo, &self.sharpness);
            photo.derived.burst = burst;
        }
        bursts::pick_keepers(&mut self.photos, None);
        self.score_all();
    }

    /// Picks the keepers of the `changed` bursts again from the flags as
    /// they are now, and scores every member of them.
    fn rekeep(&mut self, changed: &mut Vec<NonZeroU32>) {
        if changed.is_empty() {
            return;
        }
        changed.sort_unstable();
        changed.dedup();
        bursts::pick_keepers(&mut self.photos, Some(changed));
        for photo in &mut self.photos {
            if photo
                .derived
                .burst
                .is_some_and(|b| changed.binary_search(&b).is_ok())
            {
                photo.derived.score = score::score(photo, &self.sharpness);
            }
        }
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

    /// One photo more (`add`) or fewer in `folder`.
    fn count_folder(&mut self, folder: u32, add: bool) {
        let at = folder as usize;
        if self.folder_photos.len() <= at {
            self.folder_photos.resize(at + 1, 0);
        }
        let n = &mut self.folder_photos[at];
        *n = if add { *n + 1 } else { n.saturating_sub(1) };
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

    /// The name order: file name, then folder, by the walker's string
    /// rules (docs/03-media-and-scanning.md, "Order", steps 2 to 5), then
    /// id.
    fn compare_names(&self, a: &Photo, b: &Photo) -> Ordering {
        natural_cmp_text(self.name(a), self.name(b))
            .then_with(|| {
                natural_cmp_text(
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
    fn renumber(&mut self, at: &Positions) {
        for (rank, &id) in self.name_order.iter().enumerate() {
            if let Some(i) = at.get(id) {
                self.photos[i].name_rank = u32::try_from(rank).unwrap_or(u32::MAX);
            }
        }
    }

    /// Adds or replaces the rows of `facts` and takes out the rows of
    /// `remove`, as one change. When an id is in both, the facts win; when
    /// facts name an id twice, the last wins.
    ///
    /// Answers true when a kept row's effective date changed, which calls
    /// for the bursts to be worked out again ([`AlbumIndex::rebuild`]).
    #[must_use]
    fn apply(&mut self, facts: &[PhotoFacts], remove: &[i64]) -> bool {
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

        // Bursts whose keeper may have changed, and whether a date moved.
        let mut rekeep: Vec<NonZeroU32> = Vec::new();
        let mut redated = false;
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
                    photo.derived.score = score::score(&photo, &self.sharpness);
                    if let Some(b) = old.derived.burst
                        && (old.star() != photo.star() || old.hidden() != photo.hidden())
                    {
                        rekeep.push(b);
                    }
                    redated |= old.date != photo.date;
                    photo.name_rank = old.name_rank;
                    if old.folder == folder && self.name(&old) == f.file_name {
                        photo.name_at = old.name_at;
                        photo.name_len = old.name_len;
                    } else {
                        self.dead_names += usize::from(old.name_len);
                        self.place_name(&mut photo, &f.file_name);
                        placed.push(id);
                    }
                    self.count_folder(old.folder, false);
                    self.count_folder(photo.folder, true);
                    self.photos[at] = photo;
                }
                Err(_) => {
                    self.place_name(&mut photo, &f.file_name);
                    placed.push(id);
                    self.count_folder(photo.folder, true);
                    added.push(photo);
                }
            }
        }
        if gone.is_empty() && placed.is_empty() && facts.is_empty() {
            return false;
        }
        self.generation += 1;

        if !gone.is_empty() {
            let mut dead = 0;
            let mut left = Vec::new();
            self.photos.retain(|p| {
                let keep = !gone.contains(&p.id);
                if !keep {
                    dead += usize::from(p.name_len);
                    left.push(p.folder);
                    // A burst that lost a member may have lost its keeper.
                    rekeep.extend(p.derived.burst);
                }
                keep
            });
            self.dead_names += dead;
            for folder in left {
                self.count_folder(folder, false);
            }
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
            // Lookups by id here are many and scattered; one pass builds a
            // table instead of a binary search over the records each time.
            let at = Positions::of(&self.photos);
            for id in placed {
                let Some(photo) = at.get(id).map(|i| self.photos[i]) else {
                    continue;
                };
                let rank = self.name_order.partition_point(|&other| {
                    at.get(other).is_some_and(|i| {
                        self.compare_names(&self.photos[i], &photo) == Ordering::Less
                    })
                });
                self.name_order.insert(rank, id);
            }
            self.renumber(&at);
        }
        self.compact_names();
        self.rekeep(&mut rekeep);
        redated
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

/// The sharpness of every analysed photo, sorted ascending: the `S` of
/// the photo score's Focus line.
fn sharpness_of(photos: &[Photo]) -> Vec<f32> {
    let mut sharpness: Vec<f32> = photos
        .iter()
        .filter_map(|p| p.analysis().map(|a| a.sharpness))
        .collect();
    sharpness.sort_unstable_by(f32::total_cmp);
    sharpness
}

/// Record positions by photo id, built in one pass for a change that looks
/// up many ids. A dense table when ids are dense (the usual case: SQLite
/// hands out ids in order), a hash map when they are sparse.
enum Positions {
    Table(Vec<u32>),
    Map(HashMap<u32, u32>),
}

impl Positions {
    const NONE: u32 = u32::MAX;

    fn of(photos: &[Photo]) -> Self {
        let max = photos.last().map_or(0, |p| p.id as usize);
        if max <= photos.len() * 4 + 1024 {
            let mut table = vec![Self::NONE; max + 1];
            for (i, p) in photos.iter().enumerate() {
                table[p.id as usize] = u32::try_from(i).unwrap_or(Self::NONE);
            }
            Self::Table(table)
        } else {
            Self::Map(
                photos
                    .iter()
                    .enumerate()
                    .map(|(i, p)| (p.id, u32::try_from(i).unwrap_or(Self::NONE)))
                    .collect(),
            )
        }
    }

    fn get(&self, id: u32) -> Option<usize> {
        let at = match self {
            Self::Table(table) => *table.get(id as usize)?,
            Self::Map(map) => *map.get(&id)?,
        };
        (at != Self::NONE).then_some(at as usize)
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
        format: text(18)?.unwrap_or_default(),
        has_duration: row.get::<_, i64>(19)? != 0,
    })
}

#[cfg(test)]
mod tests;
