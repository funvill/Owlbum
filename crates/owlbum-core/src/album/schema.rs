//! The album database's schema and its migrations (docs/02-data.md,
//! "Schema").
//!
//! `PRAGMA user_version` holds the schema version, starting at 1. Each
//! migration is a numbered SQL script compiled into the program. Opening runs
//! every script newer than the file's version, the version bump, and (for a
//! new database) the first `meta` rows, all in one transaction: a failure
//! leaves the file as it was.

use rusqlite::{Connection, TransactionBehavior};

use crate::album::sqlite_error;
use crate::error::AppResult;
use crate::text;

/// One numbered migration script.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    /// The schema version the script brings the database to. Versions start
    /// at 1 and go up by one.
    pub version: u32,
    /// The SQL, run as one batch.
    pub sql: &'static str,
}

/// Every migration, in order. Add the next script at the end; never edit
/// one that has shipped.
pub const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    sql: include_str!("migrations/0001_initial.sql"),
}];

/// The newest schema version this program knows.
pub const CURRENT_VERSION: u32 = latest(MIGRATIONS);

const fn latest(migrations: &[Migration]) -> u32 {
    match migrations.last() {
        Some(m) => m.version,
        None => 0,
    }
}

/// What opening found about the schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaState {
    /// The schema is current. `from` is the version the file had before this
    /// open: 0 for a new database, equal to `version` when nothing ran.
    Current {
        /// The schema version now.
        version: u32,
        /// The schema version before this open.
        from: u32,
    },
    /// The file was made by a newer Owlbum. Nothing was written; the album
    /// must open view-only (#4: "This album was last used by a newer
    /// Owlbum.").
    Newer {
        /// The file's schema version.
        version: u32,
    },
}

/// Facts written to `meta` when a database is created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAlbumMeta {
    /// A UUID made at creation. It names the album in thumbnail keys.
    pub album_id: String,
    /// Milliseconds since the Unix epoch, as decimal text.
    pub created_at: String,
    /// The version of the Owlbum that made the database.
    pub created_by: String,
}

impl NewAlbumMeta {
    /// Fresh facts for an album made now by this program.
    pub fn now() -> Self {
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        Self {
            album_id: uuid::Uuid::new_v4().to_string(),
            created_at: ms.to_string(),
            created_by: crate::app::version().version,
        }
    }
}

/// Reads `PRAGMA user_version`.
pub fn version(conn: &Connection) -> AppResult<u32> {
    conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
        .map(|v| u32::try_from(v).unwrap_or(u32::MAX))
        .map_err(sqlite_error(text::album::OPENING_DATABASE))
}

/// Brings the schema up to date with [`MIGRATIONS`].
pub fn migrate(conn: &mut Connection) -> AppResult<SchemaState> {
    migrate_with(conn, MIGRATIONS, NewAlbumMeta::now)
}

/// Brings the schema up to date with `migrations`. `meta` is asked for the
/// first `meta` rows only when the database is new (version 0).
///
/// A file whose version is newer than the last migration is left untouched
/// and reported as [`SchemaState::Newer`].
pub fn migrate_with(
    conn: &mut Connection,
    migrations: &[Migration],
    meta: impl FnOnce() -> NewAlbumMeta,
) -> AppResult<SchemaState> {
    let known = latest(migrations);
    let from = version(conn)?;
    if from > known {
        return Ok(SchemaState::Newer { version: from });
    }
    if from == known {
        return Ok(SchemaState::Current {
            version: from,
            from,
        });
    }

    let what = if from == 0 {
        text::album::CREATING_DATABASE
    } else {
        text::album::UPGRADING_DATABASE
    };
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sqlite_error(what))?;
    // Another program may have migrated between the read above and the
    // write lock; read again inside the transaction.
    let from = version(&tx)?;
    if from > known {
        return Ok(SchemaState::Newer { version: from });
    }
    for m in migrations.iter().filter(|m| m.version > from) {
        tx.execute_batch(m.sql).map_err(sqlite_error(what))?;
    }
    if from == 0 {
        let meta = meta();
        for (key, value) in [
            ("album_id", meta.album_id),
            ("created_at", meta.created_at),
            ("created_by", meta.created_by),
        ] {
            tx.execute(
                "INSERT OR IGNORE INTO meta (key, value) VALUES (?1, ?2)",
                (key, value),
            )
            .map_err(sqlite_error(what))?;
        }
    }
    tx.pragma_update(None, "user_version", known)
        .map_err(sqlite_error(what))?;
    tx.commit().map_err(sqlite_error(what))?;
    Ok(SchemaState::Current {
        version: known,
        from,
    })
}

