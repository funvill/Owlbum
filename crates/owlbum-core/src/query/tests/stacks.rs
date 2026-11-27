//! Stacks (docs/05-organising.md, "Stacks" and "Selection and stacks"):
//! the face by each rule, a burst with one member passing, open and
//! closed, the face changing after a star and a hide, the map ignoring
//! stacks, and where the current photo goes.

use std::num::NonZeroU32;

use super::*;
use crate::index::Derived;

/// Four photos, answered as `[a, b, c, d]`: a, b and c are one burst (its
/// id is a's), taken a minute apart, with c, in another folder, its
/// keeper; d is in no burst and comes last.
fn burst(album: &Album) -> [i64; 4] {
    let ids = add(
        album,
        &[
            p("x/a.jpg", Some("2024-06-01T10:01:00")),
            p("x/b.jpg", Some("2024-06-01T10:02:00")),
            p("y/c.jpg", Some("2024-06-01T10:03:00")),
            p("x/d.jpg", Some("2024-06-02T10:00:00")),
        ],
    );
    let [a, b, c, d] = [ids[0], ids[1], ids[2], ids[3]];
    install(album, [a, b, c], c);
    [a, b, c, d]
}

/// Makes `members` one burst named after the first, with `keeper`.
fn install(album: &Album, members: [i64; 3], keeper: i64) {
    let id = |i: i64| u32::try_from(i).unwrap();
    let burst = NonZeroU32::new(id(members[0]));
    album.index().install_derived(
        members
            .iter()
            .map(|&m| {
                (
                    id(m),
                    Derived {
                        burst,
                        keeper: m == keeper,
                        ..Derived::default()
                    },
                )
            })
            .collect(),
    );
}

fn stack_of(engine: &QueryEngine, album: &Album, token: &str) -> Vec<(i64, Option<RowStack>)> {
    engine
        .window(album, &Overlay::default(), token, 0, WINDOW_MAX)
        .unwrap()
        .into_iter()
        .map(|r| (r.id, r.stack))
        .collect()
}

fn rs(id: i64, size: u32, open: bool, place: u32) -> Option<RowStack> {
    Some(RowStack {
        id,
        size,
        open,
        face: place == 1,
        place,
    })
}

#[test]
fn a_closed_stack_is_one_tile_at_its_keeper_and_counts_every_member() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let [a, b, c, d] = burst(&album);
    let engine = QueryEngine::new();
    let r = run(&engine, &album, &default_query());
    assert_eq!((r.count, r.photos, r.total), (2, 4, 4));
    assert_eq!(
        stack_of(&engine, &album, &r.token),
        vec![(c, rs(a, 3, false, 1)), (d, None)]
    );
    // One id per tile; the folded members are located by their stack.
    assert_eq!(engine.ids(&r.token, 0, 10).unwrap(), vec![c, d]);
    assert_eq!(
        engine.locate(&r.token, a).unwrap(),
        Some(Located::Folded { folded_in: a })
    );
    assert_eq!(
        engine.locate(&r.token, b).unwrap(),
        Some(Located::Folded { folded_in: a })
    );
    assert_eq!(
        engine.locate(&r.token, c).unwrap(),
        Some(Located::At { index: 0 })
    );
}

#[test]
fn an_open_stack_shows_its_members_after_the_face_in_sort_order() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let [a, b, c, d] = burst(&album);
    let engine = QueryEngine::new();
    let q = Query {
        open_stacks: vec![a, 999],
        ..default_query()
    };
    let r = run(&engine, &album, &q);
    assert_eq!((r.count, r.photos), (4, 4));
    assert_eq!(
        stack_of(&engine, &album, &r.token),
        vec![
            (c, rs(a, 3, true, 1)),
            (a, rs(a, 3, true, 2)),
            (b, rs(a, 3, true, 3)),
            (d, None)
        ]
    );
    assert_eq!(
        engine.locate(&r.token, b).unwrap(),
        Some(Located::At { index: 2 })
    );
}

#[test]
fn a_starred_member_is_the_face_before_the_keeper() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let [a, b, c, _] = burst(&album);
    change(
        &album,
        &format!("UPDATE photos SET star = 1 WHERE id = {b}"),
        &[b],
    );
    install(&album, [a, b, c], c);
    let engine = QueryEngine::new();
    let r = run(&engine, &album, &default_query());
    assert_eq!(
        stack_of(&engine, &album, &r.token)[0],
        (b, rs(a, 3, false, 1))
    );
}

