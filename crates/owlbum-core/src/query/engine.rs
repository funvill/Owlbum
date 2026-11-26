//! The query engine: runs queries over the album index and keeps their
//! results under tokens (docs/05-organising.md, "How Rust runs a query").

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use rusqlite::OptionalExtension;

use super::filter::{self, Content, Matcher};
use super::keep::settle;
use super::shape::{Facets, Filter, Keep, Located, Query, QueryResult, Row};
use super::{sort, support};
use crate::album::{Album, sqlite_error};
use crate::error::{AppError, AppResult, ErrorCode};
use crate::index::Kind;
use crate::media::MediaKind;
use crate::scan::memory::Overlay;
use crate::text;

/// How many results are kept. An older token answers `query_expired`.
pub const TOKENS_KEPT: usize = 4;

/// The most rows one `query_window` answers; a larger `limit` is taken as
/// this.
pub const WINDOW_MAX: u32 = 500;

/// Numbers every token in the process, so a token of one album (or of an
/// album opened earlier) never names another's result.
static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);

/// One result kept under its token.
#[derive(Debug)]
struct Held {
    token: String,
    /// Photo ids, one per tile, in order.
    tiles: Vec<u32>,
    /// Photos folded inside a closed stack, to the stack's id. Empty until
    /// stacks are built (#30).
    folded: HashMap<u32, i64>,
    /// The normalised filter the result was made with, and its people and
    /// tag ids, for `query_facets`.
    filter: Filter,
    content: Content,
}

/// Runs queries for one open album and keeps the last [`TOKENS_KEPT`]
/// results. The shell holds one per open album, so tokens die with it.
#[derive(Debug, Default)]
pub struct QueryEngine {
    held: Mutex<VecDeque<Arc<Held>>>,
}

/// The display facts of a row that the album index does not keep.
struct Display {
    rel_path: String,
    file_name: String,
    thumb_key: String,
    kind: MediaKind,
    width: u32,
    height: u32,
    duration_ms: Option<u64>,
    star: bool,
    hidden: bool,
}

/// What a row takes from the album index.
#[derive(Clone, Copy)]
struct Indexed {
    width: u32,
    height: u32,
    kind: Kind,
    star: bool,
    hidden: bool,
    score: Option<i32>,
}

impl QueryEngine {
    /// An engine holding no results.
    pub fn new() -> Self {
        Self::default()
    }

    /// `query_run`: works out the ordered tiles of `query`, keeps them
    /// under a new token, and settles `keep` against them.
    pub fn run(&self, album: &Album, query: &Query, keep: Option<&Keep>) -> AppResult<QueryResult> {
        support::check(query)?;
        let filter = query.filter.normalised();
        filter::validate(&filter)?;
        let content = filter::content(album, &filter)?;
        // Read before the index: a change landing in between makes this
        // result newer than its revision says, never older, so the page's
        // "is the event newer" test can only re-run once too often.
        let revision = album.revision();
        // The old result is looked up before the new one is stored, which
        // may drop it.
        let old = keep.and_then(|k| self.find(&k.token));
        let (tiles, album_photos, total, facets, chips) = album.index().read(|v| {
            let matcher = Matcher::new(v, &filter, &content);
            let passed = filter::pass(v, &matcher, query.want_facets);
            (
                sort::order(v, &passed.shown, query.sort),
                v.len(),
                passed.total,
                passed.facets,
                filter::chips(&filter, v.scope()),
            )
        });
        // Every photo that passes; stacks unfolded once they exist (#30).
        let photos = count(tiles.len());
        let kept = keep.map(|k| settle(k, old.as_deref().map(|h| h.tiles.as_slice()), &tiles));
        let count = count(tiles.len());
        let token = format!("q{}", NEXT_TOKEN.fetch_add(1, Ordering::Relaxed));
        self.store(Held {
            token: token.clone(),
            tiles,
            folded: HashMap::new(),
            filter,
            content,
        });
        Ok(QueryResult {
            token,
            revision,
            count,
            photos,
            total: self::count(total),
            album_photos: self::count(album_photos),
            filtered: !query.filter.is_empty() || support::is_search(query.search.as_deref()),
            panel_count: self::count(chips.len()),
            chips,
            kept,
            facets,
            ..QueryResult::default()
        })
    }

