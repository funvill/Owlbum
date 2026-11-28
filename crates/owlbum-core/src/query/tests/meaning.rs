//! The meaning segment (docs/05-organising.md, "How it combines";
//! docs/07-tagging.md, "Search by meaning") and the vector cache it is
//! found with.

use std::time::Instant;

use super::*;
use crate::album::search::reindex;
use crate::tagging::meaning::MOST;

/// Adds photos and writes their `search` entries, so their names match.
fn indexed(album: &Album, photos: &[P<'_>]) -> Vec<i64> {
    let ids = add(album, photos);
    let all = ids.clone();
    album
        .write_photos(move |tx, touched| {
            for &id in &all {
                reindex(tx, id)?;
            }
            touched.photos(all);
            Ok(())
        })
        .unwrap();
    ids
}

fn grid(text: &str) -> Query {
    Query {
        search: Some(text.to_owned()),
        ..default_query()
    }
}

fn u(ids: &[i64]) -> Vec<u32> {
    ids.iter().map(|&i| u32::try_from(i).unwrap()).collect()
}

#[test]
fn the_segment_follows_the_word_matches_in_hit_order() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let [a, b, c, d] = indexed(
        &album,
        &[
            p("harbour.jpg", Some("2024-01-04T00:00:00")),
            p("x1.jpg", Some("2024-01-01T00:00:00")),
            p("x2.jpg", Some("2024-01-02T00:00:00")),
            p("x3.jpg", Some("2024-01-03T00:00:00")),
        ],
    )[..] else {
        unreachable!()
    };
    let engine = QueryEngine::new();
    let r = run(&engine, &album, &grid("harbour"));
    assert_eq!(r.count, 1);
    assert_eq!(
        engine.meaning_text(&r.token).unwrap().as_deref(),
        Some("harbour")
    );

    // d scores best, then the text match a (dropped), then b; c is no hit.
    let (count, photos, start) = engine
        .append_meaning(&album, &r.token, &u(&[d, a, b]))
        .unwrap();
    assert_eq!((count, photos, start), (3, 3, Some(1)));
    assert_eq!(
        tile_ids(&engine, &album, &r.token),
        [a, d, b],
        "score order whatever the sort"
    );
    let rows = engine
        .window(&album, &Overlay::default(), &r.token, 0, 10)
        .unwrap();
    assert!(rows.iter().all(|r| r.stack.is_none()), "never stacked");
    assert_eq!(
        engine.locate(&r.token, d).unwrap(),
        Some(Located::At { index: 1 })
    );
    assert_eq!(engine.ids(&r.token, 1, 10).unwrap(), [d, b]);
    assert_eq!(engine.locate(&r.token, c).unwrap(), None);

    // Called again, the segment is replaced, not added to.
    let again = engine.append_meaning(&album, &r.token, &u(&[b])).unwrap();
    assert_eq!(again, (2, 2, Some(1)));
    assert_eq!(tile_ids(&engine, &album, &r.token), [a, b]);
    // Nothing found: no segment.
    let none = engine.append_meaning(&album, &r.token, &[]).unwrap();
    assert_eq!(none, (1, 1, None));

    // A new run has no segment until it is asked for.
    engine.append_meaning(&album, &r.token, &u(&[d])).unwrap();
    let fresh = run(&engine, &album, &grid("harbour"));
    assert_eq!(tile_ids(&engine, &album, &fresh.token), [a]);
}

#[test]
fn hits_that_fail_another_filter_are_hidden_or_gone_are_dropped() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let ids = indexed(
        &album,
        &[
            p("harbour.jpg", None),
            p("x1.jpg", None),
            hidden("x2.jpg", None),
            p("x3.jpg", None),
        ],
    );
    let [a, b, c, d] = ids[..] else {
        unreachable!()
    };
    change(
        &album,
        &format!("UPDATE photos SET star = 1 WHERE id IN ({a}, {d})"),
        &ids,
    );
    let engine = QueryEngine::new();

    let r = run(&engine, &album, &grid("harbour"));
    let (_, _, start) = engine
        .append_meaning(&album, &r.token, &[9999, u(&[c])[0], u(&[b])[0]])
        .unwrap();
    assert_eq!(start, Some(1));
    assert_eq!(
        tile_ids(&engine, &album, &r.token),
        [a, b],
        "a missing id and a hidden photo are dropped"
    );

    let starred = Query {
        filter: Filter {
            star: Some(StarFilter::Yes),
            ..Filter::default()
        },
        ..grid("harbour")
    };
    let r = run(&engine, &album, &starred);
    engine
        .append_meaning(&album, &r.token, &u(&[b, d]))
        .unwrap();
    assert_eq!(
        tile_ids(&engine, &album, &r.token),
        [a, d],
        "the star filter still applies"
    );
}

