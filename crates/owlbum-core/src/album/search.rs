//! Keeping the full-text `search` table in step (docs/02-data.md, "Schema").
//!
//! `search` is a contentless FTS5 table (`rowid = photos.id`): it cannot
//! change one column of a row, so every change rewrites the photo's whole
//! entry from its source fields with [`reindex`], in the transaction that
//! changed them.

use rusqlite::{Transaction, params};

use crate::album::sqlite_error;
use crate::error::AppResult;
use crate::text;

/// Rewrites photo `id`'s entry in `search` from its row, its people and its
/// tags. A photo that no longer exists loses its entry.
///
/// - `file_name`, `description`, `notes` from the row;
/// - `people`: the names of its people, space-separated;
/// - `place`: `loc_name`;
/// - `tag_description`: the description of its newest `done` describe row;
/// - `keywords`, `objects`: its `photo_labels`, space-separated.
pub fn reindex(tx: &Transaction<'_>, id: i64) -> AppResult<()> {
    let failed = sqlite_error(text::album::WRITING_DATABASE);
    tx.execute("DELETE FROM search WHERE rowid = ?1", [id])
        .map_err(&failed)?;
    tx.execute(
        "INSERT INTO search (rowid, file_name, description, notes, people, place,
                             tag_description, keywords, objects)
         SELECT p.id, p.file_name, p.description, p.notes,
                (SELECT group_concat(n.name, ' ') FROM photo_people pp
                   JOIN people n ON n.id = pp.person_id WHERE pp.photo_id = p.id),
                p.loc_name,
                (SELECT t.description FROM tags t
                   WHERE t.photo_id = p.id AND t.part = 'describe' AND t.status = 'done'
                   ORDER BY t.made_at DESC LIMIT 1),
                (SELECT group_concat(l.label, ' ') FROM photo_labels l
                   WHERE l.photo_id = p.id AND l.kind = 'keyword'),
                (SELECT group_concat(l.label, ' ') FROM photo_labels l
                   WHERE l.photo_id = p.id AND l.kind = 'object')
         FROM photos p WHERE p.id = ?1",
        params![id],
    )
    .map_err(&failed)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::album::schema;
    use rusqlite::Connection;

    fn db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        schema::migrate(&mut conn).unwrap();
        conn.execute_batch(
            "INSERT INTO photos (id, rel_path, rel_dir, file_name, size, mtime_ms, file_key, thumb_key,
                                 kind, format, width, height, scan_version, notes, loc_name)
             VALUES (1, 'a/beach.jpg', 'a', 'beach.jpg', 1, 1, 'k', 't', 'image', 'jpeg', 1, 1, 1,
                     'sunny day', 'Nice');
             INSERT INTO people (id, name) VALUES (1, 'Ann'), (2, 'Bob');
             INSERT INTO photo_people VALUES (1, 1), (1, 2);
             INSERT INTO tags (photo_id, part, model_id, status, description, made_at)
               VALUES (1, 'describe', 'm', 'done', 'a dog on sand', 5);
             INSERT INTO photo_labels VALUES ('keyword', 'dog', 1), ('object', 'ball', 1);",
        )
        .unwrap();
        conn
    }

    fn hits(conn: &Connection, q: &str) -> Vec<i64> {
        let mut st = conn
            .prepare("SELECT rowid FROM search WHERE search MATCH ?1")
            .unwrap();
        st.query_map([q], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    #[test]
    fn every_source_field_is_searchable() {
        let mut conn = db();
        let tx = conn.transaction().unwrap();
        reindex(&tx, 1).unwrap();
        tx.commit().unwrap();
        for q in [
            "file_name:beach",
            "notes:sunny",
            "people:bob",
            "place:nice",
            "tag_description:sand",
            "keywords:dog",
            "objects:ball",
        ] {
            assert_eq!(hits(&conn, q), [1], "{q}");
        }
    }

    #[test]
    fn reindexing_replaces_the_old_entry() {
        let mut conn = db();
        let tx = conn.transaction().unwrap();
        reindex(&tx, 1).unwrap();
        tx.execute(
            "UPDATE photos SET file_name = 'lake.jpg', rel_path = 'lake.jpg' WHERE id = 1",
            [],
        )
        .unwrap();
        reindex(&tx, 1).unwrap();
        tx.commit().unwrap();
        assert!(hits(&conn, "beach").is_empty());
        assert_eq!(hits(&conn, "lake"), [1]);
    }

    #[test]
    fn a_photo_that_is_gone_loses_its_entry_and_a_first_index_is_fine() {
        let mut conn = db();
        let tx = conn.transaction().unwrap();
        reindex(&tx, 7).unwrap(); // no such photo, no entry yet
        reindex(&tx, 1).unwrap();
        tx.execute("DELETE FROM photos WHERE id = 1", []).unwrap();
        reindex(&tx, 1).unwrap();
        tx.commit().unwrap();
        assert!(hits(&conn, "beach").is_empty());
    }
}
