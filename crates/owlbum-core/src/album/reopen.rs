//! The album session: where the person left off in a folder
//! (docs/04-browsing.md, "Reopening where I was").
//!
//! The page sends and receives the current photo as an id; `recent.session`
//! in `app.sqlite` keeps it as the photo's path relative to the album root,
//! so it still means the same photo after the album database is rebuilt.
//! [`to_stored`] turns the page's session into the stored one and
//! [`to_page`] turns it back, reading `photos` for the conversion.
//!
//! The session is JSON with the query's own field names (`view`, `sort`,
//! `filter`, `search`, ...) beside `selected_id` and `scroll`. Fields this
//! version does not know pass through both ways untouched, so a later query
//! field (or an older Owlbum's) is kept. On the way back to the page the
//! known fields are checked: a value that is not allowed is dropped, so a
//! session written by another version can never break opening the folder.

use rusqlite::{Connection, OptionalExtension};
use serde_json::{Map, Value};

use crate::album::sqlite_error;
use crate::error::AppResult;
use crate::query::{Filter, Query};
use crate::text;

/// The page's session turned into the stored one: `selected_id` becomes
/// `selected`, the photo's `rel_path`, or null when the id names no photo.
/// `selected` is always written, so a save with nothing selected replaces
/// the photo an earlier save kept. Anything that is not an object stores
/// as an empty object.
pub fn to_stored(conn: &Connection, page: &Value) -> AppResult<Value> {
    let mut out = page.as_object().cloned().unwrap_or_default();
    let id = out.remove("selected_id").and_then(|v| v.as_i64());
    let path = match id {
        Some(id) => conn
            .query_row("SELECT rel_path FROM photos WHERE id = ?1", [id], |r| {
                r.get::<_, String>(0)
            })
            .optional()
            .map_err(sqlite_error(text::album::READING_DATABASE))?,
        None => None,
    };
    out.insert("selected".into(), path.map_or(Value::Null, Value::String));
    Ok(Value::Object(out))
}

/// The stored session turned back into the page's: `selected` becomes
/// `selected_id`, null when no photo that is not missing has that path any
/// more. The [`Query`] fields (`view`, `sort`, `search`, ...), each filter
/// and `scroll` are checked, and a value that is not allowed is dropped;
/// the page fills what is absent from its defaults.
pub fn to_page(conn: &Connection, stored: &Value) -> AppResult<Value> {
    let mut out = stored.as_object().cloned().unwrap_or_default();
    let path = out.remove("selected");
    let id = match path.as_ref().and_then(Value::as_str) {
        Some(path) => conn
            .query_row(
                "SELECT id FROM photos WHERE rel_path = ?1 AND missing = 0",
                [path],
                |r| r.get::<_, i64>(0),
            )
            .optional()
            .map_err(sqlite_error(text::album::READING_DATABASE))?,
        None => None,
    };
    out.insert("selected_id".into(), id.map_or(Value::Null, Value::from));
    keep_query_fields(&mut out);
    if !out
        .get("scroll")
        .is_some_and(|v| v.as_u64().is_some_and(|n| u32::try_from(n).is_ok()))
    {
        out.remove("scroll");
    }
    if let Some(filter) = out.remove("filter") {
        out.insert("filter".into(), Value::Object(clean_filter(&filter)));
    }
    Ok(Value::Object(out))
}

/// The photo whose file is `path`, in the album whose root is `root`: its
/// id, or none when the path is not under the root or no photo that is not
/// missing has it (not scanned yet, say). On Windows the name is matched
/// whatever its case, as the file system does.
pub fn photo_at(
    conn: &Connection,
    root: &std::path::Path,
    path: &std::path::Path,
) -> AppResult<Option<i64>> {
    let Ok(below) = path.strip_prefix(root) else {
        return Ok(None);
    };
    let mut parts = Vec::new();
    for part in below.components() {
        let std::path::Component::Normal(name) = part else {
            return Ok(None);
        };
        let Some(name) = name.to_str() else {
            return Ok(None);
        };
        parts.push(name);
    }
    let rel = parts.join("/");
    let find = |sql: &str| {
        conn.query_row(sql, [&rel], |r| r.get::<_, i64>(0))
            .optional()
            .map_err(sqlite_error(text::album::READING_DATABASE))
    };
    let exact = find("SELECT id FROM photos WHERE rel_path = ?1 AND missing = 0")?;
    if exact.is_some() || !cfg!(windows) {
        return Ok(exact);
    }
    find(
        "SELECT id FROM photos WHERE rel_path = ?1 COLLATE NOCASE AND missing = 0 ORDER BY id LIMIT 1",
    )
}

/// Drops each field named as a [`Query`] field whose value that field
/// cannot take, tried one at a time over the default query, so every field
/// the `Query` gains later is checked too. The filter is checked filter by
/// filter in [`clean_filter`] instead.
fn keep_query_fields(out: &mut Map<String, Value>) {
    let Ok(Value::Object(base)) = serde_json::to_value(Query::default()) else {
        return;
    };
    out.retain(|k, v| {
        if k == "filter" || !base.contains_key(k) {
            return true;
        }
        let mut one = base.clone();
        one.insert(k.clone(), v.clone());
        serde_json::from_value::<Query>(Value::Object(one)).is_ok()
    });
}

