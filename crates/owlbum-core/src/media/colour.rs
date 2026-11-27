//! The one place decoded pixels are brought to sRGB
//! (docs/03-media-and-scanning.md, "Media handlers": the sRGB rule).
//!
//! Every handler's `preview` and `working` result passes through
//! [`to_srgb`]. `source` never does: it keeps the file's own depth and
//! profile for the edit renderer.
//!
//! The colour library is `moxcms`, chosen by measurement in #7 against
//! `lcms2` and `qcms` (docs/03-media-and-scanning.md, open question 8).
//! The rules:
//!
//! - An RGB profile converts the colour channels; a grey profile converts
//!   a grey picture (R = G = B). Alpha is never touched.
//! - A profile that cannot be read or used, one for another colour space
//!   (a CMYK JPEG reaches here already turned into RGB by its decoder), or
//!   a grey profile on a colour picture leaves the pixels as they are. A
//!   damaged profile must never cost the user the picture.
//! - A profile that gives sRGB back (within one level on a test lattice)
//!   is skipped, so the many files that embed sRGB cost nothing.
//! - The picture is converted in strips through a small scratch buffer,
//!   so a large picture is never held twice.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use moxcms::{ColorProfile, DataColorSpace, TransformOptions};

use super::handler::Pixels;

/// Pixels converted per call into the library.
const STRIP: usize = 16 * 1024;

/// Converts 8-bit pixels described by the ICC profile `icc` to sRGB. With no
/// profile the pixels are taken as sRGB already.
pub fn to_srgb(mut pixels: Pixels, icc: Option<&[u8]>) -> Pixels {
    if let Some(c) = icc.and_then(Conversion::new) {
        c.apply(&mut pixels.rgba);
    }
    pixels
}

/// True when [`to_srgb`] leaves pixels in `icc` as they are: no profile,
/// an sRGB profile, or one that cannot be used.
pub fn is_srgb(icc: Option<&[u8]>) -> bool {
    icc.and_then(Conversion::new).is_none()
}

/// The bytes of an sRGB ICC profile, for an edited file whose pixels were
/// converted to sRGB (docs/06, "Colour profiles").
pub fn srgb_profile() -> Vec<u8> {
    ColorProfile::new_srgb().encode().unwrap_or_default()
}

/// True when `icc` describes RGB: the only profile an RGB file may embed.
pub fn is_rgb_profile(icc: &[u8]) -> bool {
    icc.get(16..20) == Some(b"RGB ")
}

/// Converts 16-bit RGBA samples described by `icc` to sRGB in place, by
/// the same rules as [`to_srgb`] (the edit renderer's 16-bit path). A
/// profile that cannot be used, or that is sRGB already, leaves them.
pub fn to_srgb_16(rgba: &mut [u16], icc: Option<&[u8]>) {
    let Some(icc) = icc else { return };
    if is_srgb(Some(icc)) || !rgba.len().is_multiple_of(4) {
        return;
    }
    let built = catch_unwind(AssertUnwindSafe(|| {
        let profile = ColorProfile::new_from_slice(icc).ok()?;
        let (source, layout) = match profile.color_space {
            DataColorSpace::Rgb => (Source::Rgb, moxcms::Layout::Rgba),
            DataColorSpace::Gray => (Source::Grey, moxcms::Layout::GrayAlpha),
            _ => return None,
        };
        let t = profile
            .create_transform_16bit(
                layout,
                &ColorProfile::new_srgb(),
                moxcms::Layout::Rgba,
                TransformOptions::default(),
            )
            .ok()?;
        Some((source, t))
    }));
    let Ok(Some((source, transform))) = built else {
        return;
    };
    if let Source::Grey = source
        && !rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p[0] == p[1] && p[1] == p[2])
    {
        return;
    }
    let mut scratch = Vec::with_capacity(STRIP * 4);
    for strip in rgba.chunks_mut(STRIP * 4) {
        scratch.clear();
        match source {
            Source::Rgb => scratch.extend_from_slice(strip),
            Source::Grey => {
                scratch.extend(strip.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[3]]));
            }
        }
        if transform.transform(&scratch, strip).is_err()
            && let Source::Rgb = source
        {
            strip.copy_from_slice(&scratch);
        }
    }
}

