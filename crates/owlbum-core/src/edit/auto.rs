//! `edit_auto`: Auto's light and colour from the photo's own statistics
//! (docs/06-editing-and-file-actions.md, "Auto").
//!
//! The statistics are measured on the medium thumbnail: sRGB, the whole
//! upright photo, with no edit applied. The whole photo is used, not the
//! crop, so Auto gives the same answer before and after cropping.

use image::RgbImage;

use super::model::Colour;
use super::plan::{ColourStats, auto_colour};
use crate::error::{AppError, AppResult};
use crate::text;
use crate::thumbs::{Kind, Photo, Thumbs};

/// The mean and standard deviation of R, G and B over every pixel of
/// `img`, in levels from 0 to 255. An empty picture gives all 0.
pub fn stats(img: &RgbImage) -> ColourStats {
    let mut sum = [0f64; 3];
    let mut squares = [0f64; 3];
    for p in img.pixels() {
        for c in 0..3 {
            let v = f64::from(p.0[c]);
            sum[c] += v;
            squares[c] += v * v;
        }
    }
    let n = f64::from(img.width()) * f64::from(img.height());
    if n == 0.0 {
        return ColourStats {
            mean: [0.0; 3],
            stdev: [0.0; 3],
        };
    }
    let mean = sum.map(|s| s / n);
    let mut stdev = [0f64; 3];
    for c in 0..3 {
        stdev[c] = (squares[c] / n - mean[c] * mean[c]).max(0.0).sqrt();
    }
    ColourStats { mean, stdev }
}

/// Auto for `photo`: its medium thumbnail (made first when it is not in
/// the cache) measured and passed to [`auto_colour`]. `internal` when no
/// thumbnail can be made or read.
pub fn of_photo(thumbs: &Thumbs, photo: &Photo) -> AppResult<Colour> {
    let img = thumbs
        .thumbnail(photo, Kind::Medium)
        .as_deref()
        .and_then(|bytes| image::load_from_memory(bytes).ok())
        .ok_or_else(|| AppError::internal(text::edit::AUTO))?;
    Ok(auto_colour(&stats(&img.to_rgb8())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    #[test]
    fn a_flat_picture_has_its_colour_as_mean_and_no_spread() {
        let s = stats(&RgbImage::from_pixel(8, 4, Rgb([10, 118, 250])));
        assert_eq!(s.mean, [10.0, 118.0, 250.0]);
        assert_eq!(s.stdev, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn half_black_half_white_spreads_by_half_the_range() {
        let img = RgbImage::from_fn(4, 2, |x, _| {
            if x < 2 {
                Rgb([0, 0, 0])
            } else {
                Rgb([255, 255, 255])
            }
        });
        let s = stats(&img);
        assert_eq!(s.mean, [127.5; 3]);
        for v in s.stdev {
            assert!((v - 127.5).abs() < 1e-9);
        }
    }

    #[test]
    fn a_dark_flat_thumbnail_brightens_and_adds_contrast() {
        let c = auto_colour(&stats(&RgbImage::from_pixel(16, 16, Rgb([40, 40, 40]))));
        assert!(c.brightness > 0.0 && c.contrast > 0.0);
        assert_eq!(c.saturation, 0.0);
        assert_eq!(c.warmth, 0.0);
    }

    #[test]
    fn an_empty_picture_gives_finite_values() {
        let c = auto_colour(&stats(&RgbImage::new(0, 0)));
        assert!(
            [c.brightness, c.contrast, c.saturation, c.warmth]
                .iter()
                .all(|v| v.is_finite())
        );
    }
}
