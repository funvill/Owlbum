//! The scanner's view of `photos` rows, and the writes it makes to them.
//!
//! Scan threads never write: they describe each change as an [`Op`] and
//! hand it to a sink ([`super::sink`]), which applies it to the database on
//! the writer thread ([`apply`]) or, in a view-only album, to the
//! in-memory overlay ([`super::memory`]).

use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};

use crate::album::{search, sqlite_error};
use crate::error::AppResult;
use crate::media::MediaKind;
use crate::text;

/// What the compare stage knows about one row (docs/03, "Concurrency and
/// writes": one query for the scope into a hash map).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Known {
    pub id: i64,
    pub rel_path: String,
    pub size: u64,
    pub mtime_ms: i64,
    pub file_key: String,
    pub scan_version: i64,
    pub scan_error: Option<String>,
    pub missing: bool,
    /// `analysis IS NULL`: with no `scan_error`, the row waits for the
    /// pixels pass.
    pub waiting: bool,
    /// The registry id of what the file really is.
    pub format: String,
    /// The stored upright size, for the pixels pass to correct.
    pub width: u32,
    pub height: u32,
}

impl Known {
    /// The file name: the last component of `rel_path`.
    pub fn file_name(&self) -> &str {
        file_name(&self.rel_path)
    }
}

/// The last component of a relative path.
pub fn file_name(rel_path: &str) -> &str {
    rel_path.rsplit('/').next().unwrap_or(rel_path)
}

/// Everything before the last `/`; `""` for the root.
pub fn rel_dir(rel_path: &str) -> &str {
    rel_path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// Every scan fact of one row: what the facts pass writes, or an error
/// row's placeholders.
#[derive(Debug, Clone, PartialEq)]
pub struct Facts {
    pub size: u64,
    pub mtime_ms: i64,
    pub file_key: String,
    pub thumb_key: String,
    pub kind: MediaKind,
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub duration_ms: Option<u64>,
    /// Read by the metadata reader (`scan::metadata`).
    pub captured_at: Option<String>,
    pub gps_lat: Option<f64>,
    pub gps_lon: Option<f64>,
    pub camera: Option<String>,
    pub exif: Option<String>,
    /// None: waiting for the pixels pass (or an error row).
    pub analysis: Option<String>,
    pub scan_version: i64,
    /// `<code>: <sentence>` when the file could not be read.
    pub scan_error: Option<String>,
}

/// One change the scanner makes.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// A row found again: `missing` back to 0.
    Unmiss { id: i64 },
    /// A row given to a renamed or moved file: new path, all else kept.
    Move { id: i64, rel_path: String },
    /// New facts for a row (`id`), or a new row at `rel_path` when `id` is
    /// None. `rekey` deletes the photo's tags, vectors and labels (its
    /// file content changed). The row is no longer missing.
    Facts {
        id: Option<i64>,
        rel_path: String,
        facts: Box<Facts>,
        rekey: bool,
    },
    /// The pixels pass ended: the analysis, and the corrected size.
    Pixels {
        rel_path: String,
        analysis: String,
        size: Option<(u32, u32)>,
    },
    /// The pixels pass failed: the row becomes an error row.
    Failed { rel_path: String, facts: Box<Facts> },
    /// The file is gone: `missing = 1`.
    Missing { id: i64 },
}

/// What one applied op means for `album_changed`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Applied {
    /// The photo whose file content changed.
    pub rekeyed: Option<i64>,
    /// The photo whose row the op changed, for the album index; None when
    /// the op's row was not found.
    pub photo: Option<i64>,
}

const KNOWN_COLUMNS: &str = "id, rel_path, size, mtime_ms, file_key, scan_version, scan_error, \
     missing, analysis IS NULL, format, width, height";

