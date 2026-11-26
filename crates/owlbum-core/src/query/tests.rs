use std::path::Path;
use std::sync::Arc;

use serde_json::json;

use super::*;
use crate::album::{Album, OpenOptions, sqlite_error};
use crate::error::{AppResult, ErrorCode};
use crate::events::Emitter;
use crate::media::MediaKind;
use crate::scan::memory::Overlay;
use crate::scan::{Request, ScanEnv, Scanner};
use crate::text;

// ---- Helpers ----

struct Quiet;
impl Emitter for Quiet {
    fn emit_json(&self, _: &'static str, _: serde_json::Value) {}
}

/// One photo to put in a test album.
#[derive(Clone, Copy)]
struct P<'a> {
    path: &'a str,
    date: Option<&'a str>,
    hidden: bool,
}

const fn p<'a>(path: &'a str, date: Option<&'a str>) -> P<'a> {
    P {
        path,
        date,
        hidden: false,
    }
}

const fn hidden<'a>(path: &'a str, date: Option<&'a str>) -> P<'a> {
    P {
        path,
        date,
        hidden: true,
    }
}

fn album(dir: &Path) -> Album {
    Album::open(dir, &OpenOptions::default()).unwrap()
}

/// Inserts photos through the writer, so the album index follows, and
/// answers their ids in the order given.
fn add(album: &Album, photos: &[P<'_>]) -> Vec<i64> {
    let photos: Vec<(String, Option<String>, bool)> = photos
        .iter()
        .map(|p| (p.path.to_owned(), p.date.map(str::to_owned), p.hidden))
        .collect();
    album
        .write_photos(move |tx, touched| {
            let mut ids = Vec::new();
            for (path, date, hidden) in &photos {
                let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
                tx.execute(
                    "INSERT INTO photos (rel_path, rel_dir, file_name, size, mtime_ms, file_key, thumb_key, kind, format, width, height, scan_version, captured_at, hidden)
                     VALUES (?1, ?2, ?3, 1, 1, 'fk', 'tk-' || ?3, 'image', 'jpeg', 4000, 3000, 1, ?4, ?5)",
                    (path, dir, name, date, hidden),
                )
                .map_err(sqlite_error(text::album::WRITING_DATABASE))?;
                let id = tx.last_insert_rowid();
                touched.photo(id);
                ids.push(id);
            }
            Ok(ids)
        })
        .unwrap()
}

/// Runs one SQL statement on the album through the writer, reporting `ids`.
fn change(album: &Album, sql: &str, ids: &[i64]) {
    let sql = sql.to_owned();
    let ids = ids.to_vec();
    album
        .write_photos(move |tx, touched| {
            tx.execute_batch(&sql)
                .map_err(sqlite_error(text::album::WRITING_DATABASE))?;
            touched.photos(ids);
            Ok(())
        })
        .unwrap();
}

/// The query the page sends for its default view.
fn default_query() -> Query {
    Query {
        stack: true,
        ..Query::default()
    }
}

fn run(engine: &QueryEngine, album: &Album, query: &Query) -> QueryResult {
    engine.run(album, query, None).unwrap()
}

/// Every row of a result's tiles, by id.
fn tile_ids(engine: &QueryEngine, album: &Album, token: &str) -> Vec<i64> {
    engine
        .window(album, &Overlay::default(), token, 0, WINDOW_MAX)
        .unwrap()
        .iter()
        .map(|r| r.id)
        .collect()
}

fn code<T: std::fmt::Debug>(r: AppResult<T>) -> ErrorCode {
    r.unwrap_err().code
}

/// A small album: three dated photos out of order, one undated, one
/// hidden.
fn small(album: &Album) -> Vec<i64> {
    add(
        album,
        &[
            p("b.jpg", Some("2024-06-02T10:00:00")),
            p("a.jpg", Some("2024-06-03T10:00:00")),
            p("Trip/c.jpg", None),
            hidden("d.jpg", Some("2024-06-01T10:00:00")),
            p("e.jpg", Some("2024-06-01T09:00:00")),
        ],
    )
}

// ---- query_run: the default view ----

#[test]
fn the_default_query_lists_every_shown_photo_by_date_undated_last() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let ids = small(&album);
    let engine = QueryEngine::new();
    let r = run(&engine, &album, &default_query());
    assert_eq!(
        tile_ids(&engine, &album, &r.token),
        [ids[4], ids[0], ids[1], ids[2]]
    );
    assert_eq!(r.count, 4);
    assert_eq!(r.photos, 4, "X counts photos");
    assert_eq!(r.total, 4, "Y leaves hidden photos out");
    assert_eq!(r.album_photos, 5, "album_photos includes hidden ones");
    assert!(!r.filtered);
    assert_eq!(r.panel_count, 0);
    assert!(r.chips.is_empty());
    assert_eq!(r.kept, None);
    assert_eq!(r.groups, None);
    assert_eq!(r.unit, None);
    assert_eq!(r.facets, None);
}

