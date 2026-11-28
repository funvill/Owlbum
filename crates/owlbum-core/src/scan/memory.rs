//! Scan facts kept in memory for a view-only album
//! (docs/03-media-and-scanning.md, "View-only albums"; docs/02-data.md,
//! "Opening, locking and view-only").
//!
//! Nothing is written to the album database. Each row the scanner would
//! have written is held here instead: rows of files the database does not
//! know get ids above the database's largest, and rows it would have
//! marked missing are hidden. Each row an op changes goes into the album
//! index straight away ([`MemoryRow::index_facts`], then
//! [`crate::index::AlbumIndex::upsert`] or `remove`), over the database's
//! rows the index loaded on open.

use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension};

use super::rows::{By, Facts, Known, Op, file_name, kind_text, load_row, max_id, rel_dir};
use crate::album::sqlite_error;
use crate::error::AppResult;
use crate::index::PhotoFacts;
use crate::text;

/// One row as the scanner left it in memory.
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryRow {
    pub id: i64,
    pub rel_path: String,
    /// Hidden: the file was not found (the database is not marked).
    pub missing: bool,
    pub facts: Facts,
}

impl MemoryRow {
    /// The row as the compare stage sees it.
    pub fn known(&self) -> Known {
        Known {
            id: self.id,
            rel_path: self.rel_path.clone(),
            size: self.facts.size,
            mtime_ms: self.facts.mtime_ms,
            file_key: self.facts.file_key.clone(),
            scan_version: self.facts.scan_version,
            scan_error: self.facts.scan_error.clone(),
            missing: self.missing,
            waiting: self.facts.analysis.is_none(),
            format: self.facts.format.clone(),
            width: self.facts.width,
            height: self.facts.height,
        }
    }

    /// The file name.
    pub fn file_name(&self) -> &str {
        file_name(&self.rel_path)
    }

