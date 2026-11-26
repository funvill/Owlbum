//! What the pixels pass makes from its one decode
//! (docs/03-media-and-scanning.md, "Pixels pass" and "Thumbnails and the
//! zoom copy"): the thumbnail buffers, and the analysis from the small one.
//!
//! The 1600 px buffer is always built, so the analysis never depends on
//! which thumbnails the cache keeps. Roadmap #11 encodes and writes
//! [`Buffers::medium`] and [`Buffers::small`] next to the call of
//! [`analysis`] in the scanner.

use fast_image_resize::images::{Image, ImageRef};
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
use image::{GrayImage, ImageBuffer, Pixel, RgbImage};

use super::analysis;
use crate::media::Pixels;

/// The medium thumbnail's longest side.
pub const MEDIUM: u32 = 1600;
/// The small thumbnail's longest side.
pub const SMALL: u32 = 512;

/// The thumbnail pixels of one decode, before encoding.
#[derive(Debug, Clone)]
pub struct Buffers {
    /// Flattened on white, fitted inside 1600 × 1600.
    pub medium: RgbImage,
    /// Fitted inside 512 × 512 from `medium`.
    pub small: RgbImage,
    /// The source's alpha at `small`'s size, made by the same resizes as
    /// the colours; None when the source has no alpha channel.
    pub small_alpha: Option<GrayImage>,
}

/// The size that fits `w × h` inside `side × side`, keeping the aspect
/// ratio, never enlarging, each side rounded to the nearest pixel and at
/// least 1.
pub fn fit(w: u32, h: u32, side: u32) -> (u32, u32) {
    let longest = w.max(h);
    if longest <= side {
        return (w, h);
    }
    // n · side / longest, rounded half up, in integers.
    let (side, longest) = (u64::from(side), u64::from(longest));
    let scale = |n: u32| {
        let n = (u64::from(n) * side * 2 + longest) / (2 * longest);
        u32::try_from(n).unwrap_or(u32::MAX).max(1)
    };
    (scale(w), scale(h))
}

/// Resizes with Lanczos3 to fit inside `side`; a picture that already fits
/// is returned as it is.
pub fn fit_image<P>(img: ImageBuffer<P, Vec<u8>>, side: u32) -> ImageBuffer<P, Vec<u8>>
where
    P: Pixel<Subpixel = u8> + 'static,
{
    let (w, h) = fit(img.width(), img.height(), side);
    if (w, h) == img.dimensions() {
        img
    } else {
        lanczos(&img, w, h)
    }
}

/// `img` resized to `w × h` with Lanczos3 by `fast_image_resize`, which
/// works on 8-bit samples in fixed point.
pub fn lanczos<P>(img: &ImageBuffer<P, Vec<u8>>, w: u32, h: u32) -> ImageBuffer<P, Vec<u8>>
where
    P: Pixel<Subpixel = u8> + 'static,
{
    let kind = match P::CHANNEL_COUNT {
        1 => PixelType::U8,
        2 => PixelType::U8x2,
        3 => PixelType::U8x3,
        _ => PixelType::U8x4,
    };
    let mut dst = Image::new(w, h, kind);
    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3));
    let resized = ImageRef::new(img.width(), img.height(), img.as_raw(), kind)
        .map_err(|_| ())
        .and_then(|src| {
            Resizer::new()
                .resize(&src, &mut dst, &options)
                .map_err(|_| ())
        });
    resized
        .ok()
        .and_then(|()| ImageBuffer::from_raw(w, h, dst.into_vec()))
        // Neither can fail for a buffer of the stated size; the fallback
        // keeps the scan going.
        .unwrap_or_else(|| {
            image::imageops::resize(img, w, h, image::imageops::FilterType::Lanczos3)
        })
}

/// The colours flattened on white (`c·a + 255·(1 − a)`, rounded) and, when
/// the picture has alpha, the alpha plane.
pub fn flatten(px: &Pixels) -> (RgbImage, Option<GrayImage>) {
    let n = px.width as usize * px.height as usize;
    let mut rgb = Vec::with_capacity(n * 3);
    let mut alpha = px.has_alpha.then(|| Vec::with_capacity(n));
    for p in px.rgba.as_chunks::<4>().0.iter().take(n) {
        match &mut alpha {
            None => rgb.extend_from_slice(&p[..3]),
            Some(plane) => {
                let a = u32::from(p[3]);
                for &c in &p[..3] {
                    rgb.push(((u32::from(c) * a + 255 * (255 - a) + 127) / 255) as u8);
                }
                plane.push(p[3]);
            }
        }
    }
    // A handler never returns a short buffer; pad rather than panic.
    rgb.resize(n * 3, 255);
    let rgb = RgbImage::from_raw(px.width, px.height, rgb).unwrap_or_else(|| RgbImage::new(1, 1));
    let alpha = alpha.and_then(|mut plane| {
        plane.resize(n, 255);
        GrayImage::from_raw(px.width, px.height, plane)
    });
    (rgb, alpha)
}