#[test]
fn dates_tie_by_the_name_order() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let ids = add(
        &album,
        &[
            p("img_10.jpg", Some("2024-01-01T00:00:00")),
            p("IMG_9.jpg", Some("2024-01-01T00:00:00")),
            p("x/IMG_9.jpg", Some("2024-01-01T00:00:00")),
        ],
    );
    let engine = QueryEngine::new();
    let r = run(&engine, &album, &default_query());
    assert_eq!(
        tile_ids(&engine, &album, &r.token),
        [ids[1], ids[2], ids[0]]
    );
}

#[test]
fn the_name_sort_is_the_name_order() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let ids = add(
        &album,
        &[
            p("img_10.jpg", Some("2020-01-01T00:00:00")),
            p("IMG_9.jpg", Some("2024-01-01T00:00:00")),
            p("a.jpg", None),
        ],
    );
    let engine = QueryEngine::new();
    let q = Query {
        sort: QuerySort::Name,
        ..default_query()
    };
    let r = run(&engine, &album, &q);
    assert_eq!(
        tile_ids(&engine, &album, &r.token),
        [ids[2], ids[1], ids[0]]
    );
}

#[test]
fn the_map_view_lists_the_same_photos() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    small(&album);
    let engine = QueryEngine::new();
    let grid = run(&engine, &album, &default_query());
    let map = run(
        &engine,
        &album,
        &Query {
            view: QueryView::Map,
            ..default_query()
        },
    );
    assert_eq!(
        tile_ids(&engine, &album, &grid.token),
        tile_ids(&engine, &album, &map.token)
    );
    assert_eq!(map.count, grid.count);
}

#[test]
fn an_empty_album_gives_an_empty_result() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let engine = QueryEngine::new();
    let r = run(&engine, &album, &default_query());
    assert_eq!((r.count, r.photos, r.total, r.album_photos), (0, 0, 0, 0));
    assert!(tile_ids(&engine, &album, &r.token).is_empty());
}

#[test]
fn values_the_doc_treats_as_absent_are_accepted() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    small(&album);
    let engine = QueryEngine::new();
    let q = Query {
        open_stacks: vec![999_999],
        search: Some(" a ".to_owned()),
        filter: Filter {
            has_location: Some(false),
            has_people: Some(false),
            low_quality: Some(false),
            near_duplicates: Some(false),
            people: Some(vec![]),
            tags: Some(vec![]),
            ..Filter::default()
        },
        ..default_query()
    };
    let r = engine.run(&album, &q, None).unwrap();
    assert_eq!(r.count, 4);
    assert!(!r.filtered, "text under the minimum length is not a filter");
    let none = Query {
        stack: false,
        search: None,
        ..default_query()
    };
    assert_eq!(engine.run(&album, &none, None).unwrap().count, 4);
}