#[test]
fn copies_appear_once_and_never_beside_their_text_match() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let ids = indexed(
        &album,
        &[
            p("harbour.jpg", None),
            p("x1.jpg", None),
            p("x1-copy.jpg", None),
            p("x3.jpg", None),
            p("x4.jpg", None),
        ],
    );
    let [a, b, c, _, e] = ids[..] else {
        unreachable!()
    };
    change(
        &album,
        &format!(
            "UPDATE photos SET file_key = 'k-x', analysis = '{{\"dhash\":\"11\"}}' WHERE id IN ({b}, {c});
             UPDATE photos SET file_key = 'k-a', analysis = '{{\"dhash\":\"22\"}}' WHERE id IN ({a}, {e});"
        ),
        &ids,
    );
    // e has the same file key and hash as a, which is a text match.
    let engine = QueryEngine::new();
    let r = run(&engine, &album, &grid("harbour.jpg"));
    let words = tile_ids(&engine, &album, &r.token);
    assert!(words.contains(&a));
    engine
        .append_meaning(&album, &r.token, &u(&[c, b, e]))
        .unwrap();
    let all = tile_ids(&engine, &album, &r.token);
    assert_eq!(&all[words.len()..], [c], "b is a copy of c; e of a");
}

#[test]
fn at_most_200_are_added() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let names: Vec<String> = (0..MOST + 30).map(|i| format!("p{i}.jpg")).collect();
    let photos: Vec<P<'_>> = names.iter().map(|n| p(n, None)).collect();
    let ids = add(&album, &photos);
    let engine = QueryEngine::new();
    let r = run(&engine, &album, &grid("nothing matches"));
    assert_eq!(r.count, 0);
    let (count, photos, start) = engine.append_meaning(&album, &r.token, &u(&ids)).unwrap();
    assert_eq!((count, photos, start), (200, 200, Some(0)));
    assert_eq!(engine.ids(&r.token, 0, 1).unwrap(), [ids[0]]);
}

#[test]
fn only_a_grid_result_with_text_takes_a_segment_and_a_dropped_token_expires() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    small(&album);
    let engine = QueryEngine::new();
    let text = |q: Query| {
        let r = run(&engine, &album, &q);
        engine.meaning_text(&r.token).unwrap()
    };
    assert_eq!(text(grid("  sea  ")).as_deref(), Some("sea"));
    assert_eq!(text(default_query()), None);
    assert_eq!(
        text(Query {
            view: QueryView::Timeline,
            ..grid("sea")
        }),
        None
    );
    assert_eq!(
        text(Query {
            view: QueryView::Map,
            ..grid("sea")
        }),
        None
    );
    assert_eq!(code(engine.meaning_text("q-gone")), ErrorCode::QueryExpired);
    assert_eq!(
        code(engine.append_meaning(&album, "q-gone", &[])),
        ErrorCode::QueryExpired
    );
}

// ---- The vector cache ----

fn blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// Writes vector rows through the writer, as the tagging queue does, and
/// tells the cache.
fn put_vectors(album: &Album, rows: Vec<(i64, &'static str, i64, Vec<u8>)>) {
    let ids: Vec<i64> = rows.iter().map(|r| r.0).collect();
    album
        .db()
        .write(move |tx| {
            for (id, model, dim, bytes) in &rows {
                tx.execute(
                    "INSERT OR REPLACE INTO vectors (photo_id, model_id, dim, vector) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![id, model, dim, bytes],
                )
                .map_err(sqlite_error(text::album::WRITING_DATABASE))?;
            }
            Ok(())
        })
        .unwrap();
    album.vectors().stale(ids);
}

#[test]
fn the_cache_scores_skips_other_dims_and_follows_writes() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let [a, b, c, d] = add(
        &album,
        &[
            p("a.jpg", None),
            p("b.jpg", None),
            p("c.jpg", None),
            p("d.jpg", None),
        ],
    )[..] else {
        unreachable!()
    };
    put_vectors(
        &album,
        vec![
            (a, "m", 4, blob(&[1.0, 0.0, 0.0, 0.0])),
            (b, "m", 4, blob(&[0.8, 0.6, 0.0, 0.0])),
            (c, "m", 3, blob(&[1.0, 0.0, 0.0])),
            (d, "other", 4, blob(&[1.0, 0.0, 0.0, 0.0])),
        ],
    );
    let t = [1.0, 0.0, 0.0, 0.0];
    let ids = |hits: Vec<(u32, f32)>| hits.into_iter().map(|h| i64::from(h.0)).collect::<Vec<_>>();
    let cache = album.vectors();
    assert_eq!(ids(cache.scores(&album, "m", &t, 0.5).unwrap()), [a, b]);
    assert_eq!(ids(cache.scores(&album, "m", &t, 0.9).unwrap()), [a]);

    // Written after the cache was filled: found without reading it all.
    put_vectors(
        &album,
        vec![
            (c, "m", 4, blob(&[0.0, 0.6, 0.8, 0.0])),
            (d, "m", 4, blob(&[0.9, 0.0, 0.0, 0.43589])),
        ],
    );
    assert_eq!(ids(cache.scores(&album, "m", &t, 0.5).unwrap()), [a, d, b]);

    // A rescan that finds new content deletes the vector (rows.rs) through
    // write_photos, which marks the photo.
    change(
        &album,
        &format!("DELETE FROM vectors WHERE photo_id = {a}"),
        &[a],
    );
    assert_eq!(ids(cache.scores(&album, "m", &t, 0.5).unwrap()), [d, b]);

    // Another model fills afresh.
    assert_eq!(ids(cache.scores(&album, "other", &t, 0.5).unwrap()), [d]);
    // Equal scores go by id.
    put_vectors(
        &album,
        vec![(b, "other", 4, blob(&t)), (a, "other", 4, blob(&t))],
    );
    assert_eq!(
        ids(cache.scores(&album, "other", &t, 0.5).unwrap()),
        [a, b, d]
    );
}