    /// `query_facets`: the counts beside the filter panel's options for the
    /// query `token` was made from, over the album index as it is now.
    pub fn facets(&self, album: &Album, token: &str) -> AppResult<Facets> {
        let held = self.held(token)?;
        Ok(album.index().read(|v| {
            let matcher = Matcher::new(v, &held.filter, &held.content);
            filter::pass(v, &matcher, true).facets.unwrap_or_default()
        }))
    }

    /// `query_window`: the rows at `offset`, at most `limit` and at most
    /// [`WINDOW_MAX`]. Fewer only at the end of the result.
    ///
    /// The rows are read as they are now. A row whose photo left the
    /// database since the run (Tidy) keeps its place as a stand-in with
    /// empty texts, so the window's length never changes under the page;
    /// the page re-runs on the `album_changed` that follows.
    pub fn window(
        &self,
        album: &Album,
        overlay: &Overlay,
        token: &str,
        offset: u32,
        limit: u32,
    ) -> AppResult<Vec<Row>> {
        let held = self.held(token)?;
        let start = (offset as usize).min(held.tiles.len());
        let end = start
            .saturating_add(limit.min(WINDOW_MAX) as usize)
            .min(held.tiles.len());
        let ids = &held.tiles[start..end];
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        // A row held in memory (a view-only album) replaces the
        // database's row with the same id.
        let mut display: Vec<Option<Display>> = ids
            .iter()
            .map(|&id| {
                overlay
                    .get(i64::from(id))
                    .filter(|r| !r.missing)
                    .map(|r| Display {
                        rel_path: r.rel_path.clone(),
                        file_name: r.file_name().to_owned(),
                        thumb_key: r.facts.thumb_key.clone(),
                        kind: r.facts.kind,
                        width: r.facts.width,
                        height: r.facts.height,
                        duration_ms: r.facts.duration_ms,
                        star: false,
                        hidden: false,
                    })
            })
            .collect();
        let indexed: Vec<Option<Indexed>> = album.index().read(|v| {
            ids.iter()
                .map(|&id| {
                    v.get(id).map(|p| Indexed {
                        width: p.width,
                        height: p.height,
                        kind: p.kind,
                        star: p.star(),
                        hidden: p.hidden(),
                        score: p.derived.score,
                    })
                })
                .collect()
        });
        album.db().read(|conn| {
            let failed = sqlite_error(text::query::READING_ROWS);
            // One read transaction for the whole window. Without it each
            // lookup is its own transaction, which takes and drops the
            // file's shared lock: 200 rows took 20 to 30 ms at 100,000
            // photos on Windows, against 0.4 ms in one (#26).
            let tx = conn.unchecked_transaction().map_err(&failed)?;
            let mut stmt = tx
                .prepare_cached(
                    "SELECT rel_path, file_name, thumb_key, kind, width, height, duration_ms, star, hidden
                       FROM photos WHERE id = ?1",
                )
                .map_err(&failed)?;
            for (slot, &id) in display.iter_mut().zip(ids) {
                if slot.is_some() {
                    continue;
                }
                *slot = stmt
                    .query_row([i64::from(id)], |r| {
                        Ok(Display {
                            rel_path: r.get(0)?,
                            file_name: r.get(1)?,
                            thumb_key: r.get(2)?,
                            kind: media_kind(&r.get::<_, String>(3)?),
                            width: u32::try_from(r.get::<_, i64>(4)?).unwrap_or(0),
                            height: u32::try_from(r.get::<_, i64>(5)?).unwrap_or(0),
                            duration_ms: r
                                .get::<_, Option<i64>>(6)?
                                .and_then(|d| u64::try_from(d).ok()),
                            star: r.get::<_, i64>(7)? != 0,
                            hidden: r.get::<_, i64>(8)? != 0,
                        })
                    })
                    .optional()
                    .map_err(&failed)?;
            }
            drop(stmt);
            tx.commit().map_err(&failed)?;
            Ok(())
        })?;

        Ok(ids
            .iter()
            .zip(display)
            .zip(indexed)
            .map(|((&id, display), indexed)| row(id, display, indexed))
            .collect())
    }