#[test]
fn parts_not_built_yet_answer_bad_query() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    small(&album);
    let engine = QueryEngine::new();
    let with_filter = |filter: Filter| Query {
        filter,
        ..default_query()
    };
    let refused = [
        Query {
            view: QueryView::Timeline,
            ..default_query()
        },
        Query {
            sort: QuerySort::Score,
            ..default_query()
        },
        Query {
            sort: QuerySort::Interesting,
            ..default_query()
        },
        Query {
            sort: QuerySort::Colour,
            ..default_query()
        },
        Query {
            search: Some("lake".to_owned()),
            ..default_query()
        },
        Query {
            want_facets: true,
            ..default_query()
        },
        with_filter(Filter {
            star: Some(StarFilter::Yes),
            ..Filter::default()
        }),
        with_filter(Filter {
            shape: Some(ShapeFilter::Portrait),
            ..Filter::default()
        }),
        with_filter(Filter {
            hidden: Some(HiddenFilter::Show),
            ..Filter::default()
        }),
        with_filter(Filter {
            from: Some("2024-06-01".to_owned()),
            ..Filter::default()
        }),
        with_filter(Filter {
            to: Some("2024-06-01".to_owned()),
            ..Filter::default()
        }),
        with_filter(Filter {
            folder: Some(String::new()),
            ..Filter::default()
        }),
        with_filter(Filter {
            has_location: Some(true),
            ..Filter::default()
        }),
        with_filter(Filter {
            has_people: Some(true),
            ..Filter::default()
        }),
        with_filter(Filter {
            people: Some(vec!["Ann".to_owned()]),
            ..Filter::default()
        }),
        with_filter(Filter {
            tags: Some(vec![TagFilter {
                kind: TagKind::Keyword,
                value: "lake".to_owned(),
            }]),
            ..Filter::default()
        }),
        with_filter(Filter {
            low_quality: Some(true),
            ..Filter::default()
        }),
        with_filter(Filter {
            near_duplicates: Some(true),
            ..Filter::default()
        }),
    ];
    for q in refused {
        let err = engine.run(&album, &q, None).unwrap_err();
        assert_eq!(err.code, ErrorCode::BadQuery, "{q:?}");
        assert!(err.message.contains("yet"), "{}", err.message);
    }
}

#[test]
fn a_refused_query_makes_no_token() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    small(&album);
    let engine = QueryEngine::new();
    let first = run(&engine, &album, &default_query());
    for _ in 0..TOKENS_KEPT {
        let bad = Query {
            sort: QuerySort::Score,
            ..default_query()
        };
        assert!(engine.run(&album, &bad, None).is_err());
    }
    assert!(engine.locate(&first.token, 1).is_ok());
}

// ---- Revision ----

#[test]
fn a_result_carries_the_album_revision_it_was_built_from() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    small(&album);
    let engine = QueryEngine::new();
    let before = album.revision();
    assert_eq!(run(&engine, &album, &default_query()).revision, before);
    let next = album.next_revision();
    assert_eq!(run(&engine, &album, &default_query()).revision, next);
}

// ---- Tokens ----

#[test]
fn the_last_four_tokens_are_kept_and_older_ones_expire() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    small(&album);
    let engine = QueryEngine::new();
    let tokens: Vec<String> = (0..=TOKENS_KEPT)
        .map(|_| run(&engine, &album, &default_query()).token)
        .collect();
    assert_eq!(TOKENS_KEPT, 4);
    let unique: std::collections::HashSet<&String> = tokens.iter().collect();
    assert_eq!(unique.len(), tokens.len(), "every run makes a new token");
    let overlay = Overlay::default();
    assert_eq!(
        code(engine.window(&album, &overlay, &tokens[0], 0, 10)),
        ErrorCode::QueryExpired
    );
    assert_eq!(code(engine.locate(&tokens[0], 1)), ErrorCode::QueryExpired);
    for t in &tokens[1..] {
        assert_eq!(engine.window(&album, &overlay, t, 0, 10).unwrap().len(), 4);
    }
}

#[test]
fn an_unknown_token_answers_query_expired() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let engine = QueryEngine::new();
    let err = engine
        .window(&album, &Overlay::default(), "nope", 0, 10)
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::QueryExpired);
    assert_eq!(err.message, text::query::EXPIRED);
}

