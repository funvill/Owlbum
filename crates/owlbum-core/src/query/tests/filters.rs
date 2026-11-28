//! Filters, chips, counts and facets (docs/05-organising.md, "The list of
//! filters", "The filtered row" and "Counts beside options").

use std::num::NonZeroU32;

use super::*;
use crate::index::Derived;

/// Six photos, one per letter, answered as `[a, b, c, d, e, f]`:
///
/// | | path | date | shape | other |
/// |---|---|---|---|---|
/// | a | `a.jpg` | 2024-06-01 10:00 | landscape | starred, GPS, low quality |
/// | b | `b.jpg` | 2024-06-15 23:59 | portrait | Ann on it, in a burst |
/// | c | `Trip/c.jpg` | 2024-06-30 00:00 | square | keyword "lake", object "boat" |
/// | d | `Trip/Day/d.jpg` | none | unknown | hidden, starred |
/// | e | `Trip/e.jpg` | 2024-07-01 | landscape | Ann and Bo on it, keyword "lake", in a burst |
/// | f | `f.jpg` | 2024-05-31 23:59 | landscape | typed place |
fn filterable(album: &Album) -> [i64; 6] {
    let ids = add(
        album,
        &[
            p("a.jpg", Some("2024-06-01T10:00:00")),
            p("b.jpg", Some("2024-06-15T23:59:59")),
            p("Trip/c.jpg", Some("2024-06-30T00:00:00")),
            hidden("Trip/Day/d.jpg", None),
            p("Trip/e.jpg", Some("2024-07-01T00:00:00")),
            p("f.jpg", Some("2024-05-31T23:59:59")),
        ],
    );
    let [a, b, c, d, e, f] = ids[..] else {
        unreachable!()
    };
    change(
        album,
        &format!(
            "UPDATE photos SET star = 1, gps_lat = 49.2, gps_lon = -123.1 WHERE id = {a};
             UPDATE photos SET width = 3000, height = 4000 WHERE id = {b};
             UPDATE photos SET width = 3000, height = 3000 WHERE id = {c};
             UPDATE photos SET width = 0, height = 0, star = 1 WHERE id = {d};
             UPDATE photos SET loc_lat = 1.0, loc_lon = 2.0 WHERE id = {f};
             INSERT INTO people (id, name) VALUES (1, 'Ann'), (2, 'Bo');
             INSERT INTO photo_people (photo_id, person_id) VALUES ({b}, 1), ({e}, 1), ({e}, 2);
             INSERT INTO photo_labels (kind, label, photo_id) VALUES
               ('keyword', 'lake', {c}), ('object', 'boat', {c}), ('keyword', 'lake', {e});"
        ),
        &ids,
    );
    let id = |i: i64| u32::try_from(i).unwrap();
    let burst = NonZeroU32::new(7);
    album.index().install_derived(vec![
        (
            id(a),
            Derived {
                quality_flags: 1,
                ..Derived::default()
            },
        ),
        (
            id(b),
            Derived {
                burst,
                ..Derived::default()
            },
        ),
        (
            id(e),
            Derived {
                burst,
                ..Derived::default()
            },
        ),
    ]);
    [a, b, c, d, e, f]
}

fn filtered(filter: Filter) -> Query {
    Query {
        filter,
        ..default_query()
    }
}

/// The ids that pass `filter`, sorted. Stacking is off, so b and e (a
/// burst) are each a tile.
fn passing(engine: &QueryEngine, album: &Album, filter: Filter) -> Vec<i64> {
    let q = Query {
        stack: false,
        ..filtered(filter)
    };
    let r = run(engine, album, &q);
    let mut ids = tile_ids(engine, album, &r.token);
    ids.sort_unstable();
    ids
}

fn tag(kind: TagKind, value: &str) -> TagFilter {
    TagFilter {
        kind,
        value: value.to_owned(),
    }
}