/// What the profile describes.
#[derive(Clone, Copy)]
enum Source {
    Rgb,
    Grey,
}

/// A profile ready to convert pixels to sRGB.
struct Conversion {
    source: Source,
    transform: Arc<moxcms::Transform8BitExecutor>,
}

impl Conversion {
    /// None when converting would change nothing or cannot be done.
    fn new(icc: &[u8]) -> Option<Self> {
        // The bytes come from the user's files. A library panic on a
        // hostile profile is treated like a profile it refused.
        catch_unwind(AssertUnwindSafe(|| Self::build(icc)))
            .ok()
            .flatten()
    }

    fn build(icc: &[u8]) -> Option<Self> {
        let profile = ColorProfile::new_from_slice(icc).ok()?;
        let (source, layout) = match profile.color_space {
            DataColorSpace::Rgb => (Source::Rgb, moxcms::Layout::Rgba),
            DataColorSpace::Gray => (Source::Grey, moxcms::Layout::GrayAlpha),
            _ => return None,
        };
        let transform = profile
            .create_transform_8bit(
                layout,
                &ColorProfile::new_srgb(),
                moxcms::Layout::Rgba,
                TransformOptions::default(),
            )
            .ok()?;
        let c = Self { source, transform };
        (!c.gives_back_srgb()).then_some(c)
    }

    /// True when every colour of a test lattice (every grey level, for a
    /// grey profile) comes back within one level.
    fn gives_back_srgb(&self) -> bool {
        let lattice: Vec<u8> = match self.source {
            Source::Rgb => {
                let steps = (0..=8u16).map(|i| (i * 255 / 8) as u8);
                steps
                    .clone()
                    .flat_map(|r| {
                        let steps = steps.clone();
                        steps
                            .clone()
                            .flat_map(move |g| steps.clone().flat_map(move |b| [r, g, b, 255]))
                    })
                    .collect()
            }
            Source::Grey => (0..=255u8).flat_map(|v| [v, v, v, 255]).collect(),
        };
        let mut converted = lattice.clone();
        // A transform that fails here would fail on the picture too.
        !self.apply(&mut converted)
            || converted
                .iter()
                .zip(&lattice)
                .all(|(a, b)| a.abs_diff(*b) <= 1)
    }

