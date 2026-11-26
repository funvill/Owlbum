use std::num::NonZeroU32;

use super::*;
use crate::index::PhotoFacts;
use crate::index::record::build;

const S: [f32; 5] = [10.0, 20.0, 30.0, 40.0, 50.0];

fn analysis(sharp: f32, luma: f32, shadows: f32, highlights: f32, sx: f32, sy: f32) -> String {
    format!(
        r#"{{"sharpness":{sharp},"mean_luma":{luma},"clipped_shadows":{shadows},"clipped_highlights":{highlights},"saliency_x":{sx},"saliency_y":{sy}}}"#
    )
}

fn photo(changes: PhotoFacts) -> Photo {
    build(&PhotoFacts { id: 1, ..changes }, 0).unwrap()
}

/// A centred, well-exposed, unclipped photo with no grid, sharpness 30.
fn plain() -> PhotoFacts {
    PhotoFacts {
        analysis: Some(analysis(30.0, 0.48, 0.0, 0.0, 0.5, 0.5)),
        ..sized()
    }
}

/// A 4000 × 3000 image, not analysed.
fn sized() -> PhotoFacts {
    PhotoFacts {
        kind: "image".into(),
        width: 4000,
        height: 3000,
        ..PhotoFacts::default()
    }
}

fn lines(p: &Photo) -> Vec<(String, String, i32)> {
    breakdown(p, &S)
        .lines
        .into_iter()
        .map(|l| (l.label, l.why, l.points))
        .collect()
}

fn line(p: &Photo, label: &str) -> Option<(String, i32)> {
    breakdown(p, &S)
        .lines
        .into_iter()
        .find(|l| l.label == label)
        .map(|l| (l.why, l.points))
}

#[test]
fn the_percentile_counts_values_strictly_below() {
    assert_eq!(percentile(&[], 5.0), 0.5);
    assert_eq!(percentile(&[5.0], 5.0), 0.5);
    assert_eq!(percentile(&[10.0, 20.0], 10.0), 0.0);
    assert_eq!(percentile(&[10.0, 20.0], 20.0), 1.0);
    assert_eq!(percentile(&S, 30.0), 0.5);
    assert_eq!(
        percentile(&[10.0, 10.0, 10.0], 10.0),
        0.0,
        "ties are not below"
    );
}

#[test]
fn a_plain_photo_has_focus_exposure_and_size() {
    let p = photo(plain());
    assert_eq!(
        lines(&p),
        [
            // 35 × 0.5 = 17.5, rounded away from zero.
            ("Focus".into(), "sharper than 50 % of this album".into(), 18),
            ("Exposure".into(), "well exposed".into(), 20),
            ("Size".into(), "12 megapixels".into(), 10),
        ]
    );
    let b = breakdown(&p, &S);
    assert_eq!(b.total, 48);
    assert!(b.analysed);
    assert_eq!(score(&p, &S), Some(48));
}

#[test]
fn exposure_says_dark_or_bright() {
    let dark = photo(PhotoFacts {
        analysis: Some(analysis(30.0, 0.25, 0.0, 0.0, 0.5, 0.5)),
        ..plain()
    });
    // 20 × (1 − 0.23 × 2.2) = 9.88.
    assert_eq!(
        line(&dark, "Exposure"),
        Some(("on the dark side".into(), 10))
    );
    let bright = photo(PhotoFacts {
        analysis: Some(analysis(30.0, 0.7, 0.0, 0.0, 0.5, 0.5)),
        ..plain()
    });
    // 20 × (1 − 0.22 × 2.2) = 10.32.
    assert_eq!(
        line(&bright, "Exposure"),
        Some(("on the bright side".into(), 10))
    );
    let black = photo(PhotoFacts {
        analysis: Some(analysis(30.0, 0.0, 0.0, 0.0, 0.5, 0.5)),
        ..plain()
    });
    assert_eq!(line(&black, "Exposure"), None, "a line worth 0 is left out");
}

