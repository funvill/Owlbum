//! Text search (docs/05-organising.md, "Text search").

use super::*;
use crate::album::search::reindex;

/// Four photos answered as `[a, b, c, d]`, their `search` entries written:
///
/// | | path | other |
/// |---|---|---|
/// | a | `Lakeside/IMG_1234.jpg` | notes "Café by the water", Ann on it, starred |
/// | b | `beach.jpg` | place "Nice", keyword "lake" |
/// | c | `day 2/boat.jpg` | description "José and the boat" |
/// | d | `lake.jpg` | hidden |
fn searchable(album: &Album) -> [i64; 4] {
    let ids = add(
        album,
        &[
            p("Lakeside/IMG_1234.jpg", None),
            p("beach.jpg", None),
            p("day 2/boat.jpg", None),
            hidden("lake.jpg", None),
        ],
    );
    let [a, b, c, d] = ids[..] else {
        unreachable!()
    };
    let sql = format!(
        "UPDATE photos SET notes = 'Café by the water', star = 1 WHERE id = {a};
         UPDATE photos SET loc_name = 'Nice' WHERE id = {b};
         UPDATE photos SET description = 'José and the boat' WHERE id = {c};
         INSERT INTO people (id, name) VALUES (1, 'Ann');
         INSERT INTO photo_people (photo_id, person_id) VALUES ({a}, 1);
         INSERT INTO photo_labels (kind, label, photo_id) VALUES ('keyword', 'lake', {b});"
    );
    let all = ids.clone();
    album
        .write_photos(move |tx, touched| {
            tx.execute_batch(&sql)
                .map_err(sqlite_error(text::album::WRITING_DATABASE))?;
            for &id in &all {
                reindex(tx, id)?;
            }
            touched.photos(all);
            Ok(())
        })
        .unwrap();
    [a, b, c, d]
}

fn searching(engine: &QueryEngine, album: &Album, text: &str, filter: Filter) -> Vec<i64> {
    let q = Query {
        search: Some(text.to_owned()),
        filter,
        ..default_query()
    };
    let r = run(engine, album, &q);
    let mut ids = tile_ids(engine, album, &r.token);
    ids.sort_unstable();
    ids
}

#[test]
fn search_text_matches_whole_words_and_their_starts() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let [a, b, c, _] = searchable(&album);
    let engine = QueryEngine::new();
    let find = |t: &str| searching(&engine, &album, t, Filter::default());
    assert_eq!(
        find("lak"),
        [b],
        "keyword; the folder is no source, the hidden one is left out"
    );
    assert_eq!(find("img_12"), [a], "a name with punctuation is a phrase");
    assert_eq!(find("cafe"), [a], "accents are ignored");
    assert_eq!(find("jose BOAT"), [c], "every term, any column, any case");
    assert_eq!(find("ann water"), [a], "terms may match different columns");
    assert_eq!(find("nice"), [b], "the place");
    assert!(find("ake").is_empty(), "no substring match");
    assert!(find("ann nice").is_empty(), "every term must match");
    assert!(
        find(r#""the boat" OR (x"#).is_empty(),
        "operators mean nothing"
    );
    assert_eq!(
        find(r#""and the bo""#),
        [c],
        "a phrase with its last word a prefix"
    );
}

#[test]
fn search_text_is_one_more_filter() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let [a, b, _, d] = searchable(&album);
    let engine = QueryEngine::new();
    let starred = Filter {
        star: Some(StarFilter::Yes),
        ..Filter::default()
    };
    assert_eq!(
        searching(&engine, &album, "a", Filter::default()).len(),
        3,
        "too short: ignored"
    );
    assert!(searching(&engine, &album, "nice", starred.clone()).is_empty());
    assert_eq!(searching(&engine, &album, "ann", starred), [a]);
    let shown = Filter {
        hidden: Some(HiddenFilter::Show),
        ..Filter::default()
    };
    assert_eq!(searching(&engine, &album, "lake", shown), [b, d]);

    let q = Query {
        search: Some("nice".to_owned()),
        want_facets: true,
        ..default_query()
    };
    let r = run(&engine, &album, &q);
    assert!(r.filtered);
    assert_eq!(
        (r.photos, r.total, r.panel_count),
        (1, 3, 0),
        "no chip, not counted"
    );
    let facets = r.facets.unwrap();
    assert_eq!(facets.star.yes, 0, "the starred photo fails the text");
    assert_eq!(facets.star.no, 1);
    assert_eq!(engine.facets(&album, &r.token).unwrap(), facets);
}

#[test]
fn a_change_to_typed_info_is_searchable_at_once() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let [_, b, _, _] = searchable(&album);
    let engine = QueryEngine::new();
    assert!(searching(&engine, &album, "giraffe", Filter::default()).is_empty());
    crate::info::typed::set_info(
        &album,
        vec![b],
        serde_json::from_value(json!({ "notes": "a giraffe" })).unwrap(),
    )
    .unwrap();
    assert_eq!(
        searching(&engine, &album, "giraffe", Filter::default()),
        [b]
    );
}