    /// Converts RGBA pixels in place. False, with the pixels unchanged,
    /// when they do not fit the profile.
    fn apply(&self, rgba: &mut [u8]) -> bool {
        if !rgba.len().is_multiple_of(4) {
            return false;
        }
        if let Source::Grey = self.source
            && !rgba
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| p[0] == p[1] && p[1] == p[2])
        {
            return false;
        }
        let mut scratch = Vec::with_capacity(STRIP * 4);
        for strip in rgba.chunks_mut(STRIP * 4) {
            scratch.clear();
            match self.source {
                Source::Rgb => scratch.extend_from_slice(strip),
                Source::Grey => {
                    scratch.extend(strip.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[3]]))
                }
            }
            // Lengths always match, so the library has no reason to fail; if
            // it does, the strip is put back as it was.
            if self.transform.transform(&scratch, strip).is_err() {
                match self.source {
                    Source::Rgb => strip.copy_from_slice(&scratch),
                    Source::Grey => {
                        for (p, ga) in strip
                            .as_chunks_mut::<4>()
                            .0
                            .iter_mut()
                            .zip(scratch.as_chunks::<2>().0)
                        {
                            p.copy_from_slice(&[ga[0], ga[0], ga[0], ga[1]]);
                        }
                    }
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use owlbum_fixtures::icc::{self, RgbProfile};

    use super::*;

    /// One pixel per colour, alpha `alpha`.
    fn pixels(colours: &[[u8; 3]], alpha: u8) -> Pixels {
        let n = u32::try_from(colours.len()).unwrap();
        Pixels {
            width: n,
            height: 1,
            full_width: n,
            full_height: 1,
            rgba: colours
                .iter()
                .flat_map(|c| [c[0], c[1], c[2], alpha])
                .collect(),
            has_alpha: alpha != 255,
        }
    }

    /// Every 8-bit colour on a 9 × 9 × 9 lattice, corners included.
    fn lattice() -> Vec<[u8; 3]> {
        let steps: Vec<u8> = (0..=8).map(|i| (i * 255 / 8) as u8).collect();
        let mut v = Vec::new();
        for &r in &steps {
            for &g in &steps {
                for &b in &steps {
                    v.push([r, g, b]);
                }
            }
        }
        v
    }

    /// Converts the lattice through `p` and checks each colour against the
    /// value worked out from the profile's numbers. Returns how far the
    /// colours moved, at most, so a test can tell a conversion happened.
    fn check_rgb(p: &RgbProfile) -> u8 {
        let colours = lattice();
        let out = to_srgb(pixels(&colours, 255), Some(&p.bytes()));
        let mut moved = 0;
        for (c, got) in colours.iter().zip(out.rgba.as_chunks::<4>().0) {
            let want = p.expected_srgb(*c);
            for k in 0..3 {
                assert!(
                    (f64::from(got[k]) - want[k]).abs() <= 2.0,
                    "{}: {c:?} gave {got:?}, want {want:?}",
                    p.name
                );
                moved = moved.max(got[k].abs_diff(c[k]));
            }
        }
        moved
    }

    #[test]
    fn no_profile_leaves_pixels_alone() {
        let px = pixels(&lattice(), 255);
        assert_eq!(to_srgb(px.clone(), None), px);
        assert!(is_srgb(None));
    }

    #[test]
    fn display_p3_is_converted() {
        assert!(check_rgb(&icc::DISPLAY_P3) >= 10);
        assert!(check_rgb(&icc::DISPLAY_P3_GAMMA_22) >= 10);
        assert!(!is_srgb(Some(&icc::DISPLAY_P3.bytes())));
    }

    #[test]
    fn adobe_rgb_is_converted() {
        assert!(check_rgb(&icc::ADOBE_RGB) >= 10);
        assert!(!is_srgb(Some(&icc::ADOBE_RGB.bytes())));
    }

    #[test]
    fn an_in_gamut_p3_colour_lands_where_expected() {
        // Inside the sRGB gamut, so nothing is clipped: a mid orange.
        let c = [200, 100, 50];
        let want = icc::DISPLAY_P3.expected_srgb(c);
        assert!(want.iter().all(|v| (1.0..254.0).contains(v)), "{want:?}");
        let got = to_srgb(pixels(&[c], 255), Some(&icc::DISPLAY_P3.bytes())).pixel(0, 0);
        for k in 0..3 {
            assert!(
                (f64::from(got[k]) - want[k]).abs() <= 1.5,
                "{got:?} {want:?}"
            );
        }
        assert!(got[0].abs_diff(c[0]) >= 10, "{got:?}");
    }

    #[test]
    fn a_grey_profile_converts_grey_pixels() {
        let gamma = 461; // 1.8
        let levels: Vec<[u8; 3]> = (0..=255u8).map(|v| [v, v, v]).collect();
        let out = to_srgb(pixels(&levels, 255), Some(&icc::grey(gamma)));
        let mut moved = 0;
        for (v, got) in (0..=255u8).zip(out.rgba.as_chunks::<4>().0) {
            let want = icc::expected_grey_srgb(gamma, v);
            for k in 0..3 {
                assert!(
                    (f64::from(got[k]) - want).abs() <= 2.0,
                    "{v} gave {got:?}, want {want}"
                );
            }
            moved = moved.max(got[0].abs_diff(v));
        }
        assert!(moved >= 10);
        assert!(!is_srgb(Some(&icc::grey(gamma))));
    }

    #[test]
    fn a_grey_profile_on_colour_pixels_changes_nothing() {
        // A grey profile cannot describe colour; the file is wrong, so the
        // pixels are shown as they are.
        let px = pixels(&[[10, 10, 10], [200, 100, 50]], 255);
        assert_eq!(to_srgb(px.clone(), Some(&icc::grey(461))), px);
    }

    #[test]
    fn alpha_is_kept_exactly() {
        let colours = lattice();
        for icc in [
            icc::DISPLAY_P3.bytes(),
            icc::ADOBE_RGB.bytes(),
            icc::grey(461),
        ] {
            let mut px = pixels(&colours, 0);
            for (i, p) in px.rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                p[3] = (i % 256) as u8;
                if icc[16..20] == *b"GRAY" {
                    (p[1], p[2]) = (p[0], p[0]);
                }
            }
            let out = to_srgb(px.clone(), Some(&icc));
            assert_ne!(out.rgba, px.rgba, "nothing was converted");
            let alpha = |p: &Pixels| {
                p.rgba
                    .iter()
                    .skip(3)
                    .step_by(4)
                    .copied()
                    .collect::<Vec<_>>()
            };
            assert_eq!(alpha(&out), alpha(&px));
            assert_eq!(
                (out.width, out.height, out.has_alpha),
                (px.width, px.height, px.has_alpha)
            );
        }
    }

    #[test]
    fn an_srgb_profile_changes_nothing() {
        let px = pixels(&lattice(), 255);
        let srgb = icc::SRGB.bytes();
        assert_eq!(to_srgb(px.clone(), Some(&srgb)), px);
        assert!(is_srgb(Some(&srgb)));
    }

    #[test]
    fn a_picture_larger_than_one_strip_is_converted_throughout() {
        // 300 × 300 is several conversion strips.
        let c = [200, 100, 50];
        let n = 300 * 300;
        let px = Pixels {
            width: 300,
            height: 300,
            full_width: 300,
            full_height: 300,
            rgba: [c[0], c[1], c[2], 255].repeat(n),
            has_alpha: false,
        };
        let out = to_srgb(px, Some(&icc::DISPLAY_P3.bytes()));
        let first = out.pixel(0, 0);
        assert_ne!(&first[..3], &c);
        assert!(out.rgba.as_chunks::<4>().0.iter().all(|p| *p == first));
    }

    fn broken_profiles() -> Vec<(String, Vec<u8>)> {
        let good = icc::DISPLAY_P3.bytes();
        let mut v = vec![
            ("empty".to_string(), Vec::new()),
            (
                "random bytes".to_string(),
                (0..3000u32)
                    .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
                    .collect(),
            ),
        ];
        for len in 0..good.len() {
            v.push((format!("cut to {len} bytes"), good[..len].to_vec()));
        }
        for i in 0..good.len() {
            let mut b = good.clone();
            b[i] ^= 0xA5;
            v.push((format!("byte {i} flipped"), b));
        }
        v
    }

    #[test]
    fn a_broken_profile_never_panics_and_keeps_the_picture() {
        let px = pixels(&lattice(), 128);
        for (what, icc) in broken_profiles() {
            let out = to_srgb(px.clone(), Some(&icc));
            assert_eq!(out.rgba.len(), px.rgba.len(), "{what}");
            assert_eq!((out.width, out.height), (px.width, px.height), "{what}");
            // Whatever a damaged profile does to colour, alpha stays.
            for (a, b) in out
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .zip(px.rgba.as_chunks::<4>().0)
            {
                assert_eq!(a[3], b[3], "{what}");
            }
            let _ = is_srgb(Some(&icc));
        }
    }

    #[test]
    fn an_unusable_profile_leaves_pixels_alone() {
        let good = icc::DISPLAY_P3.bytes();
        let mut cut = good[..good.len() / 2].to_vec();
        let len = u32::try_from(cut.len()).unwrap();
        cut[..4].copy_from_slice(&len.to_be_bytes());
        let px = pixels(&lattice(), 255);
        for icc in [
            Vec::new(),
            b"not a profile".to_vec(),
            good[..100].to_vec(),
            cut,
        ] {
            assert_eq!(to_srgb(px.clone(), Some(&icc)), px);
            assert!(is_srgb(Some(&icc)));
        }
    }

    #[test]
    fn a_profile_for_another_colour_space_leaves_pixels_alone() {
        // A CMYK JPEG reaches `to_srgb` already turned into RGB by the
        // decoder, still carrying its CMYK profile: applying it would be
        // wrong, so the pixels are kept.
        let px = pixels(&lattice(), 255);
        for space in [b"CMYK", b"Lab ", b"XYZ "] {
            let mut p = icc::DISPLAY_P3.bytes();
            p[16..20].copy_from_slice(space);
            assert_eq!(to_srgb(px.clone(), Some(&p)), px);
        }
    }
}