#[test]
fn tokens_are_unique_across_engines() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let a = QueryEngine::new();
    let b = QueryEngine::new();
    let ta = run(&a, &album, &default_query()).token;
    let tb = run(&b, &album, &default_query()).token;
    assert_ne!(ta, tb);
    assert_eq!(code(b.locate(&ta, 1)), ErrorCode::QueryExpired);
}

// ---- query_window ----

#[test]
fn a_window_reads_the_rows_of_the_result() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let ids = small(&album);
    change(
        &album,
        &format!(
            "UPDATE photos SET star = 1, kind = 'video', duration_ms = 4500, width = 1000, height = 2000 WHERE id = {}",
            ids[2]
        ),
        &[ids[2]],
    );
    let engine = QueryEngine::new();
    let r = run(&engine, &album, &default_query());
    let rows = engine
        .window(&album, &Overlay::default(), &r.token, 3, 10)
        .unwrap();
    assert_eq!(rows.len(), 1, "a limit past the end answers the tail");
    let row = &rows[0];
    assert_eq!(row.id, ids[2]);
    assert_eq!(row.rel_path, "Trip/c.jpg");
    assert_eq!(row.file_name, "c.jpg");
    assert_eq!(row.thumb_key, "tk-c.jpg");
    assert!((row.aspect - 0.5).abs() < 1e-9);
    assert_eq!(row.kind, MediaKind::Video);
    assert_eq!(row.duration_ms, Some(4500));
    assert!(row.star);
    assert!(!row.hidden);
    assert_eq!(row.score, None, "nothing is scored yet");
    assert_eq!(row.stack, None);
    assert_eq!(row.edit_plan, None);
}

#[test]
fn windows_at_the_start_middle_and_end() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let names: Vec<String> = (0..20).map(|i| format!("p{i:02}.jpg")).collect();
    let photos: Vec<P<'_>> = names.iter().map(|n| p(n, None)).collect();
    let ids = add(&album, &photos);
    let engine = QueryEngine::new();
    let r = run(&engine, &album, &default_query());
    let o = Overlay::default();
    let ids_of = |rows: Vec<Row>| rows.into_iter().map(|r| r.id).collect::<Vec<_>>();
    assert_eq!(
        ids_of(engine.window(&album, &o, &r.token, 0, 3).unwrap()),
        ids[0..3]
    );
    assert_eq!(
        ids_of(engine.window(&album, &o, &r.token, 9, 2).unwrap()),
        ids[9..11]
    );
    assert_eq!(
        ids_of(engine.window(&album, &o, &r.token, 18, 5).unwrap()),
        ids[18..20]
    );
    assert!(
        engine
            .window(&album, &o, &r.token, 20, 5)
            .unwrap()
            .is_empty()
    );
    assert!(
        engine
            .window(&album, &o, &r.token, 500, 5)
            .unwrap()
            .is_empty()
    );
    assert!(
        engine
            .window(&album, &o, &r.token, 0, 0)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_window_holds_at_most_500_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let names: Vec<String> = (0..520).map(|i| format!("p{i:04}.jpg")).collect();
    let photos: Vec<P<'_>> = names.iter().map(|n| p(n, None)).collect();
    add(&album, &photos);
    let engine = QueryEngine::new();
    let r = run(&engine, &album, &default_query());
    let rows = engine
        .window(&album, &Overlay::default(), &r.token, 0, 10_000)
        .unwrap();
    assert_eq!(WINDOW_MAX, 500);
    assert_eq!(rows.len(), 500);
}

#[test]
fn a_window_shows_the_rows_as_they_are_now() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let ids = small(&album);
    let engine = QueryEngine::new();
    let r = run(&engine, &album, &default_query());
    change(
        &album,
        &format!("UPDATE photos SET star = 1 WHERE id = {}", ids[0]),
        &[ids[0]],
    );
    let rows = engine
        .window(&album, &Overlay::default(), &r.token, 1, 1)
        .unwrap();
    assert_eq!(rows[0].id, ids[0]);
    assert!(rows[0].star);
}