fn known_from(r: &Row<'_>) -> rusqlite::Result<Known> {
    Ok(Known {
        id: r.get(0)?,
        rel_path: r.get(1)?,
        size: r.get::<_, i64>(2)?.max(0).unsigned_abs(),
        mtime_ms: r.get(3)?,
        file_key: r.get(4)?,
        scan_version: r.get(5)?,
        scan_error: r.get(6)?,
        missing: r.get(7)?,
        waiting: r.get(8)?,
        format: r.get(9)?,
        width: r.get(10)?,
        height: r.get(11)?,
    })
}

/// The SQL condition for rows in `scope` (`?1`): every row for the root,
/// else the scope folder's rows on a component boundary.
const IN_SCOPE: &str = "(?1 = '' OR substr(rel_path, 1, length(?1) + 1) = ?1 || '/')";

/// Every row in `scope`, in one query.
pub fn load_known(conn: &Connection, scope: &str) -> AppResult<Vec<Known>> {
    let failed = sqlite_error(text::album::READING_DATABASE);
    let mut st = conn
        .prepare(&format!(
            "SELECT {KNOWN_COLUMNS} FROM photos WHERE {IN_SCOPE}"
        ))
        .map_err(&failed)?;
    let rows = st
        .query_map([scope], known_from)
        .map_err(&failed)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(&failed)?;
    Ok(rows)
}

/// True when any row's path lies under the folder `rel_dir` (not `""`).
pub fn has_rows_under(conn: &Connection, rel_dir: &str) -> AppResult<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM photos
                       WHERE substr(rel_path, 1, length(?1) + 1) = ?1 || '/')",
        [rel_dir],
        |r| r.get(0),
    )
    .map_err(sqlite_error(text::album::READING_DATABASE))
}

/// The largest id in the database, 0 when there are no rows.
pub fn max_id(conn: &Connection) -> AppResult<i64> {
    conn.query_row("SELECT coalesce(max(id), 0) FROM photos", [], |r| r.get(0))
        .map_err(sqlite_error(text::album::READING_DATABASE))
}

/// One row's path, missing flag and scan facts, by id or by path.
pub fn load_row(conn: &Connection, by: By<'_>) -> AppResult<Option<(i64, String, bool, Facts)>> {
    let (cond, value): (&str, rusqlite::types::Value) = match by {
        By::Id(id) => ("id = ?1", id.into()),
        By::Path(p) => ("rel_path = ?1", p.to_owned().into()),
    };
    conn.query_row(
        &format!(
            "SELECT id, rel_path, missing, size, mtime_ms, file_key, thumb_key, kind, format,
                    width, height, duration_ms, captured_at, gps_lat, gps_lon, camera, exif,
                    analysis, scan_version, scan_error
             FROM photos WHERE {cond}"
        ),
        [value],
        |r| {
            let kind: String = r.get(7)?;
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                Facts {
                    size: r.get::<_, i64>(3)?.max(0).unsigned_abs(),
                    mtime_ms: r.get(4)?,
                    file_key: r.get(5)?,
                    thumb_key: r.get(6)?,
                    kind: kind_from(&kind),
                    format: r.get(8)?,
                    width: r.get(9)?,
                    height: r.get(10)?,
                    duration_ms: r
                        .get::<_, Option<i64>>(11)?
                        .map(|d| d.max(0).unsigned_abs()),
                    captured_at: r.get(12)?,
                    gps_lat: r.get(13)?,
                    gps_lon: r.get(14)?,
                    camera: r.get(15)?,
                    exif: r.get(16)?,
                    analysis: r.get(17)?,
                    scan_version: r.get(18)?,
                    scan_error: r.get(19)?,
                },
            ))
        },
    )
    .optional()
    .map_err(sqlite_error(text::album::READING_DATABASE))
}

/// How [`load_row`] finds a row.
#[derive(Debug, Clone, Copy)]
pub enum By<'a> {
    Id(i64),
    Path(&'a str),
}

/// `photos.kind` as stored.
pub fn kind_text(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Image => "image",
        MediaKind::Raw => "raw",
        MediaKind::Video => "video",
    }
}

