//! Scan facts kept in memory for a view-only album
//! (docs/03-media-and-scanning.md, "View-only albums"; docs/02-data.md,
//! "Opening, locking and view-only").
//!
//! Nothing is written to the album database. Each row the scanner would
//! have written is held here instead: rows of files the database does not
//! know get ids above the database's largest, and rows it would have
//! marked missing are hidden. The album index (#12) reads these over the
//! database's rows.

use std::collections::HashMap;

use rusqlite::Connection;

use super::rows::{By, Facts, Known, Op, file_name, load_row, max_id};
use crate::error::AppResult;

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

    /// Applies one op. `conn` reads the database row an op changes the
    /// first time memory needs it.
    pub fn apply(&mut self, conn: &Connection, op: &Op) -> AppResult<()> {
        match op {
            Op::Unmiss { id } => {
                if let Some(row) = self.row(conn, By::Id(*id))? {
                    row.missing = false;
                }
            }
            Op::Missing { id } => {
                if let Some(row) = self.row(conn, By::Id(*id))? {
                    row.missing = true;
                }
            }
            Op::Move { id, rel_path } => {
                if let Some(row) = self.row(conn, By::Id(*id))? {
                    row.rel_path.clone_from(rel_path);
                    row.missing = false;
                }
            }
            Op::Facts {
                id: Some(id),
                facts,
                ..
            } => {
                if let Some(row) = self.row(conn, By::Id(*id))? {
                    row.facts = (**facts).clone();
                    row.missing = false;
                }
            }
            Op::Facts {
                id: None,
                rel_path,
                facts,
                ..
            } => {
                if let Some(row) = self.row(conn, By::Path(rel_path))? {
                    row.facts = (**facts).clone();
                    row.missing = false;
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
                }
            }
            Op::Pixels {
                rel_path,
                analysis,
                size,
            } => {
                if let Some(row) = self.row(conn, By::Path(rel_path))? {
                    row.facts.analysis = Some(analysis.clone());
                    if let Some((w, h)) = size {
                        row.facts.width = *w;
                        row.facts.height = *h;
                    }
                }
            }
            Op::Failed { rel_path, facts } => {
                if let Some(row) = self.row(conn, By::Path(rel_path))? {
                    row.facts = (**facts).clone();
                }
            }
        }
        Ok(())
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