#[test]
fn every_filter_alone_passes_the_expected_photos() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let [a, b, c, d, e, f] = filterable(&album);
    let engine = QueryEngine::new();
    let none = Filter::default();
    let cases: Vec<(&str, Filter, Vec<i64>)> = vec![
        ("nothing", none.clone(), vec![a, b, c, e, f]),
        (
            "starred",
            Filter {
                star: Some(StarFilter::Yes),
                ..none.clone()
            },
            vec![a],
        ),
        (
            "not starred",
            Filter {
                star: Some(StarFilter::No),
                ..none.clone()
            },
            vec![b, c, e, f],
        ),
        (
            "portrait",
            Filter {
                shape: Some(ShapeFilter::Portrait),
                ..none.clone()
            },
            vec![b],
        ),
        // A square photo, and one of unknown size, match neither shape.
        (
            "landscape",
            Filter {
                shape: Some(ShapeFilter::Landscape),
                ..none.clone()
            },
            vec![a, e, f],
        ),
        (
            "hidden shown",
            Filter {
                hidden: Some(HiddenFilter::Show),
                ..none.clone()
            },
            vec![a, b, c, d, e, f],
        ),
        (
            "only hidden",
            Filter {
                hidden: Some(HiddenFilter::Only),
                ..none.clone()
            },
            vec![d],
        ),
        // Both ends inclusive, by the day; a photo with no date fails.
        (
            "June",
            Filter {
                from: Some("2024-06-01".into()),
                to: Some("2024-06-30".into()),
                ..none.clone()
            },
            vec![a, b, c],
        ),
        (
            "from",
            Filter {
                from: Some("2024-06-30".into()),
                ..none.clone()
            },
            vec![c, e],
        ),
        (
            "until",
            Filter {
                to: Some("2024-05-31".into()),
                ..none.clone()
            },
            vec![f],
        ),
        // Exactly the folder: not the folders beneath it.
        (
            "Trip",
            Filter {
                folder: Some("Trip".into()),
                ..none.clone()
            },
            vec![c, e],
        ),
        (
            "top folder",
            Filter {
                folder: Some(String::new()),
                ..none.clone()
            },
            vec![a, b, f],
        ),
        (
            "no such folder",
            Filter {
                folder: Some("Nowhere".into()),
                ..none.clone()
            },
            vec![],
        ),
        // GPS or a typed place.
        (
            "has location",
            Filter {
                has_location: Some(true),
                ..none.clone()
            },
            vec![a, f],
        ),
        (
            "has people",
            Filter {
                has_people: Some(true),
                ..none.clone()
            },
            vec![b, e],
        ),
        // Names ignore case; every listed person must be on the photo.
        (
            "Ann",
            Filter {
                people: Some(vec!["ann".into()]),
                ..none.clone()
            },
            vec![b, e],
        ),
        (
            "Ann and Bo",
            Filter {
                people: Some(vec!["Ann".into(), "Bo".into()]),
                ..none.clone()
            },
            vec![e],
        ),
        (
            "unknown person",
            Filter {
                people: Some(vec!["Cy".into()]),
                ..none.clone()
            },
            vec![],
        ),
        // Tags are lower-cased and trimmed, and must equal a whole label.
        (
            "lake",
            Filter {
                tags: Some(vec![tag(TagKind::Keyword, " Lake ")]),
                ..none.clone()
            },
            vec![c, e],
        ),
        (
            "la",
            Filter {
                tags: Some(vec![tag(TagKind::Keyword, "la")]),
                ..none.clone()
            },
            vec![],
        ),
        (
            "boat",
            Filter {
                tags: Some(vec![tag(TagKind::Object, "boat")]),
                ..none.clone()
            },
            vec![c],
        ),
        (
            "boat as keyword",
            Filter {
                tags: Some(vec![tag(TagKind::Keyword, "boat")]),
                ..none.clone()
            },
            vec![],
        ),
        (
            "low quality",
            Filter {
                low_quality: Some(true),
                ..none.clone()
            },
            vec![a],
        ),
        (
            "near-duplicates",
            Filter {
                near_duplicates: Some(true),
                ..none.clone()
            },
            vec![b, e],
        ),
    ];
    for (what, filter, want) in cases {
        assert_eq!(passing(&engine, &album, filter), want, "{what}");
    }
}

