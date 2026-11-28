//! CMYK JPEG pictures converted through their CMYK profile to sRGB (#82;
//! docs/03-media-and-scanning.md, "The still handler").
//!
//! `image` decodes a CMYK JPEG with `zune-jpeg`, which turns CMYK into RGB
//! with a plain formula, after which the CMYK profile can no longer apply.
//! A JPEG whose embedded profile is CMYK is therefore decoded again here,
//! to its raw samples, and converted with a `moxcms` CMYK to sRGB
//! transform. A CMYK JPEG without a CMYK profile keeps the formula.
//!
//! Samples: a CMYK JPEG written with an Adobe marker (Photoshop and nearly
//! every other writer) stores each channel inverted, 255 for no ink; a
//! YCCK one stores the inks' C, M and Y as YCbCr and K inverted. An ICC
//! CMYK profile takes 0 for no ink.

use std::panic::{AssertUnwindSafe, catch_unwind};

use image::RgbImage;
use moxcms::{ColorProfile, DataColorSpace, TransformOptions};
use zune_core::bytestream::ZCursor;
use zune_core::colorspace::ColorSpace;
use zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

/// True when `icc` reads as a CMYK profile.
pub fn is_cmyk_profile(icc: &[u8]) -> bool {
    catch_unwind(|| ColorProfile::new_from_slice(icc).ok())
        .ok()
        .flatten()
        .is_some_and(|p| p.color_space == DataColorSpace::Cmyk)
}

/// A CMYK or YCCK JPEG's samples as ink amounts (0 for no ink), 4 bytes a
/// pixel, at stored orientation. None when the file is not one.
fn inks(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut probe = JpegDecoder::new(ZCursor::new(bytes));
    probe.decode_headers().ok()?;
    let input = probe.input_colorspace()?;
    if !matches!(input, ColorSpace::CMYK | ColorSpace::YCCK) {
        return None;
    }
    // Asking for the input's own colour space gives the samples untouched.
    let options = DecoderOptions::default().jpeg_set_out_colorspace(input);
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    let mut samples = decoder.decode().ok()?;
    let info = decoder.info()?;
    let (w, h) = (u32::from(info.width), u32::from(info.height));
    if samples.len() != w as usize * h as usize * 4 {
        return None;
    }
    for p in samples.as_chunks_mut::<4>().0 {
        if input == ColorSpace::YCCK {
            let [c, m, y] = ycc_to_rgb(p[0], p[1], p[2]);
            *p = [c, m, y, 255 - p[3]];
        } else {
            *p = [255 - p[0], 255 - p[1], 255 - p[2], 255 - p[3]];
        }
    }
    Some((w, h, samples))
}

