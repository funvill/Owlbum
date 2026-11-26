//! Image analysis (docs/03-media-and-scanning.md, "Image analysis"): the
//! quality measures, perceptual hash and colours stored as a photo's
//! `analysis` JSON. The photo score, bursts and colour sorting (docs/05)
//! read them, so every step is pinned: change one and raise
//! [`super::SCAN_VERSION`].
//!
//! The input is the small thumbnail's pixels before encoding (upright,
//! sRGB, flattened on white, at most 512 px) and, when the source has
//! alpha, its alpha plane at the same size ([`super::pixels::Buffers`]).

use image::imageops::{self, FilterType};
use image::{GrayImage, RgbImage};
use serde::Serialize;

/// The analysis image fits inside this many pixels a side.
pub const SIDE: u32 = 256;
/// Grey levels at or below this are clipped shadows.
pub const SHADOWS: u8 = 8;
/// Grey levels at or above this are clipped highlights.
pub const HIGHLIGHTS: u8 = 247;
/// The saliency weight's spread, in fractions of the picture.
pub const SALIENCY_SIGMA: f64 = 0.3;
/// Alpha below this is a transparent pixel.
pub const ALPHA_OPAQUE: u8 = 250;

/// The `analysis` JSON, keys in the order docs/03 lists them.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Analysis {
    pub sharpness: f64,
    pub mean_luma: f64,
    pub clipped_shadows: f64,
    pub clipped_highlights: f64,
    pub saliency_x: f64,
    pub saliency_y: f64,
    /// 16 hex digits.
    pub dhash: String,
    pub avg_colour: [u8; 3],
    /// 3 × 3 cells, row-major, RGB.
    pub colour_grid: Vec<u8>,
    pub transparent: bool,
}

impl Analysis {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_owned())
    }
}

/// Analyses the small thumbnail's pixels (`small`) and, when the source has
/// alpha, its alpha plane (`alpha`, the same size).
pub fn analyse(small: &RgbImage, alpha: Option<&GrayImage>) -> Analysis {
    let _ = (small, alpha, imageops::flip_horizontal::<RgbImage>, FilterType::Lanczos3);
    Analysis {
        sharpness: 0.0,
        mean_luma: 0.0,
        clipped_shadows: 0.0,
        clipped_highlights: 0.0,
        saliency_x: 0.0,
        saliency_y: 0.0,
        dhash: String::new(),
        avg_colour: [0; 3],
        colour_grid: Vec::new(),
        transparent: false,
    }
}

#[cfg(test)]
mod tests {
    use image::{Luma, Rgb};

    use super::*;

    fn flat(w: u32, h: u32, v: u8) -> RgbImage {
        RgbImage::from_pixel(w, h, Rgb([v, v, v]))
    }

    fn grey_image(w: u32, h: u32, f: impl Fn(u32, u32) -> u8) -> RgbImage {
        RgbImage::from_fn(w, h, |x, y| {
            let v = f(x, y);
            Rgb([v, v, v])
        })
    }

    #[test]
    fn flat_grey() {
        let a = analyse(&flat(64, 48, 128), None);
        assert_eq!(a.sharpness, 0.0);
        assert_eq!((a.saliency_x, a.saliency_y), (0.5, 0.5));
        assert_eq!(a.mean_luma, 0.502);
        assert_eq!((a.clipped_shadows, a.clipped_highlights), (0.0, 0.0));
        assert_eq!(a.dhash, "0000000000000000");
        assert_eq!(a.avg_colour, [128, 128, 128]);
        assert_eq!(a.colour_grid, vec![128; 27]);
        assert!(!a.transparent);
    }

    #[test]
    fn black_and_white_clip() {
        let a = analyse(&flat(40, 30, 0), None);
        assert_eq!((a.mean_luma, a.clipped_shadows, a.clipped_highlights), (0.0, 1.0, 0.0));
        let a = analyse(&flat(40, 30, 255), None);
        assert_eq!((a.mean_luma, a.clipped_shadows, a.clipped_highlights), (1.0, 0.0, 1.0));
        // The thresholds are inclusive.
        assert_eq!(analyse(&flat(4, 4, 8), None).clipped_shadows, 1.0);
        assert_eq!(analyse(&flat(4, 4, 9), None).clipped_shadows, 0.0);
        assert_eq!(analyse(&flat(4, 4, 247), None).clipped_highlights, 1.0);
        assert_eq!(analyse(&flat(4, 4, 246), None).clipped_highlights, 0.0);
        // A quarter black: 0.25.
        let a = analyse(&grey_image(8, 8, |x, y| if x < 4 && y < 4 { 0 } else { 128 }), None);
        assert_eq!(a.clipped_shadows, 0.25);
    }

