//! The timeline view (docs/04-browsing.md, "Timeline view"; docs/05-organising.md,
//! "`query_run`").

use super::*;

fn timeline(unit: QueryTimelineUnit) -> Query {
    Query {
        view: QueryView::Timeline,
        timeline_unit: unit,
        ..default_query()
    }
}

fn group(key: &str, start: u32, count: u32) -> Group {
    Group {
        key: key.to_owned(),
        start,
        count,
        photos: count,
    }
}

#[test]
fn three_photos_on_two_days_and_one_undated_make_three_groups() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let ids = add(
        &album,
        &[
            p("b.jpg", Some("2026-07-15T08:00:00")),
            // Late at night: it stays on its written day whatever the
            // computer's time zone.
            p("a.jpg", Some("2026-07-14T23:30:00")),
            p("c.jpg", None),
            p("d.jpg", Some("2026-07-14T09:00:00")),
        ],
    );
    let engine = QueryEngine::new();
    let r = run(&engine, &album, &timeline(QueryTimelineUnit::Day));
    assert_eq!(
        tile_ids(&engine, &album, &r.token),
        [ids[3], ids[1], ids[0], ids[2]]
    );
    assert_eq!(r.unit, Some(GroupUnit::Day));
    assert_eq!(
        r.groups.unwrap(),
        [
            group("2026-07-14", 0, 2),
            group("2026-07-15", 2, 1),
            group("none", 3, 1),
        ]
    );
}

#[test]
fn month_groups_by_the_first_seven_characters() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    add(
        &album,
        &[
            p("a.jpg", Some("2026-06-30T23:59:59")),
            p("b.jpg", Some("2026-07-01T00:00:00")),
            p("c.jpg", Some("2026-07-31T12:00:00")),
            p("d.jpg", Some("2027-01-02T12:00:00")),
        ],
    );
    let engine = QueryEngine::new();
    let r = run(&engine, &album, &timeline(QueryTimelineUnit::Month));
    assert_eq!(r.unit, Some(GroupUnit::Month));
    assert_eq!(
        r.groups.unwrap(),
        [
            group("2026-06", 0, 1),
            group("2026-07", 1, 2),
            group("2027-01", 3, 1),
        ]
    );
}

#[test]
fn auto_picks_month_only_past_ninety_days() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    // 2026-01-01 to 2026-04-01 is 90 days; to 2026-04-02 is 91.
    let ids = add(
        &album,
        &[
            p("a.jpg", Some("2026-01-01T00:00:00")),
            p("b.jpg", Some("2026-04-01T23:00:00")),
            p("c.jpg", None),
        ],
    );
    let engine = QueryEngine::new();
    let r = run(&engine, &album, &timeline(QueryTimelineUnit::Auto));
    assert_eq!(r.unit, Some(GroupUnit::Day), "90 days is Day");
    change(
        &album,
        &format!(
            "UPDATE photos SET captured_at = '2026-04-02T00:00:00' WHERE id = {}",
            ids[1]
        ),
        &ids[1..2],
    );
    let r = run(&engine, &album, &timeline(QueryTimelineUnit::Auto));
    assert_eq!(r.unit, Some(GroupUnit::Month), "91 days is Month");
}

#[test]
fn the_timeline_is_in_date_order_whatever_the_sort() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let ids = small(&album);
    let engine = QueryEngine::new();
    let q = Query {
        sort: QuerySort::Name,
        ..timeline(QueryTimelineUnit::Day)
    };
    let r = run(&engine, &album, &q);
    assert_eq!(
        tile_ids(&engine, &album, &r.token),
        [ids[4], ids[0], ids[1], ids[2]],
        "oldest first, undated last, hidden left out"
    );
    assert_eq!(
        r.groups.unwrap(),
        [
            group("2024-06-01", 0, 1),
            group("2024-06-02", 1, 1),
            group("2024-06-03", 2, 1),
            group("none", 3, 1),
        ]
    );
    // The other views carry no groups.
    let grid = run(&engine, &album, &default_query());
    assert_eq!((grid.groups, grid.unit), (None, None));
}

#[test]
fn the_timeline_groups_only_what_passes_the_filter() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    small(&album);
    let engine = QueryEngine::new();
    let q = Query {
        filter: Filter {
            from: Some("2024-06-02".to_owned()),
            hidden: Some(HiddenFilter::Show),
            ..Filter::default()
        },
        ..timeline(QueryTimelineUnit::Auto)
    };
    let r = run(&engine, &album, &q);
    assert_eq!(r.count, 2);
    assert_eq!(
        r.groups.unwrap(),
        [group("2024-06-02", 0, 1), group("2024-06-03", 1, 1)]
    );
}

#[test]
fn an_empty_or_undated_timeline_still_says_its_unit() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let engine = QueryEngine::new();
    let r = run(&engine, &album, &timeline(QueryTimelineUnit::Auto));
    assert_eq!(r.groups, Some(Vec::new()));
    assert_eq!(r.unit, Some(GroupUnit::Day));
    add(&album, &[p("a.jpg", None), p("b.jpg", None)]);
    let r = run(&engine, &album, &timeline(QueryTimelineUnit::Month));
    assert_eq!(r.groups.unwrap(), [group("none", 0, 2)]);
    assert_eq!(r.unit, Some(GroupUnit::Month));
}
