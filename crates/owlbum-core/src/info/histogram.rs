//! One photo's histogram: `photo_histogram` (docs/04-browsing.md,
//! "Histogram").
//!
//! It is counted on demand, when the panel's Histogram section is open,
//! from the photo's medium thumbnail reduced so its long side is at most
//! 256 px. Nothing is stored: a folded section costs nothing, and the scan
//! and the album database are unchanged. The histogram describes the file,
//! not its edit, so it agrees with the clipping line from the scan's
//! analysis of the file.

use image::RgbImage;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, AppResult};
use crate::text;
use crate::thumbs::{Kind, Photo, Thumbs, make};

/// Counts per channel.
pub const BINS: usize = 64;

/// The picture counted fits inside this many pixels a side.
pub const SIDE: u32 = 256;

/// What `photo_histogram` answers: [`BINS`] counts for each of luminance,
/// red, green and blue, from dark to light.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Histogram {
    pub l: Vec<u32>,
    pub r: Vec<u32>,
    pub g: Vec<u32>,
    pub b: Vec<u32>,
}

/// The bin of an 8-bit level.
fn bin(level: u8) -> usize {
    usize::from(level) * BINS / 256
}

/// The histogram of `img`, reduced first so it fits inside [`SIDE`].
/// Luminance is `0.2126 R + 0.7152 G + 0.0722 B`.
pub fn count(img: &RgbImage) -> Histogram {
    let (w, h) = make::fitted(img.width(), img.height(), SIDE);
    let reduced;
    let img = if (w, h) == img.dimensions() {
        img
    } else {
        reduced = make::lanczos(img, w, h);
        &reduced
    };
    let mut hist = Histogram {
        l: vec![0; BINS],
        r: vec![0; BINS],
        g: vec![0; BINS],
        b: vec![0; BINS],
    };
    for p in img.pixels() {
        let [r, g, b] = p.0;
        let luma = 0.2126 * f64::from(r) + 0.7152 * f64::from(g) + 0.0722 * f64::from(b);
        // In 0..=255 by construction; the cast cannot truncate.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let luma = luma.round().clamp(0.0, 255.0) as u8;
        hist.l[bin(luma)] += 1;
        hist.r[bin(r)] += 1;
        hist.g[bin(g)] += 1;
        hist.b[bin(b)] += 1;
    }
    hist
}

/// The histogram of encoded picture bytes (a thumbnail JPEG); None when
/// they do not decode.
pub fn of_bytes(bytes: &[u8]) -> Option<Histogram> {
    let img = image::load_from_memory(bytes).ok()?;
    Some(count(&img.to_rgb8()))
}

/// The histogram of `photo`'s medium thumbnail, made first when it is not
/// in the cache. `internal` when no thumbnail can be made.
pub fn of_photo(thumbs: &Thumbs, photo: &Photo) -> AppResult<Histogram> {
    thumbs
        .thumbnail(photo, Kind::Medium)
        .as_deref()
        .and_then(of_bytes)
        .ok_or_else(|| AppError::internal(text::photo::HISTOGRAM))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    fn flat(w: u32, h: u32, c: [u8; 3]) -> RgbImage {
        RgbImage::from_pixel(w, h, Rgb(c))
    }

    fn sum(v: &[u32]) -> u32 {
        v.iter().sum()
    }

    #[test]
    fn pure_red_puts_every_red_count_in_the_last_bin() {
        let h = count(&flat(40, 30, [255, 0, 0]));
        let n = 40 * 30;
        assert_eq!(h.r[BINS - 1], n);
        assert_eq!(h.g[0], n);
        assert_eq!(h.b[0], n);
        // 0.2126 × 255 = 54.2: level 54, bin 13.
        assert_eq!(h.l[13], n);
        for v in [&h.l, &h.r, &h.g, &h.b] {
            assert_eq!(v.len(), BINS);
            assert_eq!(sum(v), n);
        }
    }

    #[test]
    fn levels_fall_in_bins_of_four() {
        assert_eq!(bin(0), 0);
        assert_eq!(bin(3), 0);
        assert_eq!(bin(4), 1);
        assert_eq!(bin(128), 32);
        assert_eq!(bin(255), 63);
        let h = count(&flat(2, 2, [255, 255, 255]));
        assert_eq!(h.l[63], 4);
    }

    #[test]
    fn a_large_picture_is_counted_at_256_on_its_long_side() {
        let h = count(&flat(1600, 800, [10, 200, 90]));
        assert_eq!(sum(&h.r), 256 * 128);
        assert_eq!(h.g[bin(200)], 256 * 128);
    }

    #[test]
    fn encoded_bytes_are_decoded_first() {
        let bytes = make::encode(&flat(64, 48, [255, 0, 0]), 95);
        let h = of_bytes(&bytes).unwrap();
        assert_eq!(sum(&h.r), 64 * 48);
        // JPEG may move a level or two, never out of the brightest bins.
        assert!(h.r[BINS - 1] + h.r[BINS - 2] == 64 * 48);
        assert_eq!(of_bytes(b"not a picture"), None);
    }
}
