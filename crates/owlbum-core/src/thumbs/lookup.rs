//! Which photo of the open album a picture URL means (docs/03, "On demand"
//! step 1, and "`owlbum://` URLs served").
//!
//! A photo is found only when its row is in the album's scope, is not
//! missing and has no scan error. In a view-only album the rows the scanner
//! holds in memory ([`Overlay`]) are read over the database's, keys
//! included.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use rusqlite::params;

use crate::album::{Album, sqlite_error};
use crate::error::AppResult;
use crate::media::{self, MediaFormat, MediaKind};
use crate::scan::memory::{MemoryRow, Overlay};
use crate::scan::rows::{self, By, Facts};
use crate::scan::walk;
use crate::text;

/// What serving a picture needs to know about its photo.
#[derive(Debug, Clone)]
pub struct Photo {
    pub id: i64,
    pub rel_path: String,
    /// The file on disk.
    pub path: PathBuf,
    pub kind: MediaKind,
    /// The registry row of what the file is; None for an unknown id.
    pub format: Option<&'static MediaFormat>,
    /// Set for video and for an animated still.
    pub duration_ms: Option<u64>,
    pub thumb_key: String,
    /// The pixels pass has not run yet.
    pub waiting: bool,
}

impl Photo {
    fn from(album: &Album, id: i64, rel_path: String, facts: &Facts) -> Self {
        Self {
            id,
            path: walk::absolute(&album.location().root, &rel_path),
            rel_path,
            kind: facts.kind,
            format: media::format_by_id(&facts.format),
            duration_ms: facts.duration_ms,
            thumb_key: facts.thumb_key.clone(),
            waiting: facts.analysis.is_none(),
        }
    }
}

/// True for a row a URL may serve.
pub(crate) fn servable(album: &Album, rel_path: &str, missing: bool, facts: &Facts) -> bool {
    !missing && facts.scan_error.is_none() && walk::is_under(rel_path, &album.location().scope)
}

fn from_memory(album: &Album, row: &MemoryRow) -> Option<Photo> {
    servable(album, &row.rel_path, row.missing, &row.facts)
        .then(|| Photo::from(album, row.id, row.rel_path.clone(), &row.facts))
}

fn lock(overlay: &Mutex<Overlay>) -> MutexGuard<'_, Overlay> {
    overlay.lock().unwrap_or_else(|p| p.into_inner())
}

/// The photo whose thumbnails are named `key`. Several rows can share a key
/// (two copies of one file); any servable one will do.
pub fn by_key(album: &Album, overlay: &Mutex<Overlay>, key: &str) -> AppResult<Option<Photo>> {
    let held: Vec<i64> = {
        let memory = lock(overlay);
        if let Some(p) = memory
            .rows()
            .filter(|r| r.facts.thumb_key == key)
            .find_map(|r| from_memory(album, r))
        {
            return Ok(Some(p));
        }
        memory.rows().map(|r| r.id).collect()
    };
    let failed = sqlite_error(text::album::READING_DATABASE);
    album.db().read(|conn| {
        let ids: Vec<i64> = conn
            .prepare_cached("SELECT id FROM photos WHERE thumb_key = ?1")
            .and_then(|mut st| {
                st.query_map(params![key], |r| r.get(0))?
                    .collect::<Result<_, _>>()
            })
            .map_err(&failed)?;
        for id in ids.into_iter().filter(|id| !held.contains(id)) {
            if let Some((id, rel_path, missing, facts)) = rows::load_row(conn, By::Id(id))?
                && servable(album, &rel_path, missing, &facts)
            {
                return Ok(Some(Photo::from(album, id, rel_path, &facts)));
            }
        }
        Ok(None)
    })
}

/// The photo with `id`.
pub fn by_id(album: &Album, overlay: &Mutex<Overlay>, id: i64) -> AppResult<Option<Photo>> {
    if let Some(row) = lock(overlay).get(id) {
        return Ok(from_memory(album, row));
    }
    album.db().read(|conn| {
        Ok(
            rows::load_row(conn, By::Id(id))?.and_then(|(id, rel_path, missing, facts)| {
                servable(album, &rel_path, missing, &facts)
                    .then(|| Photo::from(album, id, rel_path, &facts))
            }),
        )
    })
}

/// Every servable photo in scope whose pixels pass has run, for the warm
/// pass.
pub fn complete_in_scope(album: &Album, overlay: &Mutex<Overlay>) -> AppResult<Vec<Photo>> {
    let scope = album.location().scope.clone();
    let mut photos: Vec<Photo> = album.db().read(|conn| {
        let failed = sqlite_error(text::album::READING_DATABASE);
        conn.prepare(
            "SELECT id, rel_path, format, duration_ms, thumb_key FROM photos
             WHERE missing = 0 AND scan_error IS NULL AND analysis IS NOT NULL
               AND (?1 = '' OR substr(rel_path, 1, length(?1) + 1) = ?1 || '/')
             ORDER BY id",
        )
        .and_then(|mut st| {
            st.query_map(params![scope], |r| {
                let rel_path: String = r.get(1)?;
                let format = media::format_by_id(&r.get::<_, String>(2)?);
                Ok(Photo {
                    id: r.get(0)?,
                    path: walk::absolute(&album.location().root, &rel_path),
                    rel_path,
                    kind: format.map_or(MediaKind::Image, |f| f.kind),
                    format,
                    duration_ms: r.get::<_, Option<i64>>(3)?.map(|d| d.max(0).unsigned_abs()),
                    thumb_key: r.get(4)?,
                    waiting: false,
                })
            })?
            .collect::<Result<_, _>>()
        })
        .map_err(&failed)
    })?;
    let memory = lock(overlay);
    if !memory.is_empty() {
        photos.retain(|p| memory.get(p.id).is_none());
        photos.extend(
            memory
                .rows()
                .filter_map(|r| from_memory(album, r))
                .filter(|p| !p.waiting),
        );
        photos.sort_by_key(|p| p.id);
    }
    Ok(photos)
}
