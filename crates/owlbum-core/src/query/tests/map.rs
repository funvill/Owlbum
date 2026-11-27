//! `query_map` (docs/04-browsing.md, "What the map holds").

use super::*;

fn map_query() -> Query {
    Query {
        view: QueryView::Map,
        sort: QuerySort::Name,
        ..default_query()
    }
}

#[test]
fn points_are_the_located_photos_in_the_result_order() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let ids = add(
        &album,
        &[
            p("c.jpg", None),
            p("a.jpg", None),
            p("b.jpg", None),
            p("d.jpg", None),
        ],
    );
    change(
        &album,
        &format!(
            "UPDATE photos SET gps_lat = 49.25, gps_lon = -123.1 WHERE id = {c};
             UPDATE photos SET gps_lat = 10, gps_lon = 20, loc_lat = -33.9, loc_lon = 151.2 WHERE id = {a};",
            c = ids[0],
            a = ids[1],
        ),
        &ids,
    );
    let engine = QueryEngine::new();
    let r = run(&engine, &album, &map_query());
    let m = engine.map(&album, &Overlay::default(), &r.token).unwrap();
    // Name order: a, b, c, d. The typed place wins over GPS.
    assert_eq!(
        m.points,
        [
            MapPoint(ids[1], -33.9, 151.2, "tk-a.jpg".into(), "a.jpg".into()),
            MapPoint(ids[0], 49.25, -123.1, "tk-c.jpg".into(), "c.jpg".into()),
        ]
    );
    assert_eq!(m.unlocated, 2);
}

#[test]
fn a_place_off_the_globe_or_half_set_is_unlocated() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let ids = add(
        &album,
        &[
            p("a.jpg", None),
            p("b.jpg", None),
            p("c.jpg", None),
            p("d.jpg", None),
        ],
    );
    change(
        &album,
        &format!(
            "UPDATE photos SET gps_lat = 91, gps_lon = 0 WHERE id = {a};
             UPDATE photos SET gps_lat = 0, gps_lon = -180.5 WHERE id = {b};
             UPDATE photos SET gps_lat = 5 WHERE id = {c};
             UPDATE photos SET gps_lat = -90, gps_lon = 180 WHERE id = {d};",
            a = ids[0],
            b = ids[1],
            c = ids[2],
            d = ids[3],
        ),
        &ids,
    );
    let engine = QueryEngine::new();
    let r = run(&engine, &album, &map_query());
    let m = engine.map(&album, &Overlay::default(), &r.token).unwrap();
    assert_eq!(m.points.iter().map(|p| p.0).collect::<Vec<_>>(), [ids[3]]);
    assert_eq!(m.unlocated, 3);
}

#[test]
fn only_photos_that_pass_the_filter_are_on_the_map() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let ids = add(&album, &[p("a.jpg", None), hidden("b.jpg", None)]);
    change(&album, "UPDATE photos SET gps_lat = 1, gps_lon = 2", &ids);
    let engine = QueryEngine::new();
    let r = run(&engine, &album, &map_query());
    let m = engine.map(&album, &Overlay::default(), &r.token).unwrap();
    assert_eq!(m.points.iter().map(|p| p.0).collect::<Vec<_>>(), [ids[0]]);
    assert_eq!(m.unlocated, 0);
}

#[test]
fn a_dropped_token_answers_query_expired() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let engine = QueryEngine::new();
    assert_eq!(
        code(engine.map(&album, &Overlay::default(), "nope")),
        ErrorCode::QueryExpired
    );
}

#[test]
fn the_points_serialise_as_compact_arrays() {
    let m = MapPoints {
        points: vec![MapPoint(7, 1.5, -2.0, "k".into(), "a.jpg".into())],
        unlocated: 3,
    };
    assert_eq!(
        serde_json::to_value(&m).unwrap(),
        json!({ "points": [[7, 1.5, -2.0, "k", "a.jpg"]], "unlocated": 3 })
    );
}
