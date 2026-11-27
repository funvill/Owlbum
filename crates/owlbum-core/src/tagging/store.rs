//! The album's tagging rows (docs/07-tagging.md, "What is stored"; docs/02-
//! data.md): `tags`, `vectors`, and the derived `photo_labels` and search
//! columns. Writes run inside a writer transaction; reads take a read-only
//! connection.

use rusqlite::{Connection, OptionalExtension, Transaction, params};

use super::catalogue::Part;
use super::results::{self, KeywordScore, ObjectCount};
use crate::album::{search, sqlite_error};
use crate::error::AppResult;
use crate::text;

/// `tags.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Done,
    Failed,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Status::Done => "done",
            Status::Failed => "failed",
        }
    }
}

/// One `tags` row, with its vector for a done keywords row.
#[derive(Debug, Clone, PartialEq)]
pub struct TagRow {
    pub part: Part,
    pub model_id: String,
    pub status: Status,
    pub description: Option<String>,
    pub objects: Vec<ObjectCount>,
    pub keywords: Vec<KeywordScore>,
    pub error: Option<String>,
    pub made_at: i64,
    pub vector: Option<Vec<f32>>,
}

impl TagRow {
    /// A failed row.
    pub fn failed(part: Part, model_id: &str, error: &str, made_at: i64) -> Self {
        Self {
            part,
            model_id: model_id.to_owned(),
            status: Status::Failed,
            description: None,
            objects: Vec::new(),
            keywords: Vec::new(),
            error: Some(error.to_owned()),
            made_at,
            vector: None,
        }
    }
}

/// The SQL that keeps a photo row (`p`) eligible for tagging inside the
/// scope given as `?1`.
pub const ELIGIBLE: &str = "p.missing = 0 AND p.scan_error IS NULL \
     AND (?1 = '' OR p.rel_dir = ?1 OR substr(p.rel_dir, 1, length(?1) + 1) = ?1 || '/')";

/// Milliseconds since the epoch, now.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn failed(e: rusqlite::Error) -> crate::AppError {
    sqlite_error(text::album::WRITING_DATABASE)(e)
}

/// The other rows that hold the same picture: the same `file_key` and the
/// same dHash.
pub fn copies(conn: &Connection, id: i64) -> AppResult<Vec<i64>> {
    let mut st = conn
        .prepare_cached(
            "SELECT b.id FROM photos a JOIN photos b
               ON b.file_key = a.file_key AND b.id != a.id
              AND json_extract(b.analysis, '$.dhash') = json_extract(a.analysis, '$.dhash')
             WHERE a.id = ?1 AND json_extract(a.analysis, '$.dhash') IS NOT NULL
             ORDER BY b.id",
        )
        .map_err(failed)?;
    let ids = st
        .query_map([id], |r| r.get(0))
        .map_err(failed)?
        .collect::<Result<Vec<i64>, _>>()
        .map_err(failed)?;
    Ok(ids)
}

fn has_row(tx: &Connection, id: i64, model: &str) -> AppResult<bool> {
    tx.query_row(
        "SELECT 1 FROM tags WHERE photo_id = ?1 AND model_id = ?2",
        params![id, model],
        |_| Ok(()),
    )
    .optional()
    .map(|r| r.is_some())
    .map_err(failed)
}

fn insert(tx: &Transaction<'_>, id: i64, row: &TagRow) -> AppResult<()> {
    let objects =
        (!row.objects.is_empty()).then(|| serde_json::to_string(&row.objects).unwrap_or_default());
    let keywords = (!row.keywords.is_empty())
        .then(|| serde_json::to_string(&row.keywords).unwrap_or_default());
    tx.execute(
        "INSERT OR REPLACE INTO tags (photo_id, part, model_id, status, description, objects,
                                      keywords, error, made_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id,
            row.part.as_str(),
            row.model_id,
            row.status.as_str(),
            row.description,
            objects,
            keywords,
            row.error,
            row.made_at
        ],
    )
    .map_err(failed)?;
    match &row.vector {
        Some(v) if row.status == Status::Done => {
            tx.execute(
                "INSERT OR REPLACE INTO vectors (photo_id, model_id, dim, vector) VALUES (?1, ?2, ?3, ?4)",
                params![id, row.model_id, v.len() as i64, results::vector_bytes(v)],
            )
            .map_err(failed)?;
        }
        _ => {
            tx.execute(
                "DELETE FROM vectors WHERE photo_id = ?1 AND model_id = ?2",
                params![id, row.model_id],
            )
            .map_err(failed)?;
        }
    }
    Ok(())
}

