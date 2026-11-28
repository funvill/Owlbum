//! Album facts the sorts read (docs/05-organising.md, "Folder facts"):
//! `quality`, "interesting", the dark and bright flags, and the Main colour
//! bucket with its lightness.
//!
//! These are pure functions of one photo's record and, for `quality`, the
//! sorted sharpness list the photo score uses. The sorts work them out when
//! they run, so they always agree with the index as it is now; nothing
//! here is stored.

use super::score::{percentile, variety};
use crate::index::{Analysis, ColourBucket, Kind, Photo};

/// `min(1, max(0, x))`.
fn clamp(x: f64) -> f64 {
    x.clamp(0.0, 1.0)
}

/// The `dark` quality flag.
pub fn dark(a: &Analysis) -> bool {
    a.mean_luma < 0.16 || a.clipped_shadows > 0.35
}

/// The `bright` quality flag.
pub fn bright(a: &Analysis) -> bool {
    a.mean_luma > 0.86 || a.clipped_highlights > 0.3
}

/// The quality flags of docs/05-organising.md, "Quality flags", as the bits
/// of [`crate::index::record::quality`]. `sharpness` is the sorted list of
/// the photo score's Focus line. A photo that is not analysed has none.
pub fn flags(p: &Photo, sharpness: &[f32]) -> u8 {
    use crate::index::record::quality::{BLURRY, BRIGHT, DARK, LOW_RES};
    let Some(a) = p.analysis() else { return 0 };
    let n = sharpness.len();
    let mut out = 0;
    if n >= 5 {
        let p10 = sharpness[n / 10];
        let median = sharpness[n / 2];
        if a.sharpness <= p10 && a.sharpness < median * 0.5 {
            out |= BLURRY;
        }
    }
    if dark(a) {
        out |= DARK;
    }
    if bright(a) {
        out |= BRIGHT;
    }
    if p.kind != Kind::Video && u64::from(p.width) * u64::from(p.height) < 1_500_000 {
        out |= LOW_RES;
    }
    out
}

/// `quality`, defined only for an analysed photo. `sharpness` is the
/// sorted list of the photo score's Focus line.
pub fn quality(p: &Photo, sharpness: &[f32]) -> Option<f64> {
    let a = p.analysis()?;
    let luma = f64::from(a.mean_luma);
    let exposure = clamp(
        1.0 - (luma - 0.48).abs() * 1.6
            - f64::from(a.clipped_highlights) * 2.0
            - f64::from(a.clipped_shadows) * 1.5,
    );
    // A video's frame size says nothing of its quality: full marks for
    // size and no small-picture discount.
    let (size, discount) = if p.kind == Kind::Video {
        (1.0, 1.0)
    } else {
        let mp = f64::from(p.width) * f64::from(p.height) / 1_000_000.0;
        (clamp(mp / 8.0), if mp < 2.0 { 0.6 } else { 1.0 })
    };
    Some((0.55 * percentile(sharpness, a.sharpness) + 0.3 * exposure + 0.15 * size) * discount)
}

/// The lowest and highest of the `quality` values of the analysed photos;
/// None when there are none.
pub fn quality_range(qualities: impl IntoIterator<Item = f64>) -> Option<(f64, f64)> {
    qualities.into_iter().fold(None, |range, q| match range {
        None => Some((q, q)),
        Some((lo, hi)) => Some((lo.min(q), hi.max(q))),
    })
}

/// "Interesting": 0 for a photo that is not analysed. `range` is
/// [`quality_range`] over the whole album index, not over a filtered list.
pub fn interesting(p: &Photo, sharpness: &[f32], range: Option<(f64, f64)>) -> f64 {
    interesting_of(p, quality(p, sharpness), range)
}

/// [`interesting`] with the photo's `quality` already worked out.
pub fn interesting_of(p: &Photo, quality: Option<f64>, range: Option<(f64, f64)>) -> f64 {
    let (Some(a), Some(q)) = (p.analysis(), quality) else {
        return 0.0;
    };
    let stretched = match range {
        Some((lo, hi)) if hi > lo => (q - lo) / (hi - lo),
        _ => 0.5,
    };
    let exposure_ok = if dark(a) || bright(a) { 0.0 } else { 1.0 };
    // A photo in no burst counts as its own keeper.
    let keeper = if p.derived.burst.is_none() || p.derived.keeper {
        1.0
    } else {
        0.0
    };
    0.45 * stretched + 0.2 * variety(a.colour_grid()) + 0.2 * keeper + 0.15 * exposure_ok
}

