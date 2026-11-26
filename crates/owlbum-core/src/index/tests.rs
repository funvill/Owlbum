use super::*;
use crate::album::schema::{self, MIGRATIONS, NewAlbumMeta};

fn db() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    schema::migrate_with(&mut conn, MIGRATIONS, NewAlbumMeta::now).unwrap();
    conn
}

/// Inserts a plain photo at `rel_dir/file_name` and answers its id.
fn add(conn: &Connection, rel_dir: &str, file_name: &str) -> i64 {
    let rel_path = if rel_dir.is_empty() {
        file_name.to_owned()
    } else {
        format!("{rel_dir}/{file_name}")
    };
    conn.execute(
        "INSERT INTO photos (rel_path, rel_dir, file_name, size, mtime_ms, file_key, thumb_key, kind, format, width, height, scan_version)
         VALUES (?1, ?2, ?3, 1, 1, 'fk', 'tk', 'image', 'jpeg', 4000, 3000, 1)",
        (&rel_path, rel_dir, file_name),
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn set(conn: &Connection, id: i64, assignment: &str) {
    conn.execute(
        &format!("UPDATE photos SET {assignment} WHERE id = ?1"),
        [id],
    )
    .unwrap();
}

fn ids(index: &AlbumIndex) -> Vec<u32> {
    index.read(|v| v.photos().iter().map(|p| p.id).collect())
}

fn id(n: i64) -> u32 {
    u32::try_from(n).unwrap()
}

/// File names in name order.
fn names(index: &AlbumIndex) -> Vec<String> {
    index.read(|v| {
        v.name_order()
            .iter()
            .map(|&id| {
                let p = v.get(id).unwrap();
                format!("{}/{}", v.folder(p.folder), v.file_name(p))
            })
            .collect()
    })
}

/// Checks the name order and the ranks agree.
fn assert_ranks_consistent(index: &AlbumIndex) {
    index.read(|v| {
        assert_eq!(v.name_order().len(), v.len());
        for (rank, &id) in v.name_order().iter().enumerate() {
            assert_eq!(v.get(id).unwrap().name_rank as usize, rank);
        }
    });
}

#[test]
fn an_empty_album_has_an_empty_index() {
    let index = AlbumIndex::load(&db(), "").unwrap();
    assert!(index.is_empty());
    assert_eq!(index.read(|v| v.name_order().len()), 0);
    assert!(AlbumIndex::empty("x").is_empty());
    assert_eq!(AlbumIndex::empty("x").read(|v| v.scope().to_owned()), "x");
}

#[test]
fn the_load_reads_every_column_the_index_keeps() {
    let conn = db();
    let a = add(&conn, "2024/06", "IMG_1.jpg");
    set(
        &conn,
        a,
        "star = 1, hidden = 1, score_adjust = 4, captured_at = '2024-06-01T10:00:00', gps_lat = 1, gps_lon = 2, edit = '{}', kind = 'raw', width = 6000, height = 4000",
    );
    set(
        &conn,
        a,
        r#"analysis = '{"sharpness":10,"mean_luma":0.5,"clipped_shadows":0,"clipped_highlights":0,"saliency_x":0.5,"saliency_y":0.5,"dhash":"00000000000000ff"}'"#,
    );
    conn.execute_batch(
        "INSERT INTO people (id, name) VALUES (1, 'Ann'), (2, 'Bo');
         INSERT INTO photo_people VALUES (1, 1), (1, 2);",
    )
    .unwrap();
    let b = add(&conn, "", "plain.png");

    let index = AlbumIndex::load(&conn, "").unwrap();
    assert_eq!(ids(&index), [id(a), id(b)]);
    index.read(|v| {
        let p = v.get(id(a)).unwrap();
        assert_eq!(v.folder(p.folder), "2024/06");
        assert_eq!(v.file_name(p), "IMG_1.jpg");
        assert!(p.star() && p.hidden() && p.has_location() && p.has_edit());
        assert_eq!(p.score_adjust, 4);
        assert_eq!(p.kind, Kind::Raw);
        assert_eq!((p.width, p.height), (6000, 4000));
        assert_eq!(p.date(), parse_date("2024-06-01T10:00:00"));
        assert_eq!(p.people, 2);
        assert_eq!(p.analysis().unwrap().dhash(), Some(0xff));
        assert_eq!(p.derived, Derived::default());

        let q = v.get(id(b)).unwrap();
        assert_eq!(v.folder(q.folder), "");
        assert!(!q.star() && !q.has_location() && !q.has_edit());
        assert_eq!(q.people, 0);
        assert_eq!(q.date(), None);
        assert_eq!(q.analysis(), None);
        assert_eq!(v.get(999), None);
    });
}

#[test]
fn missing_and_unreadable_rows_are_left_out() {
    let conn = db();
    let keep = add(&conn, "", "keep.jpg");
    let gone = add(&conn, "", "gone.jpg");
    set(&conn, gone, "missing = 1");
    let broken = add(&conn, "", "broken.jpg");
    set(&conn, broken, "scan_error = 'unsupported: no'");
    let index = AlbumIndex::load(&conn, "").unwrap();
    assert_eq!(ids(&index), [id(keep)]);
}

#[test]
fn a_scoped_open_holds_the_scope_only() {
    let conn = db();
    let root = add(&conn, "", "root.jpg");
    let ab = add(&conn, "ab", "1.jpg");
    let ab_sub = add(&conn, "ab/cd", "2.jpg");
    let abc = add(&conn, "abc", "3.jpg");
    let abc_sub = add(&conn, "abc/ab", "4.jpg");
    let upper = add(&conn, "AB", "5.jpg");
    let wild = add(&conn, "a_", "6.jpg");
    let pct = add(&conn, "a%", "7.jpg");

    let index = AlbumIndex::load(&conn, "ab").unwrap();
    assert_eq!(ids(&index), [id(ab), id(ab_sub)]);
    let index = AlbumIndex::load(&conn, "a_").unwrap();
    assert_eq!(ids(&index), [id(wild)], "'_' is not a wildcard");
    let index = AlbumIndex::load(&conn, "a%").unwrap();
    assert_eq!(ids(&index), [id(pct)], "'%' is not a wildcard");
    let index = AlbumIndex::load(&conn, "").unwrap();
    assert_eq!(
        ids(&index),
        [root, ab, ab_sub, abc, abc_sub, upper, wild, pct].map(id)
    );
}

#[test]
fn the_name_order_is_natural_then_folder_then_id() {
    let conn = db();
    add(&conn, "b", "img_10.jpg");
    add(&conn, "a", "IMG_9.jpg");
    add(&conn, "b", "img_2.jpg");
    add(&conn, "a10", "x.jpg");
    add(&conn, "a9", "x.jpg");
    add(&conn, "a", "IMG_2.jpg");
    let index = AlbumIndex::load(&conn, "").unwrap();
    assert_eq!(
        names(&index),
        [
            "a/IMG_2.jpg",
            "b/img_2.jpg",
            "a/IMG_9.jpg",
            "b/img_10.jpg",
            "a9/x.jpg",
            "a10/x.jpg",
        ]
    );
    assert_ranks_consistent(&index);
}

#[test]
fn equal_names_in_one_folder_fall_back_to_the_id() {
    let conn = db();
    let first = add(&conn, "", "A.jpg");
    let second = add(&conn, "", "a.jpg");
    let index = AlbumIndex::load(&conn, "").unwrap();
    assert_eq!(
        index.read(|v| v.name_order().to_vec()),
        [id(first), id(second)]
    );
}

#[test]
fn bad_rows_do_not_stop_the_load() {
    let conn = db();
    let a = add(&conn, "", "a.jpg");
    set(&conn, a, "analysis = 'not json', captured_at = 'yesterday'");
    let big = add(&conn, "", "big.jpg");
    set(&conn, big, &format!("id = {}", i64::from(u32::MAX) + 10));
    let index = AlbumIndex::load(&conn, "").unwrap();
    assert_eq!(ids(&index), [id(a)], "an id beyond 32 bits is skipped");
    index.read(|v| {
        let p = v.get(id(a)).unwrap();
        assert_eq!(p.analysis(), None);
        assert_eq!(p.date(), None);
    });
}

#[test]
fn a_refresh_adds_changes_and_removes_rows() {
    let conn = db();
    let a = add(&conn, "", "b.jpg");
    let b = add(&conn, "", "d.jpg");
    let index = AlbumIndex::load(&conn, "").unwrap();
    let g0 = index.generation();

    // New rows, one sorting first, one between.
    let c = add(&conn, "", "a.jpg");
    let d = add(&conn, "", "c.jpg");
    index.refresh(&conn, &[c, d]).unwrap();
    assert_eq!(ids(&index), [a, b, c, d].map(id));
    assert_eq!(names(&index), ["/a.jpg", "/b.jpg", "/c.jpg", "/d.jpg"]);
    assert_ranks_consistent(&index);
    assert!(index.generation() > g0);

    // A change.
    set(&conn, a, "star = 1");
    index.refresh(&conn, &[a, a]).unwrap();
    assert!(index.read(|v| v.get(id(a)).unwrap().star()));

    // A rename moves the row in the name order.
    set(&conn, a, "file_name = 'z.jpg', rel_path = 'z.jpg'");
    index.refresh(&conn, &[a]).unwrap();
    assert_eq!(names(&index), ["/a.jpg", "/c.jpg", "/d.jpg", "/z.jpg"]);
    assert_ranks_consistent(&index);

    // Missing, unreadable and deleted rows leave.
    set(&conn, b, "missing = 1");
    set(&conn, c, "scan_error = 'x: y'");
    conn.execute("DELETE FROM photos WHERE id = ?1", [d])
        .unwrap();
    index.refresh(&conn, &[b, c, d]).unwrap();
    assert_eq!(ids(&index), [id(a)]);
    assert_eq!(names(&index), ["/z.jpg"]);
    assert_ranks_consistent(&index);

    // And come back.
    set(&conn, b, "missing = 0");
    index.refresh(&conn, &[b]).unwrap();
    assert_eq!(ids(&index), [a, b].map(id));
    assert_ranks_consistent(&index);
}

#[test]
fn a_refresh_respects_the_scope() {
    let conn = db();
    let inside = add(&conn, "trip", "a.jpg");
    let index = AlbumIndex::load(&conn, "trip").unwrap();
    let outside = add(&conn, "home", "b.jpg");
    let deeper = add(&conn, "trip/day1", "c.jpg");
    index.refresh(&conn, &[outside, deeper]).unwrap();
    assert_eq!(ids(&index), [inside, deeper].map(id));
    // A row moved out of the scope leaves.
    set(&conn, inside, "rel_dir = 'home', rel_path = 'home/a.jpg'");
    index.refresh(&conn, &[inside]).unwrap();
    assert_eq!(ids(&index), [id(deeper)]);
}

#[test]
fn a_people_change_updates_the_count() {
    let conn = db();
    let a = add(&conn, "", "a.jpg");
    let index = AlbumIndex::load(&conn, "").unwrap();
    conn.execute_batch(
        "INSERT INTO people (id, name) VALUES (1, 'Ann');
         INSERT INTO photo_people VALUES (1, 1);",
    )
    .unwrap();
    index.refresh(&conn, &[a]).unwrap();
    assert_eq!(index.read(|v| v.get(id(a)).unwrap().people), 1);
}

#[test]
fn many_rows_inserted_out_of_order_stay_sorted() {
    let conn = db();
    for n in 0..50 {
        add(
            &conn,
            &format!("f{}", n % 7),
            &format!("IMG_{}.jpg", 100 - n),
        );
    }
    let index = AlbumIndex::load(&conn, "").unwrap();
    let mut new = Vec::new();
    for n in 0..300 {
        let i = add(&conn, &format!("f{}", n % 5), &format!("new_{n}.jpg"));
        new.push(i);
    }
    // Ids reported out of order, in a batch large enough to re-sort.
    new.reverse();
    index.refresh(&conn, &new).unwrap();
    assert_eq!(index.len(), 350);
    let fresh = AlbumIndex::load(&conn, "").unwrap();
    assert_eq!(ids(&index), ids(&fresh));
    assert_eq!(names(&index), names(&fresh));
    assert_ranks_consistent(&index);
}

#[test]
fn in_memory_rows_go_in_and_out_without_a_database() {
    let index = AlbumIndex::empty("trip");
    let facts = |id: i64, rel_dir: &str, name: &str| PhotoFacts {
        id,
        rel_dir: rel_dir.into(),
        file_name: name.into(),
        kind: "image".into(),
        ..PhotoFacts::default()
    };
    index.upsert(&[
        facts(10, "trip", "b.jpg"),
        facts(11, "trip/x", "a.jpg"),
        facts(12, "tripod", "c.jpg"),
    ]);
    assert_eq!(ids(&index), [10, 11], "the scope rule is the same as SQL's");
    assert_eq!(names(&index), ["trip/x/a.jpg", "trip/b.jpg"]);
    index.upsert(&[PhotoFacts {
        star: true,
        ..facts(10, "trip", "b.jpg")
    }]);
    assert!(index.read(|v| v.get(10).unwrap().star()));
    index.remove(&[11, 99]);
    assert_eq!(ids(&index), [10]);
    assert_ranks_consistent(&index);
}

#[test]
fn derived_values_are_installed_whole_and_keyed_by_id() {
    let conn = db();
    let a = add(&conn, "", "a.jpg");
    let b = add(&conn, "", "b.jpg");
    let index = AlbumIndex::load(&conn, "").unwrap();
    // A rebuild starts from a copy of the records...
    let inputs: Vec<Photo> = index.read(|v| v.photos().to_vec());
    // ...meanwhile a row arrives and one leaves...
    let c = add(&conn, "", "c.jpg");
    conn.execute("DELETE FROM photos WHERE id = ?1", [b])
        .unwrap();
    index.refresh(&conn, &[b, c]).unwrap();
    // ...and the rebuild's values go in.
    let values = inputs
        .iter()
        .map(|p| {
            (
                p.id,
                Derived {
                    score: Some(50 + p.id as i32),
                    ..Derived::default()
                },
            )
        })
        .collect();
    index.install_derived(values);
    index.read(|v| {
        assert_eq!(v.get(id(a)).unwrap().derived.score, Some(50 + id(a) as i32));
        assert_eq!(v.get(id(c)).unwrap().derived.score, None, "not scored yet");
        assert_eq!(v.get(id(b)), None);
    });
    // A changed row keeps its values until the next rebuild.
    set(&conn, a, "star = 1");
    index.refresh(&conn, &[a]).unwrap();
    assert_eq!(
        index.read(|v| v.get(id(a)).unwrap().derived.score),
        Some(50 + id(a) as i32)
    );
    // A rebuild that does not mention a row clears it.
    index.install_derived(Vec::new());
    assert_eq!(index.read(|v| v.get(id(a)).unwrap().derived.score), None);
}

#[test]
fn installing_derived_values_does_not_change_the_generation() {
    let conn = db();
    add(&conn, "", "a.jpg");
    let index = AlbumIndex::load(&conn, "").unwrap();
    let g = index.generation();
    index.install_derived(Vec::new());
    assert_eq!(index.generation(), g);
}

#[test]
fn a_reload_starts_again_from_the_database() {
    let conn = db();
    let a = add(&conn, "", "a.jpg");
    let index = AlbumIndex::load(&conn, "").unwrap();
    let b = add(&conn, "", "b.jpg");
    index.reload(&conn).unwrap();
    assert_eq!(ids(&index), [a, b].map(id));
    assert_ranks_consistent(&index);
}

#[test]
fn file_names_are_not_lost_when_rows_churn() {
    // Rows replaced many times leave old names behind in the name store;
    // it is compacted, and every name still reads right.
    let conn = db();
    let a = add(&conn, "", "keep-me.jpg");
    let b = add(&conn, "", "churn.jpg");
    let index = AlbumIndex::load(&conn, "").unwrap();
    for n in 0..500 {
        set(
            &conn,
            b,
            &format!("file_name = 'churn-{n}-{}.jpg'", "x".repeat(40)),
        );
        index.refresh(&conn, &[b]).unwrap();
    }
    index.read(|v| {
        assert_eq!(v.file_name(v.get(id(a)).unwrap()), "keep-me.jpg");
        assert!(v.file_name(v.get(id(b)).unwrap()).starts_with("churn-499-"));
    });
    assert!(
        index.heap_bytes() < 64 * 1024,
        "{} bytes: the name store was not compacted",
        index.heap_bytes()
    );
}

#[test]
fn memory_is_about_150_bytes_a_photo() {
    let conn = db();
    let tx = conn.unchecked_transaction().unwrap();
    for n in 0..2000 {
        add(
            &tx,
            &format!("{}/{:02}", 2016 + n % 10, 1 + n % 12),
            &format!("IMG_{n:05}.jpg"),
        );
    }
    tx.commit().unwrap();
    let index = AlbumIndex::load(&conn, "").unwrap();
    let per_photo = index.heap_bytes() / index.len();
    assert!(per_photo <= 170, "{per_photo} bytes a photo");
}

#[test]
fn sparse_ids_keep_the_name_order_too() {
    let conn = db();
    for n in 0..40 {
        let i = add(&conn, "", &format!("p{n}.jpg"));
        set(&conn, i, &format!("id = {}", 1 + n * 1_000_003));
    }
    let index = AlbumIndex::load(&conn, "").unwrap();
    let a = add(&conn, "", "p5a.jpg");
    let b = add(&conn, "", "a.jpg");
    index.refresh(&conn, &[a, b]).unwrap();
    let fresh = AlbumIndex::load(&conn, "").unwrap();
    assert_eq!(names(&index), names(&fresh));
    assert_ranks_consistent(&index);
}

#[test]
fn the_name_order_uses_the_walkers_string_rules() {
    // docs/03, "Order": runs, not characters, so "a" (then digits) comes
    // before the longer text run "a!.jpg", though '!' is below '1'.
    let conn = db();
    add(&conn, "", "a!.jpg");
    add(&conn, "", "a1.jpg");
    let index = AlbumIndex::load(&conn, "").unwrap();
    assert_eq!(names(&index), ["/a1.jpg", "/a!.jpg"]);
}