fn kind_from(text: &str) -> MediaKind {
    match text {
        "raw" => MediaKind::Raw,
        "video" => MediaKind::Video,
        _ => MediaKind::Image,
    }
}

fn to_i64(n: u64) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// Applies one op in the writer's transaction, keeping `search` in step.
pub fn apply(tx: &Transaction<'_>, op: &Op) -> AppResult<Applied> {
    let failed = sqlite_error(text::album::WRITING_DATABASE);
    let mut applied = Applied::default();
    match op {
        Op::Unmiss { id } => {
            tx.execute("UPDATE photos SET missing = 0 WHERE id = ?1", [id])
                .map_err(&failed)?;
            applied.photo = Some(*id);
        }
        Op::Missing { id } => {
            tx.execute("UPDATE photos SET missing = 1 WHERE id = ?1", [id])
                .map_err(&failed)?;
            applied.photo = Some(*id);
        }
        Op::Move { id, rel_path } => {
            tx.execute(
                "UPDATE photos SET rel_path = ?2, rel_dir = ?3, file_name = ?4, missing = 0
                 WHERE id = ?1",
                params![id, rel_path, rel_dir(rel_path), file_name(rel_path)],
            )
            .map_err(&failed)?;
            search::reindex(tx, *id)?;
            applied.photo = Some(*id);
        }
        Op::Facts {
            id,
            rel_path,
            facts,
            rekey,
        } => {
            let id = match id {
                Some(id) => {
                    write_facts(tx, "id = ?1", &(*id).into(), facts)?;
                    *id
                }
                None => insert(tx, rel_path, facts)?,
            };
            if *rekey {
                for table in ["tags", "vectors", "photo_labels"] {
                    tx.execute(&format!("DELETE FROM {table} WHERE photo_id = ?1"), [id])
                        .map_err(&failed)?;
                }
                applied.rekeyed = Some(id);
            }
            search::reindex(tx, id)?;
            applied.photo = Some(id);
        }
        Op::Pixels {
            rel_path,
            analysis,
            size,
        } => {
            let (w, h) = size.map_or((None, None), |(w, h)| (Some(w), Some(h)));
            tx.execute(
                "UPDATE photos SET analysis = ?2, width = coalesce(?3, width),
                        height = coalesce(?4, height)
                 WHERE rel_path = ?1",
                params![rel_path, analysis, w, h],
            )
            .map_err(&failed)?;
            applied.photo = id_at(tx, rel_path)?;
        }
        Op::Failed { rel_path, facts } => {
            write_facts(tx, "rel_path = ?1", &rel_path.clone().into(), facts)?;
            applied.photo = id_at(tx, rel_path)?;
        }
    }
    Ok(applied)
}

/// The id of the row at `rel_path`.
fn id_at(tx: &Transaction<'_>, rel_path: &str) -> AppResult<Option<i64>> {
    tx.query_row(
        "SELECT id FROM photos WHERE rel_path = ?1",
        [rel_path],
        |r| r.get(0),
    )
    .optional()
    .map_err(sqlite_error(text::album::WRITING_DATABASE))
}

fn write_facts(
    tx: &Transaction<'_>,
    cond: &str,
    key: &rusqlite::types::Value,
    f: &Facts,
) -> AppResult<()> {
    tx.execute(
        &format!(
            "UPDATE photos SET missing = 0, size = ?2, mtime_ms = ?3, file_key = ?4, thumb_key = ?5,
                    kind = ?6, format = ?7, width = ?8, height = ?9, duration_ms = ?10,
                    captured_at = ?11, gps_lat = ?12, gps_lon = ?13, camera = ?14, exif = ?15,
                    analysis = ?16, scan_version = ?17, scan_error = ?18
             WHERE {cond}"
        ),
        params![
            key,
            to_i64(f.size),
            f.mtime_ms,
            f.file_key,
            f.thumb_key,
            kind_text(f.kind),
            f.format,
            f.width,
            f.height,
            f.duration_ms.map(to_i64),
            f.captured_at,
            f.gps_lat,
            f.gps_lon,
            f.camera,
            f.exif,
            f.analysis,
            f.scan_version,
            f.scan_error,
        ],
    )
    .map_err(sqlite_error(text::album::WRITING_DATABASE))?;
    Ok(())
}