/// Rewrites a photo's `photo_labels` from the newest done row of each part,
/// and its search entry.
pub fn relabel(tx: &Transaction<'_>, id: i64) -> AppResult<()> {
    tx.execute("DELETE FROM photo_labels WHERE photo_id = ?1", [id])
        .map_err(failed)?;
    for (part, column, kind) in [
        (Part::Keywords, "keywords", "keyword"),
        (Part::Describe, "objects", "object"),
    ] {
        let json: Option<String> = tx
            .query_row(
                &format!(
                    "SELECT {column} FROM tags WHERE photo_id = ?1 AND part = ?2 AND status = 'done'
                     ORDER BY made_at DESC LIMIT 1"
                ),
                params![id, part.as_str()],
                |r| r.get(0),
            )
            .optional()
            .map_err(failed)?
            .flatten();
        let labels: Vec<String> = json
            .and_then(|j| serde_json::from_str::<Vec<serde_json::Value>>(&j).ok())
            .unwrap_or_default()
            .iter()
            .filter_map(|v| v["label"].as_str().map(str::to_lowercase))
            .collect();
        for label in labels {
            tx.execute(
                "INSERT OR IGNORE INTO photo_labels (kind, label, photo_id) VALUES (?1, ?2, ?3)",
                params![kind, label, id],
            )
            .map_err(failed)?;
        }
    }
    search::reindex(tx, id)
}

/// Writes a photo's new rows, and the same rows for every copy that has no
/// row for that model; keeps labels and search in step. Answers every photo
/// written to.
pub fn write(tx: &Transaction<'_>, id: i64, rows: &[TagRow]) -> AppResult<Vec<i64>> {
    let exists: bool = tx
        .query_row("SELECT 1 FROM photos WHERE id = ?1", [id], |_| Ok(()))
        .optional()
        .map_err(failed)?
        .is_some();
    if !exists {
        return Ok(Vec::new());
    }
    let mut touched = vec![id];
    let twins = copies(tx, id)?;
    for row in rows {
        insert(tx, id, row)?;
        for &c in &twins {
            if !has_row(tx, c, &row.model_id)? {
                insert(tx, c, row)?;
                if !touched.contains(&c) {
                    touched.push(c);
                }
            }
        }
    }
    for &t in &touched {
        relabel(tx, t)?;
    }
    Ok(touched)
}

/// Gives a photo every row a copy of it has and it lacks, with no model run.
/// Answers the model ids it took.
pub fn take_from_copies(tx: &Transaction<'_>, id: i64) -> AppResult<Vec<String>> {
    let mut taken = Vec::new();
    for c in copies(tx, id)? {
        let models: Vec<String> = {
            let mut st = tx
                .prepare_cached("SELECT model_id FROM tags WHERE photo_id = ?1")
                .map_err(failed)?;
            st.query_map([c], |r| r.get(0))
                .map_err(failed)?
                .collect::<Result<_, _>>()
                .map_err(failed)?
        };
        for m in models {
            if has_row(tx, id, &m)? {
                continue;
            }
            tx.execute(
                "INSERT INTO tags (photo_id, part, model_id, status, description, objects, keywords,
                                   error, made_at)
                 SELECT ?1, part, model_id, status, description, objects, keywords, error, made_at
                 FROM tags WHERE photo_id = ?2 AND model_id = ?3",
                params![id, c, m],
            )
            .map_err(failed)?;
            tx.execute(
                "INSERT OR REPLACE INTO vectors (photo_id, model_id, dim, vector)
                 SELECT ?1, model_id, dim, vector FROM vectors WHERE photo_id = ?2 AND model_id = ?3",
                params![id, c, m],
            )
            .map_err(failed)?;
            taken.push(m);
        }
    }
    if !taken.is_empty() {
        relabel(tx, id)?;
    }
    Ok(taken)
}

/// The eligible photos in the scope that have no row for `model`, in id
/// order.
pub fn needs_work(conn: &Connection, model: &str, scope: &str) -> AppResult<Vec<i64>> {
    let mut st = conn
        .prepare(&format!(
            "SELECT p.id FROM photos p WHERE {ELIGIBLE}
               AND NOT EXISTS (SELECT 1 FROM tags t WHERE t.photo_id = p.id AND t.model_id = ?2)
             ORDER BY p.id"
        ))
        .map_err(failed)?;
    let ids = st
        .query_map(params![scope, model], |r| r.get(0))
        .map_err(failed)?
        .collect::<Result<Vec<i64>, _>>()
        .map_err(failed)?;
    Ok(ids)
}