#[test]
fn filters_in_pairs_must_all_pass() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let [a, _b, c, d, e, _f] = filterable(&album);
    let engine = QueryEngine::new();
    let pairs: Vec<(Filter, Vec<i64>)> = vec![
        (
            Filter {
                star: Some(StarFilter::Yes),
                hidden: Some(HiddenFilter::Show),
                ..Filter::default()
            },
            vec![a, d],
        ),
        (
            Filter {
                folder: Some("Trip".into()),
                tags: Some(vec![tag(TagKind::Keyword, "lake")]),
                people: Some(vec!["Bo".into()]),
                ..Filter::default()
            },
            vec![e],
        ),
        (
            Filter {
                shape: Some(ShapeFilter::Landscape),
                near_duplicates: Some(true),
                ..Filter::default()
            },
            vec![e],
        ),
        (
            Filter {
                to: Some("2024-06-30".into()),
                tags: Some(vec![tag(TagKind::Keyword, "lake")]),
                ..Filter::default()
            },
            vec![c],
        ),
    ];
    for (filter, want) in pairs {
        assert_eq!(passing(&engine, &album, filter.clone()), want, "{filter:?}");
    }
}

#[test]
fn counts_x_of_y_and_y_counts_hidden_photos_only_when_they_are_shown() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    filterable(&album);
    let engine = QueryEngine::new();
    let r = run(&engine, &album, &default_query());
    assert_eq!((r.photos, r.total, r.album_photos), (5, 5, 6));
    assert!(!r.filtered);
    assert_eq!(r.panel_count, 0);
    assert!(r.chips.is_empty());

    let r = run(
        &engine,
        &album,
        &filtered(Filter {
            star: Some(StarFilter::Yes),
            ..Filter::default()
        }),
    );
    assert_eq!((r.count, r.photos, r.total), (1, 1, 5));
    assert!(r.filtered);

    let r = run(
        &engine,
        &album,
        &filtered(Filter {
            hidden: Some(HiddenFilter::Only),
            ..Filter::default()
        }),
    );
    assert_eq!((r.photos, r.total), (1, 6));
}

#[test]
fn chips_come_in_the_order_of_the_list_and_each_removes_its_own_filter() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    filterable(&album);
    let engine = QueryEngine::new();
    let filter = Filter {
        near_duplicates: Some(true),
        tags: Some(vec![
            tag(TagKind::Keyword, "Lake"),
            tag(TagKind::Object, "boat"),
        ]),
        people: Some(vec!["Ann".into()]),
        folder: Some(String::new()),
        to: Some("2024-06-30".into()),
        from: Some("2024-06-01".into()),
        star: Some(StarFilter::Yes),
        has_location: Some(false),
        ..Filter::default()
    };
    let r = run(&engine, &album, &filtered(filter.clone()));
    let texts: Vec<(&str, &str)> = r
        .chips
        .iter()
        .map(|c| (c.id.as_str(), c.text.as_str()))
        .collect();
    assert_eq!(
        texts,
        [
            ("star", "Starred"),
            ("dates", "2024-06-01 \u{2192} 2024-06-30"),
            ("folder", "Top folder"),
            ("person:Ann", "Ann"),
            ("tag:keyword:Lake", "Lake"),
            ("tag:object:boat", "boat"),
            ("near_duplicates", "Near-duplicates"),
        ]
    );
    // From and To are one chip; each tag and person is one. Search text has none.
    assert_eq!(r.panel_count, 7);
    let dates = &r.chips[1].remove;
    assert_eq!((dates.from.as_deref(), dates.to.as_deref()), (None, None));
    assert_eq!(dates.star, Some(StarFilter::Yes));
    let lake = &r.chips[4].remove;
    assert_eq!(
        lake.tags.as_deref(),
        Some(&[tag(TagKind::Object, "boat")][..])
    );
    // The last person removed leaves no list behind: absent has one form.
    assert_eq!(r.chips[3].remove.people, None);
    assert_eq!(r.chips[3].remove.has_location, None);

    let one_end = |from: Option<&str>, to: Option<&str>| {
        let f = Filter {
            from: from.map(Into::into),
            to: to.map(Into::into),
            ..Filter::default()
        };
        run(&engine, &album, &filtered(f)).chips[0].text.clone()
    };
    assert_eq!(one_end(Some("2024-06-01"), None), "From 2024-06-01");
    assert_eq!(one_end(None, Some("2024-06-30")), "Until 2024-06-30");
    let trip = run(
        &engine,
        &album,
        &filtered(Filter {
            folder: Some("Trip/Day".into()),
            ..Filter::default()
        }),
    );
    assert_eq!(trip.chips[0].text, "Trip/Day");
}