#[test]
fn a_row_removed_after_the_run_keeps_its_place() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let ids = small(&album);
    let engine = QueryEngine::new();
    let r = run(&engine, &album, &default_query());
    change(
        &album,
        &format!("DELETE FROM photos WHERE id = {}", ids[0]),
        &[ids[0]],
    );
    let rows = engine
        .window(&album, &Overlay::default(), &r.token, 0, 10)
        .unwrap();
    assert_eq!(rows.len(), 4, "the window keeps its length");
    assert_eq!(rows[1].id, ids[0]);
    assert_eq!(rows[1].thumb_key, "");
    assert_eq!(rows[2].id, ids[1]);
}

#[test]
fn a_view_only_album_shows_rows_held_only_in_memory() {
    let tmp = tempfile::tempdir().unwrap();
    let fixture = owlbum_fixtures::trip(&tmp.path().join("trip")).unwrap();
    let album = Album::open(
        &fixture.root,
        &OpenOptions {
            view_only: true,
            ..OpenOptions::default()
        },
    )
    .unwrap();
    assert!(album.is_view_only());
    let scanner = Scanner::new(Arc::new(album), ScanEnv::new(Arc::new(Quiet)));
    scanner.scan_and_wait(Request::default());
    let album = scanner.album();
    let engine = QueryEngine::new();
    let r = run(&engine, album, &default_query());
    assert!(r.count > 0);
    let overlay = scanner.overlay();
    assert!(!overlay.is_empty());
    let rows = engine
        .window(album, &overlay, &r.token, 0, WINDOW_MAX)
        .unwrap();
    assert_eq!(rows.len() as u32, r.count);
    for row in &rows {
        let memory = overlay.get(row.id).expect("every row is held in memory");
        assert_eq!(row.rel_path, memory.rel_path);
        assert_eq!(row.thumb_key, memory.facts.thumb_key);
        assert!(!row.thumb_key.is_empty());
        assert!(row.rel_path.ends_with(&row.file_name));
    }
}

// ---- query_locate ----

#[test]
fn locate_finds_every_tile_and_nothing_else() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let ids = small(&album);
    let engine = QueryEngine::new();
    let r = run(&engine, &album, &default_query());
    let tiles = tile_ids(&engine, &album, &r.token);
    for (i, id) in tiles.iter().enumerate() {
        assert_eq!(
            engine.locate(&r.token, *id).unwrap(),
            Some(Located::At {
                index: u32::try_from(i).unwrap()
            })
        );
    }
    assert_eq!(engine.locate(&r.token, ids[3]).unwrap(), None, "hidden");
    assert_eq!(engine.locate(&r.token, 424_242).unwrap(), None);
    assert_eq!(engine.locate(&r.token, -1).unwrap(), None);
    assert_eq!(engine.locate(&r.token, i64::MAX).unwrap(), None);
}

// ---- keep: the pure rules ----

fn keep(current: Option<(i64, u32)>, anchor: Option<i64>, selection: &[i64]) -> Keep {
    Keep {
        token: "old".to_owned(),
        current: current.map(|(id, index)| PhotoAt { id, index }),
        anchor_id: anchor,
        selection: selection.to_vec(),
    }
}

fn at(id: i64, index: u32) -> Option<PhotoAt> {
    Some(PhotoAt { id, index })
}

#[test]
fn rule_1_the_current_photo_stays_with_its_new_index() {
    let old = [1, 2, 3, 4, 5];
    let new = [9, 1, 3, 5, 2];
    let kept = settle(&keep(Some((3, 2)), Some(5), &[3, 4, 5]), Some(&old), &new);
    assert_eq!(kept.current, at(3, 2));
    assert_eq!(kept.anchor, at(5, 3));
    assert_eq!(kept.selection, [3, 5], "cut down to photos still shown");
}

#[test]
fn rule_3_the_photo_now_in_its_place() {
    // 3 leaves; of the photos before it (1, 2), only 2 is still there: k = 1.
    let old = [1, 2, 3, 4, 5];
    let new = [2, 4, 5];
    let kept = settle(&keep(Some((3, 2)), None, &[3]), Some(&old), &new);
    assert_eq!(kept.current, at(4, 1));
    assert_eq!(kept.anchor, at(4, 1), "no anchor: the current photo");
    assert_eq!(kept.selection, [4]);
}