/// Reads one `meta` value.
pub fn meta(conn: &Connection, key: &str) -> AppResult<Option<String>> {
    use rusqlite::OptionalExtension;
    conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
        .optional()
        .map_err(sqlite_error(text::album::READING_DATABASE))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorCode;

    fn fixed_meta() -> NewAlbumMeta {
        NewAlbumMeta {
            album_id: "00000000-0000-4000-8000-000000000001".into(),
            created_at: "1764000000000".into(),
            created_by: "9.9.9".into(),
        }
    }

    fn table_names(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type IN ('table', 'index') AND name NOT LIKE 'sqlite_%' AND name NOT LIKE 'search_%' ORDER BY name")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    #[test]
    fn migrations_are_numbered_from_one_without_gaps() {
        for (i, m) in MIGRATIONS.iter().enumerate() {
            assert_eq!(m.version as usize, i + 1);
        }
        assert_eq!(CURRENT_VERSION, 1);
    }

    #[test]
    fn a_new_database_gets_the_whole_schema_and_its_meta_rows() {
        let mut conn = Connection::open_in_memory().unwrap();
        let state = migrate_with(&mut conn, MIGRATIONS, fixed_meta).unwrap();
        assert_eq!(
            state,
            SchemaState::Current {
                version: 1,
                from: 0
            }
        );
        assert_eq!(version(&conn).unwrap(), 1);
        assert_eq!(
            table_names(&conn),
            [
                "meta",
                "people",
                "photo_labels",
                "photo_people",
                "photos",
                "photos_captured",
                "photos_dir",
                "photos_file_key",
                "photos_thumb_key",
                "search",
                "tags",
                "vectors",
            ]
        );
        assert_eq!(
            meta(&conn, "album_id").unwrap().as_deref(),
            Some("00000000-0000-4000-8000-000000000001")
        );
        assert_eq!(
            meta(&conn, "created_at").unwrap().as_deref(),
            Some("1764000000000")
        );
        assert_eq!(meta(&conn, "created_by").unwrap().as_deref(), Some("9.9.9"));
        assert_eq!(meta(&conn, "nope").unwrap(), None);
    }

    #[test]
    fn fresh_meta_has_a_v4_uuid_this_version_and_a_millisecond_time() {
        let m = NewAlbumMeta::now();
        let id = uuid::Uuid::parse_str(&m.album_id).unwrap();
        assert_eq!(id.get_version_num(), 4);
        assert_eq!(m.created_by, crate::app::version().version);
        let ms: u128 = m.created_at.parse().unwrap();
        assert!(ms > 1_700_000_000_000, "{ms}");
        assert_ne!(NewAlbumMeta::now().album_id, m.album_id);
    }

    #[test]
    fn a_current_database_is_left_alone() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate_with(&mut conn, MIGRATIONS, fixed_meta).unwrap();
        let state = migrate_with(&mut conn, MIGRATIONS, || panic!("not asked")).unwrap();
        assert_eq!(
            state,
            SchemaState::Current {
                version: 1,
                from: 1
            }
        );
    }

    #[test]
    fn later_migrations_run_in_order_and_keep_the_meta_rows() {
        const V2: &[Migration] = &[
            MIGRATIONS[0],
            Migration {
                version: 2,
                sql: "ALTER TABLE photos ADD COLUMN extra TEXT;",
            },
        ];
        let mut conn = Connection::open_in_memory().unwrap();
        migrate_with(&mut conn, MIGRATIONS, fixed_meta).unwrap();
        let state = migrate_with(&mut conn, V2, || panic!("not asked")).unwrap();
        assert_eq!(
            state,
            SchemaState::Current {
                version: 2,
                from: 1
            }
        );
        conn.prepare("SELECT extra FROM photos").unwrap();
        assert_eq!(
            meta(&conn, "album_id").unwrap().as_deref(),
            Some("00000000-0000-4000-8000-000000000001")
        );
    }

    #[test]
    fn a_failing_migration_changes_nothing() {
        const BROKEN: &[Migration] = &[
            MIGRATIONS[0],
            Migration {
                version: 2,
                sql: "CREATE TABLE fine (x); THIS IS NOT SQL;",
            },
        ];
        let mut conn = Connection::open_in_memory().unwrap();
        let e = migrate_with(&mut conn, BROKEN, fixed_meta).unwrap_err();
        assert_eq!(e.code, ErrorCode::Internal);
        assert_eq!(version(&conn).unwrap(), 0);
        assert!(table_names(&conn).is_empty(), "{:?}", table_names(&conn));
    }

    #[test]
    fn a_newer_database_is_reported_and_not_written() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate_with(&mut conn, MIGRATIONS, fixed_meta).unwrap();
        conn.pragma_update(None, "user_version", 7).unwrap();
        let state = migrate_with(&mut conn, MIGRATIONS, || panic!("not asked")).unwrap();
        assert_eq!(state, SchemaState::Newer { version: 7 });
        assert_eq!(version(&conn).unwrap(), 7);
    }

    #[test]
    fn full_text_search_supports_contentless_delete() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate_with(&mut conn, MIGRATIONS, fixed_meta).unwrap();
        conn.execute(
            "INSERT INTO search (rowid, file_name, description, place) VALUES (5, 'beach.jpg', 'Café on the beach', 'Nice')",
            [],
        )
        .unwrap();
        let hit = |q: &str| -> Vec<i64> {
            let mut s = conn
                .prepare("SELECT rowid FROM search WHERE search MATCH ?1")
                .unwrap();
            s.query_map([q], |r| r.get(0))
                .unwrap()
                .map(Result::unwrap)
                .collect()
        };
        assert_eq!(hit("cafe"), [5], "diacritics are removed");
        assert_eq!(hit("be*"), [5], "prefix search");
        conn.execute("DELETE FROM search WHERE rowid = 5", [])
            .unwrap();
        assert!(hit("cafe").is_empty());
    }

    #[test]
    fn deleting_a_photo_removes_its_people_links_tags_vectors_and_labels() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate_with(&mut conn, MIGRATIONS, fixed_meta).unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        conn.execute_batch(
            "INSERT INTO photos (id, rel_path, rel_dir, file_name, size, mtime_ms, file_key, thumb_key, kind, format, width, height, scan_version)
               VALUES (1, 'a/b.jpg', 'a', 'b.jpg', 10, 20, 'fk1:10:x', 'tk', 'image', 'jpeg', 4, 3, 1);
             INSERT INTO people (id, name) VALUES (1, 'Ann');
             INSERT INTO photo_people VALUES (1, 1);
             INSERT INTO tags (photo_id, part, model_id, status, made_at) VALUES (1, 'describe', 'm', 'done', 1);
             INSERT INTO vectors VALUES (1, 'm', 1, x'0000803f');
             INSERT INTO photo_labels VALUES ('keyword', 'sea', 1);
             DELETE FROM photos WHERE id = 1;",
        )
        .unwrap();
        for t in ["photo_people", "tags", "vectors", "photo_labels"] {
            let n: i64 = conn
                .query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0, "{t}");
        }
        let people: i64 = conn
            .query_row("SELECT count(*) FROM people", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            people, 1,
            "an unused name is removed by the writer, not by a cascade"
        );
    }

    #[test]
    fn person_names_are_unique_ignoring_case() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate_with(&mut conn, MIGRATIONS, fixed_meta).unwrap();
        conn.execute("INSERT INTO people (name) VALUES ('Ann')", [])
            .unwrap();
        assert!(
            conn.execute("INSERT INTO people (name) VALUES ('ANN')", [])
                .is_err()
        );
    }
}