/// The Main colour bucket and the lightness (0 to 1) of an average colour.
/// No average colour is `neutral` with lightness 0.
pub fn colour(avg: Option<[u8; 3]>) -> (ColourBucket, f32) {
    let Some([r, g, b]) = avg else {
        return (ColourBucket::Neutral, 0.0);
    };
    let (r, g, b) = (
        f64::from(r) / 255.0,
        f64::from(g) / 255.0,
        f64::from(b) / 255.0,
    );
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let lightness = (max + min) / 2.0;
    let saturation = if delta == 0.0 {
        0.0
    } else {
        delta / (1.0 - (2.0 * lightness - 1.0).abs())
    };
    if saturation < 0.15 {
        return (ColourBucket::Neutral, lightness as f32);
    }
    let hue = if max == r {
        60.0 * ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    let bucket = match hue {
        h if !(15.0..345.0).contains(&h) => ColourBucket::Red,
        h if h < 40.0 => ColourBucket::Orange,
        h if h < 70.0 => ColourBucket::Yellow,
        h if h < 165.0 => ColourBucket::Green,
        h if h < 200.0 => ColourBucket::Cyan,
        h if h < 260.0 => ColourBucket::Blue,
        h if h < 290.0 => ColourBucket::Purple,
        _ => ColourBucket::Pink,
    };
    (bucket, lightness as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::PhotoFacts;
    use crate::index::record::build;

    fn analysis(luma: f64, shadows: f64, highlights: f64, extra: &str) -> String {
        format!(
            r#"{{"sharpness":10,"mean_luma":{luma},"clipped_shadows":{shadows},"clipped_highlights":{highlights},"saliency_x":0.5,"saliency_y":0.5{extra}}}"#
        )
    }

    fn photo(kind: &str, w: i64, h: i64, json: Option<String>) -> Photo {
        build(
            &PhotoFacts {
                id: 1,
                file_name: "a.jpg".into(),
                kind: kind.into(),
                width: w,
                height: h,
                analysis: json,
                ..PhotoFacts::default()
            },
            0,
        )
        .unwrap()
    }

    #[test]
    fn colour_buckets_follow_the_hue_table() {
        let cases: [([u8; 3], ColourBucket); 12] = [
            ([255, 0, 0], ColourBucket::Red),
            ([255, 0, 40], ColourBucket::Red),
            ([255, 100, 0], ColourBucket::Orange),
            ([255, 230, 0], ColourBucket::Yellow),
            ([0, 200, 0], ColourBucket::Green),
            ([0, 200, 200], ColourBucket::Cyan),
            ([0, 0, 255], ColourBucket::Blue),
            ([130, 0, 255], ColourBucket::Purple),
            ([255, 0, 200], ColourBucket::Pink),
            ([128, 128, 128], ColourBucket::Neutral),
            ([120, 110, 115], ColourBucket::Neutral),
            ([0, 0, 0], ColourBucket::Neutral),
        ];
        for (rgb, bucket) in cases {
            assert_eq!(colour(Some(rgb)).0, bucket, "{rgb:?}");
        }
    }

    #[test]
    fn lightness_is_the_hsl_lightness() {
        assert_eq!(colour(Some([255, 255, 255])).1, 1.0);
        assert_eq!(colour(Some([0, 0, 0])).1, 0.0);
        assert!((colour(Some([255, 0, 0])).1 - 0.5).abs() < 1e-6);
        assert_eq!(colour(None), (ColourBucket::Neutral, 0.0));
    }

    #[test]
    fn dark_and_bright_flags() {
        let a = |l, s, h| Analysis::parse(&analysis(l, s, h, "")).unwrap();
        assert!(dark(&a(0.1, 0.0, 0.0)));
        assert!(dark(&a(0.5, 0.4, 0.0)));
        assert!(!dark(&a(0.5, 0.0, 0.0)));
        assert!(bright(&a(0.9, 0.0, 0.0)));
        assert!(bright(&a(0.5, 0.0, 0.31)));
        assert!(!bright(&a(0.5, 0.0, 0.3)));
    }

    #[test]
    fn flags_at_their_thresholds() {
        use crate::index::record::quality::{BLURRY, BRIGHT, DARK, LOW_RES};
        let measured = |s: f64, luma: f64| {
            analysis(luma, 0.0, 0.0, "").replace(r#""sharpness":10"#, &format!(r#""sharpness":{s}"#))
        };
        let big = |s: f64, luma: f64| photo("image", 2000, 1000, Some(measured(s, luma)));
        let list = [10.0, 40.0, 50.0, 60.0, 70.0];
        // p10 = 10, median = 50: blurry needs <= 10 and < 25.
        assert_eq!(flags(&big(10.0, 0.5), &list), BLURRY);
        assert_eq!(flags(&big(10.5, 0.5), &list), 0);
        // Fewer than 5 photos: never blurry.
        assert_eq!(flags(&big(10.0, 0.5), &list[..4]), 0);
        assert_eq!(flags(&big(60.0, 0.15), &list), DARK);
        assert_eq!(flags(&big(60.0, 0.87), &list), BRIGHT);
        // 1,500,000 pixels is not low_res; one row fewer is.
        assert_eq!(flags(&photo("image", 1500, 1000, Some(measured(60.0, 0.5))), &list), 0);
        assert_eq!(flags(&photo("image", 1500, 999, Some(measured(60.0, 0.5))), &list), LOW_RES);
        assert_eq!(flags(&photo("video", 100, 100, Some(measured(60.0, 0.5))), &list), 0);
        assert_eq!(flags(&photo("image", 10, 10, None), &list), 0, "not analysed");
    }

    #[test]
    fn quality_by_the_formula() {
        // One photo: percentile 0.5; exposure 1; 8 MP: size 1.
        let p = photo("image", 4000, 2000, Some(analysis(0.48, 0.0, 0.0, "")));
        let q = quality(&p, &[10.0]).unwrap();
        assert!((q - (0.55 * 0.5 + 0.3 + 0.15)).abs() < 1e-6, "{q}");
        // 1 MP: size 1/8 and the 0.6 discount.
        let p = photo("image", 1000, 1000, Some(analysis(0.48, 0.0, 0.0, "")));
        let q = quality(&p, &[10.0]).unwrap();
        assert!(
            (q - (0.55 * 0.5 + 0.3 + 0.15 / 8.0) * 0.6).abs() < 1e-6,
            "{q}"
        );
        // A video: full size marks, no discount.
        let p = photo("video", 100, 100, Some(analysis(0.48, 0.0, 0.0, "")));
        let q = quality(&p, &[10.0]).unwrap();
        assert!((q - (0.55 * 0.5 + 0.3 + 0.15)).abs() < 1e-6, "{q}");
        assert_eq!(quality(&photo("image", 10, 10, None), &[]), None);
    }

    #[test]
    fn interesting_by_the_formula() {
        let p = photo("image", 4000, 2000, Some(analysis(0.48, 0.0, 0.0, "")));
        // Lowest quality of the range: stretched 0; no grid: variety 0;
        // its own keeper; exposure fine.
        let q = quality(&p, &[10.0]).unwrap();
        let v = interesting(&p, &[10.0], Some((q, q + 1.0)));
        assert!((v - (0.2 + 0.15)).abs() < 1e-6, "{v}");
        // One quality in the album: stretched 0.5.
        let v = interesting(&p, &[10.0], Some((q, q)));
        assert!((v - (0.45 * 0.5 + 0.2 + 0.15)).abs() < 1e-6, "{v}");
        // Dark: no exposure term.
        let p = photo("image", 4000, 2000, Some(analysis(0.1, 0.0, 0.0, "")));
        let v = interesting(&p, &[10.0], Some((q, q)));
        assert!((v - (0.45 * 0.5 + 0.2)).abs() < 1e-6, "{v}");
        assert_eq!(interesting(&photo("image", 10, 10, None), &[], None), 0.0);
    }
}