#[test]
fn every_filter_field_has_a_chip() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    filterable(&album);
    let engine = QueryEngine::new();
    let all = Filter {
        star: Some(StarFilter::Yes),
        shape: Some(ShapeFilter::Portrait),
        hidden: Some(HiddenFilter::Show),
        from: Some("2024-06-01".into()),
        to: Some("2024-06-30".into()),
        folder: Some("Trip".into()),
        has_location: Some(true),
        has_people: Some(true),
        people: Some(vec!["Ann".into()]),
        tags: Some(vec![tag(TagKind::Keyword, "lake")]),
        low_quality: Some(true),
        near_duplicates: Some(true),
        kinds: Some(vec![KindFilter::Still, KindFilter::Raw]),
        formats: Some(vec!["png".into()]),
    };
    let fields = serde_json::to_value(&all).unwrap();
    let fields: Vec<&str> = fields
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let r = run(&engine, &album, &filtered(all.clone()));
    let ids: Vec<&str> = r.chips.iter().map(|c| c.id.as_str()).collect();
    for field in fields {
        let id = match field {
            "from" | "to" => "dates",
            "people" => "person:Ann",
            "tags" => "tag:keyword:lake",
            "formats" => "format:png",
            other => other,
        };
        assert!(ids.contains(&id), "no chip for the {field} filter");
    }
    // A Filter with every field set answers as many chips as panel filters.
    assert_eq!(r.panel_count, 13);
}

#[test]
fn facets_count_each_option_with_every_other_filter_kept() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    filterable(&album);
    let engine = QueryEngine::new();
    let q = Query {
        want_facets: true,
        ..default_query()
    };
    let f = run(&engine, &album, &q).facets.unwrap();
    assert_eq!((f.star.yes, f.star.no), (1, 4));
    assert_eq!((f.shape.portrait, f.shape.landscape), (1, 3));
    // Hidden follows the Hidden filter like any other filter.
    assert_eq!(f.hidden, 1);
    assert_eq!((f.has_location, f.has_people), (2, 2));
    assert_eq!((f.low_quality, f.near_duplicates), (1, 2));
    let folders: Vec<(&str, &str, u32)> = f
        .folders
        .iter()
        .map(|x| (x.folder.as_str(), x.label.as_str(), x.count))
        .collect();
    // The top folder first; a folder of only hidden photos is listed with 0.
    assert_eq!(
        folders,
        [
            ("", "Top folder", 3),
            ("Trip", "Trip", 2),
            ("Trip/Day", "Trip/Day", 0)
        ]
    );

    // Starred on: the Stars options leave it out, the others keep it.
    let q = Query {
        want_facets: true,
        filter: Filter {
            star: Some(StarFilter::Yes),
            ..Filter::default()
        },
        ..default_query()
    };
    let r = run(&engine, &album, &q);
    let f = r.facets.unwrap();
    assert_eq!((f.star.yes, f.star.no), (1, 4));
    assert_eq!((f.shape.portrait, f.shape.landscape), (0, 1));
    // d is starred and hidden: it fails only Hidden, so it counts there.
    assert_eq!(f.hidden, 1);
    assert_eq!(f.has_location, 1);
    assert_eq!(f.folders[0].count, 1);
    // `query_facets` answers the same for the token.
    assert_eq!(engine.facets(&album, &r.token).unwrap(), f);

    // A photo failing a filter with no facet (people) counts nowhere.
    let q = Query {
        want_facets: true,
        filter: Filter {
            people: Some(vec!["Bo".into()]),
            ..Filter::default()
        },
        ..default_query()
    };
    let f = run(&engine, &album, &q).facets.unwrap();
    assert_eq!((f.star.yes, f.star.no), (0, 1));
    assert_eq!(f.near_duplicates, 1);
    assert!(run(&engine, &album, &default_query()).facets.is_none());
}

#[test]
fn a_value_that_is_not_allowed_answers_bad_query() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    filterable(&album);
    let engine = QueryEngine::new();
    for filter in [
        Filter {
            from: Some("2024-02-30".into()),
            ..Filter::default()
        },
        Filter {
            to: Some("2024-6-1".into()),
            ..Filter::default()
        },
        Filter {
            people: Some(vec!["  ".into()]),
            ..Filter::default()
        },
        Filter {
            tags: Some(vec![tag(TagKind::Keyword, " ")]),
            ..Filter::default()
        },
    ] {
        assert_eq!(
            code(engine.run(&album, &filtered(filter), None)),
            ErrorCode::BadQuery
        );
    }
    assert_eq!(
        code(engine.facets(&album, "q-gone")),
        ErrorCode::QueryExpired
    );
}