    #[test]
    fn grey_is_the_rounded_luma_of_srgb_values() {
        // 0.299·200 + 0.587·100 + 0.114·50 = 124.2 → 124.
        let img = RgbImage::from_pixel(2, 2, Rgb([200, 100, 50]));
        assert_eq!(analyse(&img, None).mean_luma, 0.486); // 124 / 255
        // 0.299·255 = 76.245 → 76; 0.587·255 = 149.685 → 150.
        let red = RgbImage::from_pixel(2, 2, Rgb([255, 0, 0]));
        assert_eq!(analyse(&red, None).mean_luma, 0.298);
        let green = RgbImage::from_pixel(2, 2, Rgb([0, 255, 0]));
        assert_eq!(analyse(&green, None).mean_luma, 0.588);
    }

    #[test]
    fn sharpness_is_the_laplacian_variance() {
        // 4 × 3: interior pixels (1,1) and (2,1). p(1,1) = 10, the rest 0:
        // v = 40 and −10; mean 15, mean of squares 850, variance 625.
        let img = grey_image(4, 3, |x, y| if (x, y) == (1, 1) { 10 } else { 0 });
        assert_eq!(analyse(&img, None).sharpness, 625.0);
        // No interior pixels: 0.
        assert_eq!(analyse(&grey_image(2, 5, |x, _| x as u8 * 200), None).sharpness, 0.0);
        assert_eq!(analyse(&grey_image(1, 1, |_, _| 7), None).sharpness, 0.0);
        // A sharp edge measures higher than the same edge blurred.
        let sharp = grey_image(64, 64, |x, _| if x < 32 { 0 } else { 255 });
        let soft = grey_image(64, 64, |x, _| (x * 4).min(255) as u8);
        assert!(analyse(&sharp, None).sharpness > analyse(&soft, None).sharpness);
    }

    #[test]
    fn an_off_centre_bright_spot_pulls_the_saliency_point() {
        let img = grey_image(64, 64, |x, y| if (x, y) == (48, 16) { 255 } else { 0 });
        let a = analyse(&img, None);
        assert!((a.saliency_x - 0.75).abs() < 0.005, "{}", a.saliency_x);
        assert!((a.saliency_y - 0.25).abs() < 0.005, "{}", a.saliency_y);
        // Mirrored, it moves the other way.
        let img = grey_image(64, 64, |x, y| if (x, y) == (16, 48) { 255 } else { 0 });
        let b = analyse(&img, None);
        assert!(b.saliency_x < 0.3 && b.saliency_y > 0.7, "{b:?}");
    }

    #[test]
    fn dhash_of_ramps_and_bit_order() {
        // Brighter to the right: every bit is 1.
        let right = grey_image(90, 80, |x, _| (x * 2) as u8);
        assert_eq!(analyse(&right, None).dhash, "ffffffffffffffff");
        // Darker to the right, or only changing downwards: every bit 0.
        let left = grey_image(90, 80, |x, _| 200 - (x * 2) as u8);
        assert_eq!(analyse(&left, None).dhash, "0000000000000000");
        let down = grey_image(90, 80, |_, y| (y * 3) as u8);
        assert_eq!(analyse(&down, None).dhash, "0000000000000000");
        // Rows top to bottom, most significant bit first: only the top row
        // rises.
        let top = grey_image(9, 8, |x, y| if y == 0 { (x * 20) as u8 } else { 50 });
        assert_eq!(analyse(&top, None).dhash, "ff00000000000000");
        // Only the last pair of the bottom row rises.
        let last = grey_image(9, 8, |x, y| if (x, y) == (8, 7) { 99 } else { 50 });
        assert_eq!(analyse(&last, None).dhash, "0000000000000001");
        // In the first pair of the first row: the top bit.
        let first = grey_image(9, 8, |x, y| if (x, y) == (1, 0) { 99 } else { 50 });
        assert_eq!(analyse(&first, None).dhash, "8000000000000000");
    }

    #[test]
    fn the_box_filter_averages_areas() {
        // 18 × 16 to 9 × 8: each cell is the mean of a 2 × 2 block, and the
        // blocks rise to the right.
        let img = grey_image(18, 16, |x, _| (x / 2 * 10 + x % 2) as u8);
        assert_eq!(analyse(&img, None).dhash, "ffffffffffffffff");
    }