/// Builds both thumbnail buffers from one decode. The alpha plane goes
/// through the same resizes as the colours.
pub fn buffers(px: &Pixels) -> Buffers {
    let (rgb, alpha) = flatten(px);
    let medium = fit_image(rgb, MEDIUM);
    let small = fit_image(medium.clone(), SMALL);
    let small_alpha = alpha.map(|a| fit_image(fit_image(a, MEDIUM), SMALL));
    Buffers {
        medium,
        small,
        small_alpha,
    }
}

/// The `analysis` JSON of a picture.
pub fn analysis(px: &Pixels) -> String {
    let b = buffers(px);
    analysis::analyse(&b.small, b.small_alpha.as_ref()).to_json()
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
        assert_eq!(fit(4000, 3000, 1600), (1600, 1200));
        assert_eq!(fit(3000, 4000, 1600), (1200, 1600));
        assert_eq!(fit(1600, 1200, 512), (512, 384));
        assert_eq!(fit(100, 50, 1600), (100, 50));
        assert_eq!(fit(1600, 1600, 1600), (1600, 1600));
        assert_eq!(fit(3000, 1, 256), (256, 1));
        assert_eq!(fit(1000, 333, 256), (256, 85)); // 85.248
        assert_eq!(fit(1000, 335, 256), (256, 86)); // 85.76
        assert_eq!(fit(1, 1, 256), (1, 1));
    }

    #[test]
    fn buffers_are_1600_then_512_and_never_enlarged() {
        let b = buffers(&picture(4000, 2000, false, |_, _| [9, 9, 9, 255]));
        assert_eq!(b.medium.dimensions(), (1600, 800));
        assert_eq!(b.small.dimensions(), (512, 256));
        assert!(b.small_alpha.is_none());
        let b = buffers(&picture(300, 200, false, |_, _| [9, 9, 9, 255]));
        assert_eq!(b.medium.dimensions(), (300, 200));
        assert_eq!(b.small.dimensions(), (300, 200));
        assert_eq!(b.small.get_pixel(5, 5).0, [9, 9, 9]);
    }

    #[test]
    fn transparency_is_flattened_on_white_and_its_alpha_kept() {
        let p = picture(4, 2, true, |x, _| match x {
            0 => [0, 0, 0, 0],
            1 => [0, 0, 0, 255],
            2 => [200, 0, 0, 128],
            _ => [10, 20, 30, 255],
        });
        let (rgb, alpha) = flatten(&p);
        assert_eq!(rgb.get_pixel(0, 0).0, [255, 255, 255]);
        assert_eq!(rgb.get_pixel(1, 0).0, [0, 0, 0]);
        // 200·128/255 + 255·127/255 = 100.39 + 127 → 227; 0 → 127.
        assert_eq!(rgb.get_pixel(2, 0).0, [227, 127, 127]);
        assert_eq!(rgb.get_pixel(3, 1).0, [10, 20, 30]);
        let alpha = alpha.unwrap();
        assert_eq!(alpha.get_pixel(0, 0).0, [0]);
        assert_eq!(alpha.get_pixel(2, 1).0, [128]);
        // A picture without alpha has no plane, and its alpha bytes are
        // ignored.
        let p = picture(2, 2, false, |_, _| [1, 2, 3, 0]);
        let (rgb, alpha) = flatten(&p);
        assert_eq!(rgb.get_pixel(0, 0).0, [1, 2, 3]);
        assert!(alpha.is_none());
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
        let b = buffers(&big);
        assert_eq!(b.small_alpha.as_ref().unwrap().dimensions(), (512, 256));
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
        let b = buffers(&px);
        let mid = t.elapsed();
        let a = analysis::analyse(&b.small, None);
        println!(
            "buffers {mid:?}, analysis {:?} ({})",
            t.elapsed() - mid,
            a.dhash
        );
    }
}