/// The filterable album with a media type each (owner's feedback, #165):
/// a animated, b a still in the second format, c a still, d hidden, e a
/// video, f a RAW file. The rest keep the first format the helper writes.
fn typed(album: &Album) -> [i64; 6] {
    let ids = filterable(album);
    let [a, b, _, _, e, f] = ids;
    change(
        album,
        &format!(
            "UPDATE photos SET duration_ms = 1200 WHERE id = {a};
             UPDATE photos SET format = 'png' WHERE id = {b};
             UPDATE photos SET kind = 'video', duration_ms = 5000 WHERE id = {e};
             UPDATE photos SET kind = 'raw' WHERE id = {f};"
        ),
        &ids,
    );
    ids
}

#[test]
fn the_media_type_filter_keeps_kinds_and_formats() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let [a, b, c, _, e, f] = typed(&album);
    let engine = QueryEngine::new();

    // No videos: every kind but video.
    let no_videos = filtered(Filter {
        kinds: Some(vec![
            KindFilter::Still,
            KindFilter::Animated,
            KindFilter::Raw,
        ]),
        ..Filter::default()
    });
    let r = run(&engine, &album, &no_videos);
    let mut shown = tile_ids(&engine, &album, &r.token);
    shown.sort_unstable();
    assert_eq!(shown, vec![a, b, c, f]);
    assert_eq!(r.chips.len(), 1);
    assert_eq!(r.chips[0].text, "No videos");
    assert_eq!(r.chips[0].remove, Filter::default());
    assert_eq!(r.panel_count, 1);

    // Only the second format.
    let only = filtered(Filter {
        formats: Some(vec!["png".into()]),
        ..Filter::default()
    });
    let r = run(&engine, &album, &only);
    assert_eq!(tile_ids(&engine, &album, &r.token), vec![b]);
    assert_eq!(r.chips[0].id, "format:png");
    assert_eq!(r.chips[0].text, "PNG");

    // Kinds shown by name when more than one is left out; videos alone.
    let r = run(
        &engine,
        &album,
        &filtered(Filter {
            kinds: Some(vec![KindFilter::Video]),
            ..Filter::default()
        }),
    );
    assert_eq!(tile_ids(&engine, &album, &r.token), vec![e]);
    assert_eq!(r.chips[0].text, "Videos");
}

#[test]
fn every_kind_or_an_unknown_format_is_no_filter() {
    let all = Filter {
        kinds: Some(KindFilter::ALL.to_vec()),
        formats: Some(vec!["no-such-format".into()]),
        ..Filter::default()
    };
    assert!(all.is_empty());
    let twice = Filter {
        kinds: Some(vec![
            KindFilter::Video,
            KindFilter::Still,
            KindFilter::Video,
        ]),
        ..Filter::default()
    };
    assert_eq!(
        twice.normalised().kinds,
        Some(vec![KindFilter::Still, KindFilter::Video])
    );
}

#[test]
fn the_media_type_facets_count_kinds_and_list_the_formats_in_the_scope() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    typed(&album);
    let engine = QueryEngine::new();
    let q = Query {
        want_facets: true,
        ..default_query()
    };
    let f = run(&engine, &album, &q).facets.unwrap();
    // d is hidden, so it counts nowhere.
    assert_eq!(
        (f.kinds.still, f.kinds.animated, f.kinds.raw, f.kinds.video),
        (2, 1, 1, 1)
    );
    assert_eq!(f.formats.len(), 2);
    let second = f.formats.iter().find(|x| x.format == "png").unwrap();
    assert_eq!((second.label.as_str(), second.count), ("PNG", 1));
    assert_eq!(f.formats.iter().map(|x| x.count).sum::<u32>(), 5);

    // With the format filter on, the kinds count only what passes it, and
    // every format keeps its own count.
    let q = Query {
        want_facets: true,
        filter: Filter {
            formats: Some(vec!["png".into()]),
            ..Filter::default()
        },
        ..default_query()
    };
    let f = run(&engine, &album, &q).facets.unwrap();
    assert_eq!(
        (f.kinds.still, f.kinds.animated, f.kinds.raw, f.kinds.video),
        (1, 0, 0, 0)
    );
    assert_eq!(f.formats.iter().map(|x| x.count).sum::<u32>(), 5);
}