    /// `query_ids`: the ids of the tiles at `offset`, at most `limit`, in
    /// order. There is no cap: Ctrl+A asks for the whole result at once.
    /// Fewer only at the end of the result.
    pub fn ids(&self, token: &str, offset: u32, limit: u32) -> AppResult<Vec<i64>> {
        let held = self.held(token)?;
        let start = (offset as usize).min(held.tiles.len());
        let end = start.saturating_add(limit as usize).min(held.tiles.len());
        Ok(held.tiles[start..end]
            .iter()
            .map(|&id| i64::from(id))
            .collect())
    }

    /// `query_locate`: where photo `id` sits in a result; None when it is
    /// not in it.
    pub fn locate(&self, token: &str, id: i64) -> AppResult<Option<Located>> {
        let held = self.held(token)?;
        let Ok(tile) = u32::try_from(id) else {
            return Ok(None);
        };
        if let Some(i) = held.tiles.iter().position(|&t| t == tile) {
            return Ok(Some(Located::At { index: count(i) }));
        }
        Ok(held
            .folded
            .get(&tile)
            .map(|&stack| Located::Folded { folded_in: stack }))
    }

    fn lock(&self) -> MutexGuard<'_, VecDeque<Arc<Held>>> {
        self.held.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn find(&self, token: &str) -> Option<Arc<Held>> {
        self.lock().iter().find(|h| h.token == token).cloned()
    }

    fn held(&self, token: &str) -> AppResult<Arc<Held>> {
        self.find(token)
            .ok_or_else(|| AppError::new(ErrorCode::QueryExpired, text::query::EXPIRED))
    }

    fn store(&self, held: Held) {
        let mut all = self.lock();
        all.push_back(Arc::new(held));
        while all.len() > TOKENS_KEPT {
            all.pop_front();
        }
    }
}

/// Builds one row. The album index is preferred for what it holds, so a
/// row agrees with the result it came from.
fn row(id: u32, display: Option<Display>, indexed: Option<Indexed>) -> Row {
    let d = display.unwrap_or(Display {
        rel_path: String::new(),
        file_name: String::new(),
        thumb_key: String::new(),
        kind: MediaKind::Image,
        width: 0,
        height: 0,
        duration_ms: None,
        star: false,
        hidden: false,
    });
    let (width, height) = indexed.map_or((d.width, d.height), |i| (i.width, i.height));
    Row {
        id: i64::from(id),
        rel_path: d.rel_path,
        file_name: d.file_name,
        thumb_key: d.thumb_key,
        aspect: if width > 0 && height > 0 {
            f64::from(width) / f64::from(height)
        } else {
            1.0
        },
        kind: indexed.map_or(d.kind, |i| match i.kind {
            Kind::Image => MediaKind::Image,
            Kind::Raw => MediaKind::Raw,
            Kind::Video => MediaKind::Video,
        }),
        duration_ms: d.duration_ms,
        star: indexed.map_or(d.star, |i| i.star),
        hidden: indexed.map_or(d.hidden, |i| i.hidden),
        score: indexed.and_then(|i| i.score),
        stack: None,
        edit_plan: None,
    }
}

/// `photos.kind` as the page names it; anything unknown is an image, as in
/// the album index.
fn media_kind(kind: &str) -> MediaKind {
    match kind {
        "raw" => MediaKind::Raw,
        "video" => MediaKind::Video,
        _ => MediaKind::Image,
    }
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}