/// For the status: eligible photos, those where every model has a done row,
/// and those where some model has a failed row. All zero with no model.
pub fn counts(conn: &Connection, models: &[&str], scope: &str) -> AppResult<(u64, u64, u64)> {
    if models.is_empty() {
        return Ok((0, 0, 0));
    }
    let list = models
        .iter()
        .map(|m| format!("'{}'", m.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(",");
    let n = models.len();
    conn.query_row(
        &format!(
            "SELECT count(*),
                    coalesce(sum(done = {n} AND bad = 0), 0),
                    coalesce(sum(bad > 0), 0)
             FROM (SELECT p.id,
                     (SELECT count(*) FROM tags t WHERE t.photo_id = p.id AND t.model_id IN ({list})
                        AND t.status = 'done') AS done,
                     (SELECT count(*) FROM tags t WHERE t.photo_id = p.id AND t.model_id IN ({list})
                        AND t.status = 'failed') AS bad
                   FROM photos p WHERE {ELIGIBLE})"
        ),
        params![scope],
        |r| {
            Ok((
                r.get::<_, i64>(0)? as u64,
                r.get::<_, i64>(1)? as u64,
                r.get::<_, i64>(2)? as u64,
            ))
        },
    )
    .map_err(failed)
}

/// Whether one photo is done, failed or neither for the given models.
pub fn photo_state(conn: &Connection, id: i64, models: &[&str]) -> AppResult<(bool, bool)> {
    let mut done = 0;
    let mut bad = false;
    for m in models {
        let status: Option<String> = conn
            .query_row(
                "SELECT status FROM tags WHERE photo_id = ?1 AND model_id = ?2",
                params![id, m],
                |r| r.get(0),
            )
            .optional()
            .map_err(failed)?;
        match status.as_deref() {
            Some("done") => done += 1,
            Some(_) => bad = true,
            None => {}
        }
    }
    Ok((done == models.len() && !bad, bad))
}

/// Deletes the failed rows of `models` for the eligible photos in the scope,
/// and their copies' failed rows. Answers how many photos were cleared.
pub fn retry_failed(tx: &Transaction<'_>, models: &[&str], scope: &str) -> AppResult<u64> {
    let mut cleared = std::collections::BTreeSet::new();
    for m in models {
        let ids: Vec<i64> = {
            let mut st = tx
                .prepare(&format!(
                    "SELECT p.id FROM photos p JOIN tags t ON t.photo_id = p.id
                     WHERE {ELIGIBLE} AND t.model_id = ?2 AND t.status = 'failed'"
                ))
                .map_err(failed)?;
            st.query_map(params![scope, m], |r| r.get(0))
                .map_err(failed)?
                .collect::<Result<_, _>>()
                .map_err(failed)?
        };
        for id in ids {
            let mut all = vec![id];
            all.extend(copies(tx, id)?);
            for a in all {
                let n = tx
                    .execute(
                        "DELETE FROM tags WHERE photo_id = ?1 AND model_id = ?2 AND status = 'failed'",
                        params![a, m],
                    )
                    .map_err(failed)?;
                if n > 0 && a == id {
                    cleared.insert(a);
                }
            }
        }
    }
    Ok(cleared.len() as u64)
}

/// The row of one photo for one model.
pub fn row(conn: &Connection, id: i64, model: &str) -> AppResult<Option<TagRow>> {
    conn.query_row(
        "SELECT part, model_id, status, description, objects, keywords, error, made_at
         FROM tags WHERE photo_id = ?1 AND model_id = ?2",
        params![id, model],
        |r| {
            let objects: Option<String> = r.get(4)?;
            let keywords: Option<String> = r.get(5)?;
            Ok(TagRow {
                part: Part::parse(&r.get::<_, String>(0)?).unwrap_or(Part::Describe),
                model_id: r.get(1)?,
                status: if r.get::<_, String>(2)? == "done" {
                    Status::Done
                } else {
                    Status::Failed
                },
                description: r.get(3)?,
                objects: objects
                    .and_then(|j| serde_json::from_str(&j).ok())
                    .unwrap_or_default(),
                keywords: keywords
                    .and_then(|j| serde_json::from_str(&j).ok())
                    .unwrap_or_default(),
                error: r.get(6)?,
                made_at: r.get(7)?,
                vector: None,
            })
        },
    )
    .optional()
    .map_err(failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::album::schema;

    fn db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        schema::migrate(&mut conn).unwrap();
        for (id, path, key, dhash, missing) in [
            (1, "a/lake.jpg", "k1", "00000000000000aa", 0),
            (2, "b/lake copy.jpg", "k1", "00000000000000aa", 0),
            (3, "a/harbour.jpg", "k3", "00000000000000bb", 0),
            (4, "a/gone.jpg", "k4", "00000000000000cc", 1),
            (
                5,
                "c/same key other picture.jpg",
                "k1",
                "00000000000000dd",
                0,
            ),
        ] {
            let (dir, name) = path.rsplit_once('/').unwrap();
            conn.execute(
                "INSERT INTO photos (id, rel_path, rel_dir, file_name, size, mtime_ms, file_key, thumb_key,
                                     kind, format, width, height, scan_version, analysis, missing)
                 VALUES (?1, ?2, ?3, ?4, 1, 1, ?5, 't', 'image', 'jpeg', 1, 1, 1, ?6, ?7)",
                params![id, path, dir, name, key, format!(r#"{{"dhash":"{dhash}"}}"#), missing],
            )
            .unwrap();
        }
        conn
    }

    fn done_keywords(model: &str, words: &[&str]) -> TagRow {
        TagRow {
            part: Part::Keywords,
            model_id: model.into(),
            status: Status::Done,
            description: None,
            objects: Vec::new(),
            keywords: words
                .iter()
                .map(|w| KeywordScore {
                    label: (*w).into(),
                    score: 0.5,
                })
                .collect(),
            error: None,
            made_at: 10,
            vector: Some(vec![0.6, 0.8]),
        }
    }

    #[test]
    fn a_result_is_copied_to_copies_with_labels_and_search() {
        let mut conn = db();
        let tx = conn.transaction().unwrap();
        let touched = write(&tx, 1, &[done_keywords("m", &["lake", "sky"])]).unwrap();
        tx.commit().unwrap();
        assert_eq!(touched, vec![1, 2], "5 shares the key but not the picture");
        let labels: i64 = conn
            .query_row(
                "SELECT count(*) FROM photo_labels WHERE kind = 'keyword'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(labels, 4);
        let vectors: i64 = conn
            .query_row("SELECT count(*) FROM vectors", [], |r| r.get(0))
            .unwrap();
        assert_eq!(vectors, 2);
        let hit: i64 = conn
            .query_row(
                "SELECT count(*) FROM search WHERE search MATCH 'keywords:lake'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hit, 2);
        assert_eq!(needs_work(&conn, "m", "").unwrap(), vec![3, 5]);
        assert_eq!(needs_work(&conn, "m", "a").unwrap(), vec![3]);
        assert_eq!(counts(&conn, &["m"], "").unwrap(), (4, 2, 0));
        assert_eq!(counts(&conn, &[], "").unwrap(), (0, 0, 0));
    }

    #[test]
    fn a_new_photo_takes_its_copys_rows() {
        let mut conn = db();
        conn.execute("UPDATE photos SET file_key = 'k9' WHERE id = 2", [])
            .unwrap();
        let tx = conn.transaction().unwrap();
        write(&tx, 1, &[done_keywords("m", &["lake"])]).unwrap();
        tx.execute("UPDATE photos SET file_key = 'k1' WHERE id = 2", [])
            .unwrap();
        assert_eq!(take_from_copies(&tx, 2).unwrap(), vec!["m".to_owned()]);
        assert!(take_from_copies(&tx, 2).unwrap().is_empty());
        tx.commit().unwrap();
        assert_eq!(
            row(&conn, 2, "m").unwrap().unwrap().keywords[0].label,
            "lake"
        );
    }

    #[test]
    fn failed_rows_count_and_clear_with_their_copies() {
        let mut conn = db();
        let tx = conn.transaction().unwrap();
        write(&tx, 1, &[TagRow::failed(Part::Describe, "d", "bad", 5)]).unwrap();
        write(&tx, 3, &[TagRow::failed(Part::Describe, "d", "bad", 5)]).unwrap();
        tx.commit().unwrap();
        assert_eq!(counts(&conn, &["d"], "").unwrap(), (4, 0, 3));
        assert_eq!(photo_state(&conn, 2, &["d"]).unwrap(), (false, true));
        let tx = conn.transaction().unwrap();
        assert_eq!(retry_failed(&tx, &["d"], "a").unwrap(), 2);
        tx.commit().unwrap();
        assert_eq!(counts(&conn, &["d"], "").unwrap(), (4, 0, 0));
    }
}