#[test]
fn without_the_keeper_the_first_member_in_sort_order_is_the_face() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let [a, _, _, d] = burst(&album);
    let q = Query {
        filter: Filter {
            folder: Some("x".to_owned()),
            ..Filter::default()
        },
        ..default_query()
    };
    let engine = QueryEngine::new();
    let r = run(&engine, &album, &q);
    assert_eq!(
        stack_of(&engine, &album, &r.token),
        vec![(a, rs(a, 2, false, 1)), (d, None)]
    );
}

#[test]
fn a_burst_with_one_member_passing_is_a_plain_tile() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let [_, _, c, _] = burst(&album);
    let q = Query {
        filter: Filter {
            folder: Some("y".to_owned()),
            ..Filter::default()
        },
        ..default_query()
    };
    let engine = QueryEngine::new();
    let r = run(&engine, &album, &q);
    assert_eq!(stack_of(&engine, &album, &r.token), vec![(c, None)]);
}

#[test]
fn stacking_off_and_the_map_never_fold() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let [a, b, c, d] = burst(&album);
    let engine = QueryEngine::new();
    for q in [
        Query {
            stack: false,
            ..default_query()
        },
        Query {
            view: QueryView::Map,
            ..default_query()
        },
    ] {
        let r = run(&engine, &album, &q);
        assert_eq!((r.count, r.photos), (4, 4));
        assert_eq!(
            stack_of(&engine, &album, &r.token),
            vec![(a, None), (b, None), (c, None), (d, None)]
        );
    }
}

#[test]
fn a_timeline_group_counts_tiles_and_every_photo_of_its_stacks() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    burst(&album);
    let engine = QueryEngine::new();
    let q = Query {
        view: QueryView::Timeline,
        timeline_unit: QueryTimelineUnit::Day,
        ..default_query()
    };
    let r = run(&engine, &album, &q);
    let groups: Vec<(String, u32, u32, u32)> = r
        .groups
        .unwrap()
        .into_iter()
        .map(|g| (g.key, g.start, g.count, g.photos))
        .collect();
    assert_eq!(
        groups,
        vec![
            ("2024-06-01".to_owned(), 0, 1, 3),
            ("2024-06-02".to_owned(), 1, 1, 1)
        ]
    );
}

fn keep_current(token: &str, id: i64, index: u32) -> Keep {
    Keep {
        token: token.to_owned(),
        current: Some(PhotoAt { id, index }),
        anchor_id: Some(id),
        selection: vec![id],
        view: None,
    }
}

#[test]
fn closing_a_stack_gives_the_selection_to_its_face() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let [a, b, c, _] = burst(&album);
    let engine = QueryEngine::new();
    let open = run(
        &engine,
        &album,
        &Query {
            open_stacks: vec![a],
            ..default_query()
        },
    );
    let kept = engine
        .run(
            &album,
            &default_query(),
            Some(&keep_current(&open.token, b, 2)),
        )
        .unwrap()
        .kept
        .unwrap();
    let face = Some(PhotoAt { id: c, index: 0 });
    assert_eq!(kept.current, face);
    assert_eq!(kept.anchor, face);
    assert_eq!(kept.selection, vec![c]);
}

#[test]
fn closing_a_stack_on_a_selected_member_puts_its_face_in_the_selection() {
    // d is current; b, a member that is not current, is also selected. The
    // stack folds, so its face takes b's place in the selection (#144).
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let [a, b, c, d] = burst(&album);
    let engine = QueryEngine::new();
    let open = run(
        &engine,
        &album,
        &Query {
            open_stacks: vec![a],
            ..default_query()
        },
    );
    let keep = Keep {
        selection: vec![b, d],
        ..keep_current(&open.token, d, 3)
    };
    let kept = engine
        .run(&album, &default_query(), Some(&keep))
        .unwrap()
        .kept
        .unwrap();
    assert_eq!(kept.current, Some(PhotoAt { id: d, index: 1 }));
    assert_eq!(kept.anchor, Some(PhotoAt { id: d, index: 1 }));
    assert_eq!(kept.selection, vec![c, d]);
}

#[test]
fn a_hidden_face_hands_the_stack_and_the_selection_to_the_new_face() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let [a, b, c, d] = burst(&album);
    let engine = QueryEngine::new();
    let before = run(&engine, &album, &default_query());
    change(
        &album,
        &format!("UPDATE photos SET hidden = 1 WHERE id = {c}"),
        &[c],
    );
    install(&album, [a, b, c], b);
    let r = engine
        .run(
            &album,
            &default_query(),
            Some(&keep_current(&before.token, c, 0)),
        )
        .unwrap();
    assert_eq!(
        stack_of(&engine, &album, &r.token),
        vec![(b, rs(a, 2, false, 1)), (d, None)]
    );
    assert_eq!(r.kept.unwrap().current, Some(PhotoAt { id: b, index: 0 }));
}