    /// The row as the album index takes it: the scan facts from memory and
    /// the typed info (star, hidden, date, place, edit, people) from the
    /// database row with the same id, when there is one. None when the row
    /// is not in the index (missing, or a scan error).
    pub fn index_facts(&self, conn: &Connection) -> AppResult<Option<PhotoFacts>> {
        if self.missing || self.facts.scan_error.is_some() {
            return Ok(None);
        }
        let f = &self.facts;
        let mut facts = PhotoFacts {
            id: self.id,
            rel_dir: rel_dir(&self.rel_path).to_owned(),
            file_name: self.file_name().to_owned(),
            kind: kind_text(f.kind).to_owned(),
            captured_at: f.captured_at.clone(),
            width: i64::from(f.width),
            height: i64::from(f.height),
            gps: f.gps_lat.zip(f.gps_lon),
            analysis: f.analysis.clone(),
            format: f.format.clone(),
            has_duration: f.duration_ms.is_some(),
            ..PhotoFacts::default()
        };
        let typed = conn
            .query_row(
                "SELECT star, hidden, score_adjust, date_override, loc_lat, loc_lon,
                        edit IS NOT NULL,
                        (SELECT count(*) FROM photo_people WHERE photo_id = ?1)
                   FROM photos WHERE id = ?1",
                [self.id],
                |r| {
                    Ok(TypedInfo {
                        star: r.get::<_, i64>(0)? != 0,
                        hidden: r.get::<_, i64>(1)? != 0,
                        score_adjust: r.get(2)?,
                        date_override: r.get(3)?,
                        loc: r.get::<_, Option<f64>>(4)?.zip(r.get::<_, Option<f64>>(5)?),
                        has_edit: r.get::<_, i64>(6)? != 0,
                        people: r.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(sqlite_error(text::album::READING_DATABASE))?;
        if let Some(t) = typed {
            facts.star = t.star;
            facts.hidden = t.hidden;
            facts.score_adjust = t.score_adjust;
            facts.date_override = t.date_override;
            facts.loc = t.loc;
            facts.has_edit = t.has_edit;
            facts.people = u32::try_from(t.people).unwrap_or(0);
        }
        Ok(Some(facts))
    }
}

/// The typed info of a database row, for a row memory holds over it.
struct TypedInfo {
    star: bool,
    hidden: bool,
    score_adjust: i64,
    date_override: Option<String>,
    loc: Option<(f64, f64)>,
    has_edit: bool,
    people: i64,
}

/// The in-memory rows of one open view-only album, for the session.
#[derive(Debug, Default)]
pub struct Overlay {
    rows: HashMap<i64, MemoryRow>,
    next_id: Option<i64>,
}

impl Overlay {
    /// Every row held in memory, in no particular order. A row here replaces
    /// the database's row with the same id.
    pub fn rows(&self) -> impl Iterator<Item = &MemoryRow> {
        self.rows.values()
    }

    /// The row with `id`, if memory holds it.
    pub fn get(&self, id: i64) -> Option<&MemoryRow> {
        self.rows.get(&id)
    }

    /// True when nothing is held.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Applies one op and answers the id of the row it changed (None when
    /// the op's row was not found). `conn` reads the database row an op
    /// changes the first time memory needs it.
    pub fn apply(&mut self, conn: &Connection, op: &Op) -> AppResult<Option<i64>> {
        let changed = match op {
            Op::Unmiss { id } => self.row(conn, By::Id(*id))?.map(|row| {
                row.missing = false;
                row.id
            }),
            Op::Missing { id } => self.row(conn, By::Id(*id))?.map(|row| {
                row.missing = true;
                row.id
            }),
            Op::Move { id, rel_path } => self.row(conn, By::Id(*id))?.map(|row| {
                row.rel_path.clone_from(rel_path);
                row.missing = false;
                row.id
            }),
            Op::Facts {
                id: Some(id),
                facts,
                ..
            } => self.row(conn, By::Id(*id))?.map(|row| {
                row.facts = (**facts).clone();
                row.missing = false;
                row.id
            }),
            Op::Facts {
                id: None,
                rel_path,
                facts,
                ..
            } => {
                if let Some(row) = self.row(conn, By::Path(rel_path))? {
                    row.facts = (**facts).clone();
                    row.missing = false;
                    Some(row.id)
                } else {
                    let id = self.new_id(conn)?;
                    self.rows.insert(
                        id,
                        MemoryRow {
                            id,
                            rel_path: rel_path.clone(),
                            missing: false,
                            facts: (**facts).clone(),
                        },
                    );
                    Some(id)
                }
            }
            Op::Pixels {
                rel_path,
                analysis,
                size,
            } => self.row(conn, By::Path(rel_path))?.map(|row| {
                row.facts.analysis = Some(analysis.clone());
                if let Some((w, h)) = size {
                    row.facts.width = *w;
                    row.facts.height = *h;
                }
                row.id
            }),
            Op::Failed { rel_path, facts } => self.row(conn, By::Path(rel_path))?.map(|row| {
                row.facts = (**facts).clone();
                row.id
            }),
        };
        Ok(changed)
    }

    /// The row memory holds, loading it from the database the first time.
    fn row(&mut self, conn: &Connection, by: By<'_>) -> AppResult<Option<&mut MemoryRow>> {
        let held = match by {
            By::Id(id) => self.rows.contains_key(&id).then_some(id),
            By::Path(p) => self.rows.values().find(|r| r.rel_path == p).map(|r| r.id),
        };
        let id = match held {
            Some(id) => id,
            None => {
                let Some((id, rel_path, missing, facts)) = load_row(conn, by)? else {
                    return Ok(None);
                };
                // A database row whose path memory has since given to
                // another row is not this one.
                if matches!(by, By::Path(_)) && self.rows.contains_key(&id) {
                    return Ok(None);
                }
                self.rows.insert(
                    id,
                    MemoryRow {
                        id,
                        rel_path,
                        missing,
                        facts,
                    },
                );
                id
            }
        };
        Ok(self.rows.get_mut(&id))
    }

    fn new_id(&mut self, conn: &Connection) -> AppResult<i64> {
        let next = match self.next_id {
            Some(n) => n,
            None => max_id(conn)?.max(self.rows.keys().copied().max().unwrap_or(0)) + 1,
        };
        self.next_id = Some(next + 1);
        Ok(next)
    }
}