/// JPEG's YCbCr to RGB (full range).
fn ycc_to_rgb(y: u8, cb: u8, cr: u8) -> [u8; 3] {
    let (y, cb, cr) = (f32::from(y), f32::from(cb) - 128.0, f32::from(cr) - 128.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let clamp = |v: f32| v.round().clamp(0.0, 255.0) as u8;
    [
        clamp(y + 1.402 * cr),
        clamp(y - 0.344_136 * cb - 0.714_136 * cr),
        clamp(y + 1.772 * cb),
    ]
}

/// Ink amounts (4 bytes a pixel) converted to sRGB through the CMYK
/// profile. None when the profile cannot be used.
pub fn inks_to_srgb(inks: &[u8], icc: &[u8]) -> Option<Vec<u8>> {
    // The bytes come from the user's files: a library panic on a hostile
    // profile is treated like a profile it refused.
    catch_unwind(AssertUnwindSafe(|| {
        let profile = ColorProfile::new_from_slice(icc).ok()?;
        if profile.color_space != DataColorSpace::Cmyk {
            return None;
        }
        let transform = profile
            .create_transform_8bit(
                moxcms::Layout::Rgba,
                &ColorProfile::new_srgb(),
                moxcms::Layout::Rgb,
                TransformOptions::default(),
            )
            .ok()?;
        let mut rgb = vec![0u8; inks.len() / 4 * 3];
        transform.transform(inks, &mut rgb).ok()?;
        Some(rgb)
    }))
    .ok()
    .flatten()
}

/// The JPEG in `bytes` converted through its CMYK profile `icc` into
/// sRGB, at stored orientation. None when it is not a CMYK JPEG or the
/// profile cannot be used; the caller keeps `image`'s own decode then.
pub fn jpeg_to_srgb(bytes: &[u8], icc: &[u8]) -> Option<RgbImage> {
    let (w, h, inks) = inks(bytes)?;
    RgbImage::from_raw(w, h, inks_to_srgb(&inks, icc)?)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// sRGB (0 to 255) to CIELAB under D50, as an ICC profile's PCS.
    fn srgb_to_lab(rgb: [f64; 3]) -> [f64; 3] {
        let lin = rgb.map(|v| {
            let v = v / 255.0;
            if v <= 0.040_45 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        });
        // sRGB to XYZ, adapted to D50 (Bradford).
        let m = [
            [0.436_074_7, 0.385_064_9, 0.143_080_4],
            [0.222_504_5, 0.716_878_6, 0.060_616_9],
            [0.013_932_2, 0.097_104_5, 0.714_173_3],
        ];
        let xyz = m.map(|row| row[0] * lin[0] + row[1] * lin[1] + row[2] * lin[2]);
        let white = [0.9642, 1.0, 0.8249];
        let f = |t: f64| {
            if t > 216.0 / 24389.0 {
                t.cbrt()
            } else {
                (24389.0 / 27.0 * t + 16.0) / 116.0
            }
        };
        let [fx, fy, fz] = [
            f(xyz[0] / white[0]),
            f(xyz[1] / white[1]),
            f(xyz[2] / white[2]),
        ];
        [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
    }

    /// The sRGB colour of each ink alone in the test profile: unlike the
    /// plain formula (cyan ink gives 0, 255, 255), its cyan is a print
    /// cyan. Channels stay clear of 0, where 8-bit Lab is too coarse.
    pub(crate) const INK_COLOURS: [[u8; 3]; 4] =
        [[60, 160, 230], [230, 60, 140], [250, 235, 60], [0, 0, 0]];

    fn be(v: u32) -> [u8; 4] {
        v.to_be_bytes()
    }

    /// A small ICC v2 output profile, CMYK to Lab, whose A2B0 table
    /// (`lut8Type`, 2 grid points a channel) mixes [`INK_COLOURS`]: every
    /// corner of the CMYK cube is the product of its inks' colours.
    pub(crate) fn cmyk_profile() -> Vec<u8> {
        let mut clut = Vec::new();
        // The first input channel varies slowest.
        for corner in 0..16u8 {
            let mut rgb = [255.0f64; 3];
            for (ink, colour) in INK_COLOURS.iter().enumerate() {
                if corner >> (3 - ink) & 1 == 1 {
                    for (v, c) in rgb.iter_mut().zip(colour) {
                        *v *= f64::from(*c) / 255.0;
                    }
                }
            }
            let [l, a, b] = srgb_to_lab(rgb);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            clut.extend([
                (l * 255.0 / 100.0).round().clamp(0.0, 255.0) as u8,
                (a + 128.0).round().clamp(0.0, 255.0) as u8,
                (b + 128.0).round().clamp(0.0, 255.0) as u8,
            ]);
        }
        let identity: Vec<u8> = (0..=255).collect();
        let mut lut = b"mft1\0\0\0\0".to_vec();
        lut.extend_from_slice(&[4, 3, 2, 0]);
        // The matrix (identity, used only with an XYZ PCS).
        for i in 0..9 {
            lut.extend_from_slice(&be(if i % 4 == 0 { 0x1_0000 } else { 0 }));
        }
        for _ in 0..4 {
            lut.extend_from_slice(&identity);
        }
        lut.extend_from_slice(&clut);
        for _ in 0..3 {
            lut.extend_from_slice(&identity);
        }

        let mut wtpt = b"XYZ \0\0\0\0".to_vec();
        for v in [0x0000_F6D6u32, 0x0001_0000, 0x0000_D32D] {
            wtpt.extend_from_slice(&be(v));
        }
        let mut desc = b"desc\0\0\0\0".to_vec();
        desc.extend_from_slice(&be(5));
        desc.extend_from_slice(b"CMYK\0");
        desc.extend_from_slice(&[0; 4 + 4 + 2 + 1 + 67]);

        let tags: [(&[u8; 4], &[u8]); 3] = [(b"desc", &desc), (b"wtpt", &wtpt), (b"A2B0", &lut)];
        let mut body = Vec::new();
        let mut table = be(3).to_vec();
        let mut at = 128 + 4 + 12 * tags.len();
        for (sig, data) in tags {
            table.extend_from_slice(sig);
            table.extend_from_slice(&be(u32::try_from(at).unwrap()));
            table.extend_from_slice(&be(u32::try_from(data.len()).unwrap()));
            body.extend_from_slice(data);
            while body.len() % 4 != 0 {
                body.push(0);
            }
            at = 128 + 4 + 12 * tags.len() + body.len();
        }
        let size = 128 + table.len() + body.len();
        let mut header = Vec::with_capacity(128);
        header.extend_from_slice(&be(u32::try_from(size).unwrap()));
        header.extend_from_slice(b"none");
        header.extend_from_slice(&be(0x0210_0000));
        header.extend_from_slice(b"prtrCMYKLab ");
        header.extend_from_slice(&[0; 12]);
        header.extend_from_slice(b"acsp");
        // Platform, flags, maker, model, attributes, intent; then D50.
        header.extend_from_slice(&[0; 4 + 4 + 4 + 4 + 8 + 4]);
        for v in [0x0000_F6D6u32, 0x0001_0000, 0x0000_D32D] {
            header.extend_from_slice(&be(v));
        }
        header.resize(128, 0);
        [header, table, body].concat()
    }

    /// A CMYK JPEG (Adobe, inverted) of `inks`, 4 bytes a pixel, with the
    /// profile embedded.
    pub(crate) fn cmyk_jpeg(width: u16, height: u16, inks: &[u8], icc: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = jpeg_encoder::Encoder::new(&mut out, 100);
        enc.set_sampling_factor(jpeg_encoder::SamplingFactor::F_1_1);
        enc.add_icc_profile(icc).unwrap();
        enc.encode(inks, width, height, jpeg_encoder::ColorType::Cmyk)
            .unwrap();
        out
    }

    fn near(got: &[u8], want: [u8; 3], tolerance: u8) -> bool {
        got.iter()
            .zip(want)
            .all(|(g, w)| g.abs_diff(w) <= tolerance)
    }

    #[test]
    fn the_test_profile_reads_as_cmyk() {
        assert!(is_cmyk_profile(&cmyk_profile()));
        assert!(!is_cmyk_profile(&super::super::colour::srgb_profile()));
        assert!(!is_cmyk_profile(b"not a profile"));
    }

    #[test]
    fn inks_convert_through_the_profile() {
        let inks = [[0, 0, 0, 0], [255, 0, 0, 0], [0, 255, 0, 0], [0, 0, 0, 255]].concat();
        let rgb = inks_to_srgb(&inks, &cmyk_profile()).unwrap();
        assert!(
            near(&rgb[0..3], [255, 255, 255], 3),
            "paper {:?}",
            &rgb[0..3]
        );
        assert!(near(&rgb[3..6], INK_COLOURS[0], 6), "cyan {:?}", &rgb[3..6]);
        assert!(
            near(&rgb[6..9], INK_COLOURS[1], 6),
            "magenta {:?}",
            &rgb[6..9]
        );
        assert!(near(&rgb[9..12], [0, 0, 0], 6), "black {:?}", &rgb[9..12]);
    }

    #[test]
    fn a_cmyk_jpeg_is_decoded_to_its_inks() {
        let icc = cmyk_profile();
        // Left half cyan ink, right half none, 16 × 8.
        let inks: Vec<u8> = (0..16 * 8)
            .flat_map(|i| if i % 16 < 8 { [255, 0, 0, 0] } else { [0; 4] })
            .collect();
        let jpeg = cmyk_jpeg(16, 8, &inks, &icc);
        let img = jpeg_to_srgb(&jpeg, &icc).unwrap();
        assert_eq!(img.dimensions(), (16, 8));
        assert!(
            near(&img.get_pixel(2, 4).0, INK_COLOURS[0], 8),
            "{:?}",
            img.get_pixel(2, 4)
        );
        assert!(
            near(&img.get_pixel(13, 4).0, [255, 255, 255], 8),
            "{:?}",
            img.get_pixel(13, 4)
        );
        // An RGB JPEG is not one.
        let mut rgb = Vec::new();
        jpeg_encoder::Encoder::new(&mut rgb, 90)
            .encode(&[0; 8 * 8 * 3], 8, 8, jpeg_encoder::ColorType::Rgb)
            .unwrap();
        assert!(jpeg_to_srgb(&rgb, &icc).is_none());
    }

    #[test]
    fn the_jpeg_handler_converts_a_cmyk_file_through_its_profile() {
        use crate::media::MediaHandler;
        let icc = cmyk_profile();
        let inks: Vec<u8> = (0..8 * 8).flat_map(|_| [255, 0, 0, 0]).collect();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("print.jpg");
        std::fs::write(&path, cmyk_jpeg(8, 8, &inks, &icc)).unwrap();
        let jpeg = &super::super::still::JPEG;
        for px in [
            jpeg.working(&path).unwrap(),
            jpeg.preview(&path, 4).unwrap(),
        ] {
            assert!(
                near(&px.pixel(4, 4)[..3], INK_COLOURS[0], 8),
                "{:?}",
                px.pixel(4, 4)
            );
        }
        // The picture is sRGB now, so the renderer gets no profile.
        assert_eq!(jpeg.source(&path).unwrap().icc, None);
    }

    #[test]
    fn a_ycck_jpeg_is_decoded_to_its_inks() {
        let icc = cmyk_profile();
        let inks: Vec<u8> = (0..8 * 8).flat_map(|_| [0, 255, 0, 0]).collect();
        let mut out = Vec::new();
        let mut enc = jpeg_encoder::Encoder::new(&mut out, 100);
        enc.set_sampling_factor(jpeg_encoder::SamplingFactor::F_1_1);
        enc.encode(&inks, 8, 8, jpeg_encoder::ColorType::CmykAsYcck)
            .unwrap();
        let img = jpeg_to_srgb(&out, &icc).unwrap();
        assert!(
            near(&img.get_pixel(4, 4).0, INK_COLOURS[1], 8),
            "{:?}",
            img.get_pixel(4, 4)
        );
    }
}
