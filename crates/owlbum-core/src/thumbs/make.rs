//! What a thumbnail and a zoom copy are made of
//! (docs/03-media-and-scanning.md, "What a thumbnail is"): flattened on
//! white, resized with Lanczos3 without enlarging, and encoded as an
//! opaque 4:2:0 JPEG with no metadata.

use fast_image_resize::images::{Image, ImageRef};
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
use image::{GrayImage, ImageBuffer, Pixel, RgbImage};
use jpeg_encoder::{ColorType, Encoder, SamplingFactor};

use crate::media::Pixels;

/// The small thumbnail's longest side.
pub const SMALL_SIDE: u32 = 512;
/// The medium thumbnail's longest side.
pub const MEDIUM_SIDE: u32 = 1600;
/// The small thumbnail's JPEG quality (a starting value; docs/03, open
/// question 7).
pub const SMALL_QUALITY: u8 = 78;
/// The medium thumbnail's JPEG quality.
pub const MEDIUM_QUALITY: u8 = 84;
/// The zoom copy's JPEG quality.
pub const FULL_QUALITY: u8 = 90;
/// The longest side a JPEG can have.
pub const JPEG_MAX_SIDE: u32 = 65_535;

/// The two thumbnails' pixels from one decode: the medium one first, the
/// small one resized from it.
#[derive(Debug, Clone)]
pub struct Buffers {
    /// At most [`MEDIUM_SIDE`] on its longest side.
    pub medium: RgbImage,
    /// At most [`SMALL_SIDE`] on its longest side. The analysis image is
    /// made from these pixels (docs/03, "Image analysis").
    pub small: RgbImage,
}

/// Flattens `px` on white and makes both thumbnail buffers.
pub fn buffers(px: &Pixels) -> Buffers {
    let medium = fit(flatten(px), MEDIUM_SIDE);
    let small = fit(medium.clone(), SMALL_SIDE);
    Buffers { medium, small }
}

/// The picture with its transparency laid over white (`#ffffff`).
pub fn flatten(px: &Pixels) -> RgbImage {
    let mut out = Vec::with_capacity(px.width as usize * px.height as usize * 3);
    for p in px.rgba.as_chunks::<4>().0 {
        let a = u32::from(p[3]);
        for &c in &p[..3] {
            let v = (u32::from(c) * a + 255 * (255 - a) + 127) / 255;
            out.push(u8::try_from(v).unwrap_or(u8::MAX));
        }
    }
    RgbImage::from_raw(px.width, px.height, out).unwrap_or_else(|| RgbImage::new(1, 1))
}

/// The alpha plane of `px`, when the picture has one.
pub fn alpha(px: &Pixels) -> Option<GrayImage> {
    if !px.has_alpha {
        return None;
    }
    let plane = px.rgba.as_chunks::<4>().0.iter().map(|p| p[3]).collect();
    GrayImage::from_raw(px.width, px.height, plane)
}

/// `img` resized with Lanczos3 to fit inside `side` × `side`, keeping its
/// aspect ratio, each side at least 1 px. A picture that already fits is
/// returned as it is: never enlarged.
pub fn fit<P>(img: ImageBuffer<P, Vec<u8>>, side: u32) -> ImageBuffer<P, Vec<u8>>
where
    P: Pixel<Subpixel = u8> + 'static,
{
    let (w, h) = img.dimensions();
    let (nw, nh) = fitted(w, h, side);
    if (nw, nh) == (w, h) {
        img
    } else {
        lanczos(&img, nw, nh)
    }
}

/// `img` resized to `w × h` with Lanczos3 by `fast_image_resize`, which
/// works on 8-bit samples in fixed point (docs/03, "Image analysis": the
/// analysis numbers depend on it, so changing it raises `SCAN_VERSION`).
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

/// The size of a `w` × `h` picture fitted inside `side` × `side`.
pub fn fitted(w: u32, h: u32, side: u32) -> (u32, u32) {
    let long = w.max(h);
    if long <= side {
        return (w, h);
    }
    let scale = |n: u32| {
        let v = (u64::from(n) * u64::from(side) + u64::from(long) / 2) / u64::from(long);
        u32::try_from(v).unwrap_or(side).max(1)
    };
    (scale(w), scale(h))
}