fn insert(tx: &Transaction<'_>, rel_path: &str, f: &Facts) -> AppResult<i64> {
    let failed = sqlite_error(text::album::WRITING_DATABASE);
    let existing: Option<i64> = tx
        .query_row(
            "SELECT id FROM photos WHERE rel_path = ?1",
            [rel_path],
            |r| r.get(0),
        )
        .optional()
        .map_err(&failed)?;
    if let Some(id) = existing {
        // Another writer listed this path first (a watcher's scan); the
        // newer facts win and the row keeps its id.
        write_facts(tx, "id = ?1", &id.into(), f)?;
        return Ok(id);
    }
    tx.execute(
        "INSERT INTO photos (rel_path, rel_dir, file_name, size, mtime_ms, file_key, thumb_key,
                             kind, format, width, height, duration_ms, captured_at, gps_lat,
                             gps_lon, camera, exif, analysis, scan_version, scan_error)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                 ?18, ?19, ?20)",
        params![
            rel_path,
            rel_dir(rel_path),
            file_name(rel_path),
            to_i64(f.size),
            f.mtime_ms,
            f.file_key,
            f.thumb_key,
            kind_text(f.kind),
            f.format,
            f.width,
            f.height,
            f.duration_ms.map(to_i64),
            f.captured_at,
            f.gps_lat,
            f.gps_lon,
            f.camera,
            f.exif,
            f.analysis,
            f.scan_version,
            f.scan_error,
        ],
    )
    .map_err(&failed)?;
    Ok(tx.last_insert_rowid())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::album::schema;

    #[test]
    fn paths_split_into_folder_and_name() {
        assert_eq!(rel_dir("a/b/c.jpg"), "a/b");
        assert_eq!(file_name("a/b/c.jpg"), "c.jpg");
        assert_eq!(rel_dir("c.jpg"), "");
        assert_eq!(file_name("c.jpg"), "c.jpg");
    }

    fn facts() -> Facts {
        Facts {
            size: 10,
            mtime_ms: 20,
            file_key: "fk1:10:ab".into(),
            thumb_key: "t".into(),
            kind: MediaKind::Image,
            format: "png".into(),
            width: 4,
            height: 3,
            duration_ms: None,
            captured_at: None,
            gps_lat: None,
            gps_lon: None,
            camera: None,
            exif: None,
            analysis: None,
            scan_version: 1,
            scan_error: None,
        }
    }

    #[test]
    fn scope_matching_is_on_component_boundaries() {
        let mut conn = Connection::open_in_memory().unwrap();
        schema::migrate(&mut conn).unwrap();
        let tx = conn.transaction().unwrap();
        for p in ["2024/a.png", "2024/x/b.png", "2024-old/c.png", "d.png"] {
            apply(
                &tx,
                &Op::Facts {
                    id: None,
                    rel_path: p.into(),
                    facts: Box::new(facts()),
                    rekey: false,
                },
            )
            .unwrap();
        }
        tx.commit().unwrap();
        let paths = |scope: &str| {
            let mut v: Vec<String> = load_known(&conn, scope)
                .unwrap()
                .into_iter()
                .map(|k| k.rel_path)
                .collect();
            v.sort();
            v
        };
        assert_eq!(paths("2024"), ["2024/a.png", "2024/x/b.png"]);
        assert_eq!(paths("").len(), 4);
        assert_eq!(max_id(&conn).unwrap(), 4);
        let (id, path, missing, f) = load_row(&conn, By::Path("d.png")).unwrap().unwrap();
        assert_eq!((id, path.as_str(), missing), (4, "d.png", false));
        assert_eq!(f, facts());
        assert!(load_row(&conn, By::Id(99)).unwrap().is_none());
    }
}