#[test]
fn clipping_names_each_share_above_half_a_percent() {
    let p = photo(PhotoFacts {
        analysis: Some(analysis(30.0, 0.48, 0.03, 0.05, 0.5, 0.5)),
        ..plain()
    });
    // −15 × (0.08 − 0.02) / 0.1 = −9.
    assert_eq!(
        line(&p, "Clipping"),
        Some(("5 % blown to white, 3 % crushed to black".into(), -9))
    );
    let p = photo(PhotoFacts {
        analysis: Some(analysis(30.0, 0.48, 0.004, 0.2, 0.5, 0.5)),
        ..plain()
    });
    assert_eq!(
        line(&p, "Clipping"),
        Some(("20 % blown to white".into(), -15))
    );
    let p = photo(PhotoFacts {
        analysis: Some(analysis(30.0, 0.48, 0.0, 0.02, 0.5, 0.5)),
        ..plain()
    });
    assert_eq!(line(&p, "Clipping"), None, "only above 0.02");
}

#[test]
fn colour_variety_and_composition() {
    // Five black cells and four white ones: far apart, so the full 10.
    let mut grid = vec![0_u8; 15];
    grid.extend([255_u8; 12]);
    let json = analysis(30.0, 0.48, 0.0, 0.0, 0.5 + 1.0 / 6.0, 0.5 + 1.0 / 6.0);
    let json = format!("{},\"colour_grid\":{:?}}}", &json[..json.len() - 1], grid);
    let p = photo(PhotoFacts {
        analysis: Some(json),
        ..plain()
    });
    assert_eq!(
        line(&p, "Colour variety"),
        Some(("many colours across the frame".into(), 10))
    );
    assert_eq!(
        line(&p, "Composition"),
        Some(("subject near a third, not dead centre".into(), 5))
    );
    // One colour everywhere: no variety. Dead centre: no composition.
    let p = photo(plain());
    assert_eq!(line(&p, "Colour variety"), None);
    assert_eq!(line(&p, "Composition"), None);
}

#[test]
fn people_size_star_hidden_and_nudges() {
    let p = photo(PhotoFacts {
        people: 1,
        width: 3000,
        height: 2000,
        star: true,
        score_adjust: 2,
        ..plain()
    });
    assert_eq!(line(&p, "People"), Some(("1 person tagged".into(), 5)));
    assert_eq!(line(&p, "Size"), Some(("6.0 megapixels".into(), 8)));
    assert_eq!(line(&p, "Star"), Some(("starred".into(), 50)));
    assert_eq!(
        line(&p, "Your nudges"),
        Some(("Up pressed 2 times more".into(), 2))
    );
    let p = photo(PhotoFacts {
        people: 3,
        hidden: true,
        score_adjust: -1,
        ..plain()
    });
    assert_eq!(line(&p, "People"), Some(("3 people tagged".into(), 10)));
    assert_eq!(line(&p, "Hidden"), Some(("hidden".into(), -50)));
    assert_eq!(
        line(&p, "Your nudges"),
        Some(("Down pressed 1 time more".into(), -1))
    );
}

#[test]
fn a_burst_member_that_is_not_the_keeper_loses_ten() {
    let mut p = photo(plain());
    p.derived.burst = NonZeroU32::new(1);
    p.derived.keeper = false;
    assert_eq!(
        line(&p, "Burst"),
        Some(("a similar shot in this burst is better".into(), -10))
    );
    p.derived.keeper = true;
    assert_eq!(line(&p, "Burst"), None);
}

#[test]
fn a_video_has_no_size_line() {
    let p = photo(PhotoFacts {
        kind: "video".into(),
        ..plain()
    });
    assert_eq!(p.kind, crate::index::Kind::Video);
    assert_eq!(line(&p, "Size"), None);
    assert!(line(&p, "Focus").is_some(), "scored from its poster frame");
}

#[test]
fn a_photo_not_analysed_has_only_the_plain_lines_and_no_tile_score() {
    for json in [None, Some("{}".to_owned())] {
        let p = photo(PhotoFacts {
            analysis: json,
            star: true,
            ..sized()
        });
        assert_eq!(score(&p, &S), None, "its tile shows –");
        let b = breakdown(&p, &S);
        assert!(!b.analysed);
        assert_eq!(
            b.lines.iter().map(|l| l.label.as_str()).collect::<Vec<_>>(),
            ["Size", "Star"]
        );
        assert_eq!(b.total, 60);
    }
}

#[test]
fn megapixel_wording() {
    assert_eq!(text::score::megapixels(9.94), "9.9 megapixels");
    assert_eq!(text::score::megapixels(10.0), "10 megapixels");
    assert_eq!(text::score::megapixels(24.3), "24 megapixels");
}