    #[test]
    fn average_colour_and_colour_grid() {
        // A 3 × 3 picture is its own grid.
        let cells: Vec<[u8; 3]> = (0..9u8).map(|i| [i * 20, 255 - i * 20, i * 7]).collect();
        let img = RgbImage::from_fn(3, 3, |x, y| Rgb(cells[(y * 3 + x) as usize]));
        let a = analyse(&img, None);
        assert_eq!(a.colour_grid, cells.concat());
        assert_eq!(a.avg_colour, [80, 175, 28]);
        // 6 × 6: each cell averages a 2 × 2 block; 0 and 255 give 127.5,
        // which rounds to 128.
        let img = RgbImage::from_fn(6, 6, |x, _| Rgb(if x % 2 == 0 { [0, 0, 0] } else { [255, 255, 255] }));
        let a = analyse(&img, None);
        assert_eq!(a.colour_grid, vec![128; 27]);
        assert_eq!(a.avg_colour, [128, 128, 128]);
        // 4 × 2 to 3 × 3: areas split across source pixels.
        let img = RgbImage::from_fn(4, 2, |x, _| Rgb([if x < 2 { 0 } else { 240 }, 0, 0]));
        let a = analyse(&img, None);
        let reds: Vec<u8> = a.colour_grid.chunks(3).map(|c| c[0]).collect();
        // Column 0 covers x 0..1.33 (all 0); column 1 covers 1.33..2.67
        // (half 0, half 240); column 2 covers 2.67..4 (all 240).
        assert_eq!(reds, vec![0, 120, 240, 0, 120, 240, 0, 120, 240]);
    }

    fn alpha_with(transparent: usize) -> GrayImage {
        let mut n = 0;
        GrayImage::from_fn(100, 100, |_, _| {
            n += 1;
            Luma([if n <= transparent { 0 } else { 255 }])
        })
    }

    #[test]
    fn transparency_counts_pixels_with_alpha_below_250() {
        let img = flat(100, 100, 255);
        assert!(analyse(&img, Some(&alpha_with(100))).transparent, "1%");
        assert!(!analyse(&img, Some(&alpha_with(10))).transparent, "0.1%");
        // More than w·h / 200: exactly 0.5% is not.
        assert!(!analyse(&img, Some(&alpha_with(50))).transparent);
        assert!(analyse(&img, Some(&alpha_with(51))).transparent);
        // 249 is transparent, 250 is not.
        let a249 = GrayImage::from_pixel(10, 10, Luma([249]));
        let a250 = GrayImage::from_pixel(10, 10, Luma([250]));
        assert!(analyse(&flat(10, 10, 1), Some(&a249)).transparent);
        assert!(!analyse(&flat(10, 10, 1), Some(&a250)).transparent);
        assert!(!analyse(&img, None).transparent, "no alpha channel");
    }

    #[test]
    fn the_analysis_image_fits_inside_256() {
        // A 512 × 256 small thumbnail becomes 256 × 128. A half-black,
        // half-white split stays a half split.
        let img = grey_image(512, 256, |x, _| if x < 256 { 0 } else { 255 });
        let a = analyse(&img, None);
        assert!((a.mean_luma - 0.5).abs() < 0.01, "{}", a.mean_luma);
        assert_eq!(a.dhash.len(), 16);
        // A one-pixel-high strip keeps at least one pixel.
        let strip = grey_image(512, 1, |x, _| (x / 2) as u8);
        let a = analyse(&strip, None);
        assert_eq!(a.sharpness, 0.0);
        assert_eq!(a.dhash, "ffffffffffffffff");
    }

    #[test]
    fn the_json_has_every_key_in_order_and_is_stable() {
        let img = grey_image(50, 40, |x, y| ((x * 5) ^ (y * 3)) as u8);
        let a = analyse(&img, None).to_json();
        assert_eq!(a, analyse(&img, None).to_json());
        let v: serde_json::Value = serde_json::from_str(&a).unwrap();
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        let mut sorted = vec![
            "avg_colour",
            "clipped_highlights",
            "clipped_shadows",
            "colour_grid",
            "dhash",
            "mean_luma",
            "saliency_x",
            "saliency_y",
            "sharpness",
            "transparent",
        ];
        sorted.sort_unstable();
        assert_eq!(keys, sorted);
        assert!(a.starts_with(r#"{"sharpness":"#), "{a}");
        assert!(a.ends_with(r#""transparent":false}"#), "{a}");
        assert_eq!(v["colour_grid"].as_array().unwrap().len(), 27);
        // Rounded: 3 decimals for shares, 2 for sharpness.
        for k in ["mean_luma", "clipped_shadows", "saliency_x"] {
            let x = v[k].as_f64().unwrap();
            assert_eq!((x * 1000.0).round() / 1000.0, x, "{k}");
        }
        let s = v["sharpness"].as_f64().unwrap();
        assert_eq!((s * 100.0).round() / 100.0, s);
    }
}