#[test]
fn rule_3_keeps_the_rest_of_the_selection_that_is_still_shown() {
    let old = [1, 2, 3, 4, 5];
    let new = [1, 2, 4, 5];
    let kept = settle(&keep(Some((3, 2)), Some(3), &[1, 3, 5]), Some(&old), &new);
    assert_eq!(kept.current, at(4, 2));
    assert_eq!(
        kept.anchor,
        at(4, 2),
        "the anchor is gone: the current photo"
    );
    assert_eq!(kept.selection, [1, 5, 4]);
}

#[test]
fn rule_4_past_the_end_takes_the_last_tile() {
    let old = [1, 2, 3, 4, 5];
    let new = [1, 2, 3, 4];
    let kept = settle(&keep(Some((5, 4)), None, &[5]), Some(&old), &new);
    assert_eq!(kept.current, at(4, 3));
    assert_eq!(kept.selection, [4]);
}

#[test]
fn an_empty_result_selects_nothing() {
    let kept = settle(&keep(Some((5, 4)), Some(1), &[1, 5]), Some(&[1, 5]), &[]);
    assert_eq!(kept, Kept::default());
}

#[test]
fn a_dropped_token_falls_back_to_the_old_index() {
    let new = [10, 11, 12, 13];
    let kept = settle(&keep(Some((3, 2)), None, &[3]), None, &new);
    assert_eq!(kept.current, at(12, 2));
    let past = settle(&keep(Some((3, 9)), None, &[3]), None, &new);
    assert_eq!(past.current, at(13, 3));
}

#[test]
fn no_current_photo_stays_none() {
    let kept = settle(&keep(None, Some(2), &[1, 2]), Some(&[1, 2]), &[2, 3]);
    assert_eq!(kept.current, None);
    assert_eq!(kept.anchor, at(2, 0));
    assert_eq!(kept.selection, [2]);
    let none = settle(&keep(None, Some(1), &[]), Some(&[1, 2]), &[2, 3]);
    assert_eq!(none.anchor, None);
}

#[test]
fn ids_that_cannot_be_photos_are_not_found() {
    let kept = settle(&keep(Some((-1, 0)), Some(i64::MAX), &[-5]), None, &[7]);
    assert_eq!(kept.current, at(7, 0));
    assert_eq!(kept.anchor, at(7, 0));
    assert_eq!(kept.selection, [7]);
}

#[test]
fn a_selection_is_not_repeated() {
    let kept = settle(
        &keep(Some((2, 1)), None, &[2, 1, 2, 1]),
        Some(&[1, 2]),
        &[1, 2],
    );
    assert_eq!(kept.selection, [2, 1]);
}

// ---- keep through query_run ----

#[test]
fn hiding_the_current_photo_moves_to_the_one_in_its_place() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let ids = small(&album); // shown by date: e(4), b(0), a(1), c(2)
    let engine = QueryEngine::new();
    let first = run(&engine, &album, &default_query());
    change(
        &album,
        &format!("UPDATE photos SET hidden = 1 WHERE id = {}", ids[0]),
        &[ids[0]],
    );
    let k = Keep {
        token: first.token,
        current: Some(PhotoAt {
            id: ids[0],
            index: 1,
        }),
        anchor_id: Some(ids[0]),
        selection: vec![ids[0]],
    };
    let r = engine.run(&album, &default_query(), Some(&k)).unwrap();
    let kept = r.kept.unwrap();
    assert_eq!(kept.current, at(ids[1], 1));
    assert_eq!(kept.anchor, at(ids[1], 1));
    assert_eq!(kept.selection, [ids[1]]);
    assert_eq!(r.total, 3);
}