/// The filters of `filter` that read as a [`Filter`] each on its own; a
/// filter this version does not know is dropped too (docs/05-organising.md,
/// "Lifetime").
fn clean_filter(filter: &Value) -> Map<String, Value> {
    let Some(filter) = filter.as_object() else {
        return Map::new();
    };
    filter
        .iter()
        .filter(|(k, v)| {
            let one = Value::Object(Map::from_iter([((*k).clone(), (*v).clone())]));
            serde_json::from_value::<Filter>(one)
                .ok()
                .and_then(|f| serde_json::to_value(f).ok())
                .is_some_and(|back| back.get(k.as_str()).is_some())
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::album::schema;
    use serde_json::json;

    #[test]
    fn a_file_is_found_by_its_path_under_the_root() {
        let c = db();
        let root = std::path::Path::new("/albums/trip");
        assert_eq!(
            photo_at(&c, root, &root.join("a").join("beach.jpg")).unwrap(),
            Some(1)
        );
        assert_eq!(photo_at(&c, root, &root.join("lake.jpg")).unwrap(), Some(2));
        assert_eq!(
            photo_at(&c, root, &root.join("new.jpg")).unwrap(),
            None,
            "not scanned yet"
        );
        let elsewhere = std::path::Path::new("/albums/other/lake.jpg");
        assert_eq!(
            photo_at(&c, root, elsewhere).unwrap(),
            None,
            "not under the root"
        );
        if cfg!(windows) {
            assert_eq!(photo_at(&c, root, &root.join("LAKE.JPG")).unwrap(), Some(2));
        }
    }

    fn db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        schema::migrate(&mut conn).unwrap();
        conn.execute_batch(
            "INSERT INTO photos (id, rel_path, rel_dir, file_name, size, mtime_ms, file_key, thumb_key,
                                 kind, format, width, height, scan_version)
             VALUES (1, 'a/beach.jpg', 'a', 'beach.jpg', 1, 1, 'k1', 't1', 'image', 'jpeg', 1, 1, 1),
                    (2, 'lake.jpg', '', 'lake.jpg', 1, 1, 'k2', 't2', 'image', 'jpeg', 1, 1, 1);",
        )
        .unwrap();
        conn
    }

    fn page(selected: Value) -> Value {
        json!({
            "view": "timeline", "sort": "score", "filter": { "star": "yes" },
            "search": "beach", "selected_id": selected, "scroll": 40,
        })
    }

    #[test]
    fn the_current_photo_is_stored_by_path_and_read_back_as_its_id() {
        let c = db();
        let stored = to_stored(&c, &page(json!(1))).unwrap();
        assert_eq!(stored["selected"], json!("a/beach.jpg"));
        assert!(stored.get("selected_id").is_none());
        assert_eq!(to_page(&c, &stored).unwrap(), page(json!(1)));
    }

    #[test]
    fn nothing_selected_stores_an_explicit_null() {
        let c = db();
        assert_eq!(
            to_stored(&c, &page(Value::Null)).unwrap()["selected"],
            Value::Null
        );
        // An id that names no photo stores null too.
        assert_eq!(
            to_stored(&c, &page(json!(99))).unwrap()["selected"],
            Value::Null
        );
    }

    #[test]
    fn a_renamed_photo_is_saved_under_its_new_path() {
        // The row follows a rename; the next save turns the id into the
        // path it has then.
        let c = db();
        c.execute(
            "UPDATE photos SET rel_path = 'a/sea.jpg', file_name = 'sea.jpg' WHERE id = 1",
            [],
        )
        .unwrap();
        let stored = to_stored(&c, &page(json!(1))).unwrap();
        assert_eq!(stored["selected"], json!("a/sea.jpg"));
        assert_eq!(to_page(&c, &stored).unwrap()["selected_id"], json!(1));
    }

    #[test]
    fn a_photo_that_is_gone_or_missing_reads_as_null() {
        let c = db();
        let stored = to_stored(&c, &page(json!(2))).unwrap();
        c.execute("UPDATE photos SET missing = 1 WHERE id = 2", [])
            .unwrap();
        assert_eq!(to_page(&c, &stored).unwrap()["selected_id"], Value::Null);
        let gone = json!({ "selected": "nowhere.jpg" });
        assert_eq!(to_page(&c, &gone).unwrap()["selected_id"], Value::Null);
    }

    #[test]
    fn unknown_fields_pass_through_both_ways() {
        let c = db();
        let mut p = page(json!(2));
        p["later"] = json!({ "x": 1 });
        let stored = to_stored(&c, &p).unwrap();
        assert_eq!(stored["later"], json!({ "x": 1 }));
        assert_eq!(to_page(&c, &stored).unwrap()["later"], json!({ "x": 1 }));
    }

    #[test]
    fn values_that_are_not_allowed_are_dropped_one_by_one() {
        let c = db();
        let stored = json!({
            "view": "carousel", "sort": 7, "search": 3, "scroll": -1,
            "filter": { "star": "maybe", "shape": "portrait", "colour": "red", "people": ["Ann"] },
        });
        assert_eq!(
            to_page(&c, &stored).unwrap(),
            json!({
                "selected_id": null,
                "filter": { "shape": "portrait", "people": ["Ann"] },
            })
        );
    }

    #[test]
    fn a_session_that_is_not_an_object_reads_as_empty() {
        let c = db();
        assert_eq!(
            to_page(&c, &json!(5)).unwrap(),
            json!({ "selected_id": null })
        );
        assert_eq!(
            to_stored(&c, &json!("x")).unwrap(),
            json!({ "selected": null })
        );
    }
}