/// Encodes `img` as a baseline JPEG: 4:2:0 chroma, no metadata, no
/// embedded profile. A picture larger than a JPEG can hold is scaled down
/// to fit first.
pub fn encode(img: &RgbImage, quality: u8) -> Vec<u8> {
    let fitted;
    let img = if img.width().max(img.height()) > JPEG_MAX_SIDE {
        fitted = fit(img.clone(), JPEG_MAX_SIDE);
        &fitted
    } else {
        img
    };
    let (w, h) = img.dimensions();
    let mut out = Vec::new();
    let mut encoder = Encoder::new(&mut out, quality);
    encoder.set_sampling_factor(SamplingFactor::R_4_2_0);
    // Both sides fit in u16 after the fit above.
    let (w16, h16) = (
        u16::try_from(w).unwrap_or(u16::MAX),
        u16::try_from(h).unwrap_or(u16::MAX),
    );
    if encoder
        .encode(img.as_raw(), w16, h16, ColorType::Rgb)
        .is_err()
    {
        // Only a zero-sized picture fails, and a decoded picture never is.
        return Vec::new();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(w: u32, h: u32, rgba: [u8; 4]) -> Pixels {
        Pixels {
            width: w,
            height: h,
            full_width: w,
            full_height: h,
            rgba: rgba.repeat((w * h) as usize),
            has_alpha: rgba[3] != 255,
        }
    }

    /// The SOF0 segment's size and each component's sampling factors.
    fn sof(jpeg: &[u8]) -> (u16, u16, Vec<(u8, u8)>) {
        let mut i = 2;
        while i + 4 < jpeg.len() {
            assert_eq!(jpeg[i], 0xFF);
            let marker = jpeg[i + 1];
            let len = usize::from(u16::from_be_bytes([jpeg[i + 2], jpeg[i + 3]]));
            if marker == 0xC0 {
                let s = &jpeg[i + 4..];
                let h = u16::from_be_bytes([s[1], s[2]]);
                let w = u16::from_be_bytes([s[3], s[4]]);
                let n = usize::from(s[5]);
                let comps = (0..n)
                    .map(|c| {
                        let f = s[6 + c * 3 + 1];
                        (f >> 4, f & 0x0F)
                    })
                    .collect();
                return (w, h, comps);
            }
            i += 2 + len;
        }
        panic!("no SOF0");
    }

    /// The markers before the first scan.
    fn markers(jpeg: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut i = 2;
        while i + 4 < jpeg.len() {
            let marker = jpeg[i + 1];
            out.push(marker);
            if marker == 0xDA {
                break;
            }
            i += 2 + usize::from(u16::from_be_bytes([jpeg[i + 2], jpeg[i + 3]]));
        }
        out
    }

    #[test]
    fn transparency_is_laid_over_white() {
        let flat = flatten(&px(2, 1, [0, 0, 0, 0]));
        assert_eq!(flat.get_pixel(0, 0).0, [255, 255, 255]);
        let half = flatten(&px(1, 1, [0, 100, 255, 128]));
        // 0 over white at 50% is about 127; 255 stays 255.
        assert_eq!(half.get_pixel(0, 0).0, [127, 177, 255]);
        let opaque = flatten(&px(1, 1, [10, 20, 30, 255]));
        assert_eq!(opaque.get_pixel(0, 0).0, [10, 20, 30]);
    }

    #[test]
    fn buffers_fit_1600_and_512_keeping_the_aspect_ratio() {
        let b = buffers(&px(4000, 3000, [200, 100, 50, 255]));
        assert_eq!(b.medium.dimensions(), (1600, 1200));
        assert_eq!(b.small.dimensions(), (512, 384));
        let tall = buffers(&px(300, 3000, [0, 0, 0, 255]));
        assert_eq!(tall.medium.dimensions(), (160, 1600));
        assert_eq!(tall.small.dimensions(), (51, 512));
    }

    #[test]
    fn a_small_picture_is_never_enlarged() {
        let b = buffers(&px(300, 200, [1, 2, 3, 255]));
        assert_eq!(b.medium.dimensions(), (300, 200));
        assert_eq!(b.small.dimensions(), (300, 200));
        let mid = buffers(&px(1000, 10, [1, 2, 3, 255]));
        assert_eq!(mid.medium.dimensions(), (1000, 10));
        assert_eq!(mid.small.dimensions(), (512, 5));
    }

    #[test]
    fn a_thin_picture_keeps_at_least_one_pixel() {
        assert_eq!(fitted(100_000, 1, 512), (512, 1));
        assert_eq!(fitted(1, 100_000, 1600), (1, 1600));
    }

    #[test]
    fn thumbnails_are_420_jpegs_with_no_metadata() {
        let b = buffers(&px(800, 600, [30, 140, 220, 255]));
        let jpeg = encode(&b.small, SMALL_QUALITY);
        assert_eq!(&jpeg[..2], [0xFF, 0xD8]);
        assert_eq!(&jpeg[jpeg.len() - 2..], [0xFF, 0xD9]);
        let (w, h, comps) = sof(&jpeg);
        assert_eq!((w, h), (512, 384));
        assert_eq!(comps, [(2, 2), (1, 1), (1, 1)], "4:2:0 chroma");
        // No APP1 (EXIF, XMP) and no APP2 (ICC profile).
        let m = markers(&jpeg);
        assert!(!m.contains(&0xE1) && !m.contains(&0xE2), "{m:02X?}");
        // It decodes back to the same size, opaque.
        let back = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((back.width(), back.height()), (512, 384));
        assert!(!back.color().has_alpha());
    }

    #[test]
    fn a_picture_over_the_jpeg_limit_is_scaled_down_to_fit() {
        let wide = RgbImage::new(65_536, 1);
        let (w, h, _) = sof(&encode(&wide, FULL_QUALITY));
        assert_eq!((w, h), (65_535, 1));
    }
}