#[test]
fn keep_reads_the_old_result_before_a_new_token_drops_it() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let ids = small(&album); // shown: e, b, a, c
    let engine = QueryEngine::new();
    let oldest = run(&engine, &album, &default_query());
    for _ in 1..TOKENS_KEPT {
        run(&engine, &album, &default_query());
    }
    change(
        &album,
        &format!(
            "UPDATE photos SET hidden = 1 WHERE id IN ({}, {})",
            ids[4], ids[0]
        ),
        &[ids[4], ids[0]],
    );
    // b (index 1) and e before it both leave: with the old list k = 0, so
    // a; with only the old index it would be c.
    let k = Keep {
        token: oldest.token.clone(),
        current: Some(PhotoAt {
            id: ids[0],
            index: 1,
        }),
        anchor_id: None,
        selection: vec![],
    };
    let r = engine.run(&album, &default_query(), Some(&k)).unwrap();
    assert_eq!(r.kept.unwrap().current, at(ids[1], 0));
    assert_eq!(
        code(engine.locate(&oldest.token, ids[1])),
        ErrorCode::QueryExpired
    );
    let again = engine.run(&album, &default_query(), Some(&k)).unwrap();
    assert_eq!(
        again.kept.unwrap().current,
        at(ids[2], 1),
        "once dropped, the old index is used"
    );
}

// ---- Arguments and shapes ----

#[test]
fn an_unreadable_query_answers_bad_query() {
    let bad: Arg<Query> = serde_json::from_value(json!({
        "view": "grid", "timeline_unit": "auto", "sort": "size", "stack": true,
        "open_stacks": [], "filter": {}, "search": null, "want_facets": false
    }))
    .unwrap();
    let err = bad.get().unwrap_err();
    assert_eq!(err.code, ErrorCode::BadQuery);
    assert!(
        err.details.unwrap()["cause"]
            .as_str()
            .unwrap()
            .contains("size")
    );
    let not_even_an_object: Arg<Query> = serde_json::from_value(json!(7)).unwrap();
    assert_eq!(code(not_even_an_object.get()), ErrorCode::BadQuery);
}

#[test]
fn the_page_query_reads_as_the_doc_writes_it() {
    let good: Arg<Query> = serde_json::from_value(json!({
        "view": "map", "timeline_unit": "month", "sort": "name", "stack": false,
        "open_stacks": [4], "filter": { "star": "yes", "has_people": null, "tags": [{ "kind": "object", "value": "boat" }], "unknown": 1 },
        "search": "lake", "want_facets": true
    }))
    .unwrap();
    let q = good.get().unwrap();
    assert_eq!(q.view, QueryView::Map);
    assert_eq!(q.timeline_unit, QueryTimelineUnit::Month);
    assert_eq!(q.sort, QuerySort::Name);
    assert_eq!(q.open_stacks, [4]);
    assert_eq!(q.filter.star, Some(StarFilter::Yes));
    assert_eq!(q.filter.has_people, None);
    assert_eq!(q.filter.tags.as_ref().unwrap()[0].kind, TagKind::Object);
    assert_eq!(q.search.as_deref(), Some("lake"));
    assert!(q.want_facets);
}

#[test]
fn a_filter_writes_only_what_is_on() {
    let f = Filter {
        star: Some(StarFilter::No),
        ..Filter::default()
    };
    assert_eq!(serde_json::to_value(&f).unwrap(), json!({ "star": "no" }));
}

#[test]
fn located_serialises_as_the_doc_says() {
    assert_eq!(
        serde_json::to_value(Located::At { index: 3 }).unwrap(),
        json!({ "index": 3 })
    );
    assert_eq!(
        serde_json::to_value(Located::Folded { folded_in: 8 }).unwrap(),
        json!({ "folded_in": 8 })
    );
}

#[test]
fn a_result_leaves_out_the_parts_it_does_not_carry() {
    let tmp = tempfile::tempdir().unwrap();
    let album = album(tmp.path());
    let engine = QueryEngine::new();
    let r = serde_json::to_value(run(&engine, &album, &default_query())).unwrap();
    let mut keys: Vec<&str> = r.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "album_photos",
            "chips",
            "count",
            "filtered",
            "kept",
            "panel_count",
            "photos",
            "revision",
            "token",
            "total"
        ]
    );
    assert_eq!(r["kept"], serde_json::Value::Null);
}
