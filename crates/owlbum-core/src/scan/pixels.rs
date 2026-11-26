//! What the pixels pass makes from its one decode
//! (docs/03-media-and-scanning.md, "Pixels pass"): the thumbnail buffers
//! ([`crate::thumbs::make`]), built once and shared by the thumbnail writer
//! and the analysis, and the analysis from the small one.
//!
//! The 1600 px buffer is always built, so the analysis never depends on
//! which thumbnails the cache keeps.

use image::GrayImage;

use super::analysis;
use crate::media::Pixels;
use crate::thumbs::make::{self, Buffers};

/// The source's alpha plane at the small buffer's size, made by the same
/// resizes as the colours (1600, then 512); None when it has no alpha.
pub fn small_alpha(px: &Pixels) -> Option<GrayImage> {
    make::alpha(px).map(|a| make::fit(make::fit(a, make::MEDIUM_SIDE), make::SMALL_SIDE))
}

/// The `analysis` JSON from the pass's buffers and the small alpha plane.
pub fn analysis_of(b: &Buffers, alpha: Option<&GrayImage>) -> String {
    analysis::analyse(&b.small, alpha).to_json()
}

/// The `analysis` JSON of a picture.
pub fn analysis(px: &Pixels) -> String {
    analysis_of(&make::buffers(px), small_alpha(px).as_ref())
}

/// The upright size to store after the decode, when it should change the
/// row: the header gave none, or the decode reports a different full size.
/// The size of a reduced or embedded preview (`full_*` 0) is never stored.
pub fn corrected_size(header: (u32, u32), pixels: &Pixels) -> Option<(u32, u32)> {
    let full = (pixels.full_width, pixels.full_height);
    if full.0 == 0 || full.1 == 0 || full == header {
        None
    } else {
        Some(full)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(full: (u32, u32)) -> Pixels {
        Pixels {
            width: 4,
            height: 2,
            full_width: full.0,
            full_height: full.1,
            rgba: vec![0; 32],
            has_alpha: false,
        }
    }

    /// A picture of `w × h` whose pixels come from `f`.
    fn picture(w: u32, h: u32, has_alpha: bool, f: impl Fn(u32, u32) -> [u8; 4]) -> Pixels {
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                rgba.extend_from_slice(&f(x, y));
            }
        }
        Pixels {
            width: w,
            height: h,
            full_width: w,
            full_height: h,
            rgba,
            has_alpha,
        }
    }

    #[test]
    fn the_decode_corrects_a_missing_or_wrong_header_size() {
        assert_eq!(corrected_size((0, 0), &px((40, 20))), Some((40, 20)));
        assert_eq!(corrected_size((30, 20), &px((40, 20))), Some((40, 20)));
        assert_eq!(corrected_size((40, 20), &px((40, 20))), None);
        // A reduced preview that cannot tell the full size changes nothing.
        assert_eq!(corrected_size((0, 0), &px((0, 0))), None);
    }

    #[test]
    fn fit_keeps_the_aspect_and_never_enlarges() {
        use make::fitted;
        assert_eq!(fitted(4000, 3000, 1600), (1600, 1200));
        assert_eq!(fitted(1600, 1200, 512), (512, 384));
        assert_eq!(fitted(100, 50, 1600), (100, 50));
        assert_eq!(fitted(3000, 1, 256), (256, 1));
        assert_eq!(fitted(1000, 333, 256), (256, 85)); // 85.248
        assert_eq!(fitted(1000, 335, 256), (256, 86)); // 85.76
    }

    #[test]
    fn the_alpha_plane_follows_the_colours() {
        let p = picture(4000, 2000, true, |_, _| [9, 9, 9, 100]);
        let b = make::buffers(&p);
        assert_eq!(b.small.dimensions(), (512, 256));
        let a = small_alpha(&p).unwrap();
        assert_eq!(a.dimensions(), (512, 256));
        assert_eq!(a.get_pixel(10, 10).0, [100]);
        assert!(small_alpha(&picture(4, 4, false, |_, _| [1, 2, 3, 0])).is_none());
    }

    #[test]
    fn a_transparent_source_analyses_as_transparent() {
        // 100 × 100 with 1% transparent pixels, then 0.1%.
        let one = picture(100, 100, true, |_, y| {
            [50, 60, 70, if y == 0 { 0 } else { 255 }]
        });
        let v: serde_json::Value = serde_json::from_str(&analysis(&one)).unwrap();
        assert_eq!(v["transparent"], true);
        let tenth = picture(100, 100, true, |x, y| {
            [50, 60, 70, if y == 0 && x < 10 { 0 } else { 255 }]
        });
        let v: serde_json::Value = serde_json::from_str(&analysis(&tenth)).unwrap();
        assert_eq!(v["transparent"], false);
        // Resized from a larger picture, a transparent band stays.
        let big = picture(2000, 1000, true, |_, y| {
            [0, 0, 0, if y < 100 { 0 } else { 255 }]
        });
        assert_eq!(small_alpha(&big).unwrap().dimensions(), (512, 256));
        let v: serde_json::Value = serde_json::from_str(&analysis(&big)).unwrap();
        assert_eq!(v["transparent"], true);
    }

    #[test]
    fn the_same_picture_gives_byte_identical_json() {
        let p = picture(900, 700, false, |x, y| {
            [(x % 256) as u8, (y % 256) as u8, ((x ^ y) % 256) as u8, 255]
        });
        let a = analysis(&p);
        assert_eq!(a, analysis(&p));
        let v: serde_json::Value = serde_json::from_str(&a).unwrap();
        assert!(v["sharpness"].as_f64().unwrap() > 0.0);
    }
}

/// How long the buffers and analysis of a 24-megapixel decode take. Run
/// with `cargo test --release -p owlbum-core buffer_timings -- --ignored
/// --nocapture`.
#[cfg(test)]
#[test]
#[ignore = "a timing, not a check"]
fn buffer_timings() {
    let (w, h) = (6000u32, 4000u32);
    let rgba: Vec<u8> = (0..w * h * 4).map(|i| (i % 251) as u8).collect();
    let px = Pixels {
        width: w,
        height: h,
        full_width: w,
        full_height: h,
        rgba,
        has_alpha: false,
    };
    for _ in 0..3 {
        let t = std::time::Instant::now();
        let b = make::buffers(&px);
        let mid = t.elapsed();
        let a = analysis::analyse(&b.small, None);
        println!(
            "buffers {mid:?}, analysis {:?} ({})",
            t.elapsed() - mid,
            a.dhash
        );
    }
}