/// The benchmark of docs/07-tagging.md, "Performance for 100,000
/// vectors": a whole search over 100,000 random vectors (the scan, the
/// sort, and the meaning segment's checks) must take under 150 ms. Slow
/// to set up, so opt-in: `cargo test --release -p owlbum-core --lib
/// meaning_search_speed -- --ignored --nocapture`.
#[test]
#[ignore = "benchmark: builds 100,000 photos and vectors"]
fn meaning_search_speed_over_100000_vectors() {
    const N: usize = 100_000;
    for dim in [512usize, 768] {
        let tmp = tempfile::tempdir().unwrap();
        let album = album(tmp.path());
        let ids = album
            .write_photos(move |tx, touched| {
                let mut stmt = tx
                    .prepare(
                        "INSERT INTO photos (rel_path, rel_dir, file_name, size, mtime_ms, file_key, thumb_key, kind, format, width, height, scan_version)
                         VALUES (?1, '', ?1, 1, 1, 'fk-' || ?1, 'tk-' || ?1, 'image', 'jpeg', 4000, 3000, 1)",
                    )
                    .map_err(sqlite_error(text::album::WRITING_DATABASE))?;
                let mut vec_stmt = tx
                    .prepare("INSERT INTO vectors (photo_id, model_id, dim, vector) VALUES (?1, 'm', ?2, ?3)")
                    .map_err(sqlite_error(text::album::WRITING_DATABASE))?;
                let mut seed = 0x2545_f491_4f6c_dd1du64;
                let mut ids = Vec::with_capacity(N);
                for i in 0..N {
                    stmt.execute([format!("p{i}.jpg")])
                        .map_err(sqlite_error(text::album::WRITING_DATABASE))?;
                    let id = tx.last_insert_rowid();
                    let v: Vec<f32> = (0..dim)
                        .map(|_| {
                            seed ^= seed << 13;
                            seed ^= seed >> 7;
                            seed ^= seed << 17;
                            (seed >> 40) as f32 / (1u64 << 24) as f32 - 0.5
                        })
                        .collect();
                    let v = crate::tagging::meaning::unit(v).unwrap();
                    vec_stmt
                        .execute(rusqlite::params![id, dim as i64, blob(&v)])
                        .map_err(sqlite_error(text::album::WRITING_DATABASE))?;
                    touched.photo(id);
                    ids.push(id);
                }
                Ok(ids)
            })
            .unwrap();
        let engine = QueryEngine::new();
        let r = run(&engine, &album, &grid("sunset at the beach"));
        // A text vector close to the first photo's, so some photos pass.
        let t: Vec<f32> = album
            .db()
            .read(|c| {
                let bytes: Vec<u8> = c
                    .query_row(
                        "SELECT vector FROM vectors WHERE photo_id = ?1",
                        [ids[0]],
                        |r| r.get(0),
                    )
                    .unwrap();
                Ok(crate::tagging::results::vector_from_bytes(&bytes, dim).unwrap())
            })
            .unwrap();
        let fill = Instant::now();
        album.vectors().scores(&album, "m", &t, 2.0).unwrap();
        let fill = fill.elapsed();
        let mut best = std::time::Duration::MAX;
        for _ in 0..5 {
            let start = Instant::now();
            let hits: Vec<u32> = album
                .vectors()
                .scores(&album, "m", &t, 0.05)
                .unwrap()
                .into_iter()
                .map(|h| h.0)
                .collect();
            let (_, _, from) = engine.append_meaning(&album, &r.token, &hits).unwrap();
            assert_eq!(from, Some(0));
            best = best.min(start.elapsed());
        }
        println!("dim {dim}: filling the cache {fill:?}; a whole search {best:?}");
        assert!(
            best.as_millis() < 150,
            "a search over {N} vectors of {dim} took {best:?}"
        );
    }
}
