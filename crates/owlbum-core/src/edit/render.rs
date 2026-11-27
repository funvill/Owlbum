//! The renderer: turns an edit into pixels and an edited file
//! (docs/06-editing-and-file-actions.md, "Rendering").
//!
//! [`render_pixels`] follows the plan exactly: flip and quarter turn as
//! pixel moves, then straighten and crop together with Catmull-Rom
//! sampling of only the cropped pixels, then the colour matrix on the
//! sRGB-encoded values. [`render_edit`] decodes a file through its
//! handler's `source` call (the file's own bit depth and colour profile),
//! renders it in 8 or 16 bits, and writes the result with the original's
//! metadata (see [`super::metadata`]) through a temporary file.

use std::borrow::Cow;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use image::ImageEncoder;
use image::codecs::png::PngEncoder;

use super::metadata::{self, Changes, JPEG_EXIF_MAX, Kept};
use super::model::PhotoEdit;
use super::plan::{EditPlan, apply_colour, edit_plan};
use crate::error::{AppError, AppResult, ErrorCode};
use crate::media::colour;
use crate::media::{Layout, MediaFormat, Pixels, Samples, SaveFormat, SourcePixels};
use crate::text;

/// JPEG quality for an edited file.
pub const JPEG_QUALITY: u8 = 95;

/// A sample the renderer works in: 8 or 16 bits.
pub trait Sample: Copy + Default + PartialEq + std::fmt::Debug + Send + Sync {
    /// The largest value: white, or opaque.
    const MAX: f64;
    fn to_f64(self) -> f64;
    /// Rounded and clamped to the sample's range.
    fn from_f64(v: f64) -> Self;
}

impl Sample for u8 {
    const MAX: f64 = 255.0;
    fn to_f64(self) -> f64 {
        f64::from(self)
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn from_f64(v: f64) -> Self {
        v.round().clamp(0.0, 255.0) as u8
    }
}

impl Sample for u16 {
    const MAX: f64 = 65535.0;
    fn to_f64(self) -> f64 {
        f64::from(self)
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn from_f64(v: f64) -> Self {
        v.round().clamp(0.0, 65535.0) as u16
    }
}

/// An RGBA picture being rendered.
struct Rgba<'a, T: Clone> {
    w: u32,
    h: u32,
    px: Cow<'a, [T]>,
}

impl<T: Sample> Rgba<'_, T> {
    fn at(&self, x: u32, y: u32) -> &[T] {
        let i = (y as usize * self.w as usize + x as usize) * 4;
        &self.px[i..i + 4]
    }
}

/// Mirror (when asked) and turn by quarter turns: exact pixel moves. With
/// neither, the picture is borrowed as it is.
fn flip_and_turn<T: Sample>(w: u32, h: u32, src: &[T], flip: bool, rotate: u16) -> Rgba<'_, T> {
    if !flip && rotate == 0 {
        return Rgba {
            w,
            h,
            px: Cow::Borrowed(src),
        };
    }
    let (tw, th) = if rotate == 90 || rotate == 270 {
        (h, w)
    } else {
        (w, h)
    };
    let mut px = vec![T::default(); tw as usize * th as usize * 4];
    for ty in 0..th {
        for tx in 0..tw {
            // The pixel of the flipped picture that lands at (tx, ty).
            let (x, y) = match rotate {
                90 => (ty, h - 1 - tx),
                180 => (w - 1 - tx, h - 1 - ty),
                270 => (w - 1 - ty, tx),
                _ => (tx, ty),
            };
            let x = if flip { w - 1 - x } else { x };
            let s = (y as usize * w as usize + x as usize) * 4;
            let d = (ty as usize * tw as usize + tx as usize) * 4;
            px[d..d + 4].copy_from_slice(&src[s..s + 4]);
        }
    }
    Rgba {
        w: tw,
        h: th,
        px: Cow::Owned(px),
    }
}

/// Catmull-Rom weights for the four taps around a sample at fraction `t`.
fn catmull_rom(t: f64) -> [f64; 4] {
    let t2 = t * t;
    let t3 = t2 * t;
    [
        0.5 * (-t3 + 2.0 * t2 - t),
        0.5 * (3.0 * t3 - 5.0 * t2 + 2.0),
        0.5 * (-3.0 * t3 + 4.0 * t2 + t),
        0.5 * (t3 - t2),
    ]
}

/// Bicubic sample of `img` at pixel-centre coordinates (`x`, `y`); taps
/// past the edge repeat the edge.
fn sample<T: Sample>(img: &Rgba<'_, T>, x: f64, y: f64) -> [f64; 4] {
    let x0 = x.floor();
    let y0 = y.floor();
    let wx = catmull_rom(x - x0);
    let wy = catmull_rom(y - y0);
    let max_x = f64::from(img.w - 1);
    let max_y = f64::from(img.h - 1);
    let mut out = [0.0; 4];
    for (j, wyj) in wy.iter().enumerate() {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let sy = (y0 + j as f64 - 1.0).clamp(0.0, max_y) as u32;
        for (i, wxi) in wx.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let sx = (x0 + i as f64 - 1.0).clamp(0.0, max_x) as u32;
            let p = img.at(sx, sy);
            let w = wxi * wyj;
            for c in 0..4 {
                out[c] += w * p[c].to_f64();
            }
        }
    }
    out
}

/// Renders `edit` on an upright RGBA picture of `w × h` pixels. Answers
/// the plan and the plan's `out` size of RGBA samples.
pub fn render_rgba<T: Sample>(w: u32, h: u32, src: &[T], edit: &PhotoEdit) -> (EditPlan, Vec<T>) {
    let plan = edit_plan(w, h, edit, false);
    let turned = flip_and_turn(w, h, src, edit.flip, edit.rotate);
    let c = plan.crop;
    let mut px = Vec::with_capacity(c.width as usize * c.height as usize * 4);
    if edit.straighten == 0.0 {
        // The frame is the turned picture: copy the crop unchanged.
        for y in c.top..c.top + c.height {
            let s = (y as usize * turned.w as usize + c.left as usize) * 4;
            px.extend_from_slice(&turned.px[s..s + c.width as usize * 4]);
        }
    } else {
        let (sin, cos) = edit.straighten.to_radians().sin_cos();
        let fcx = f64::from(plan.frame.w) / 2.0;
        let fcy = f64::from(plan.frame.h) / 2.0;
        let tcx = f64::from(turned.w) / 2.0;
        let tcy = f64::from(turned.h) / 2.0;
        for y in c.top..c.top + c.height {
            for x in c.left..c.left + c.width {
                let dx = f64::from(x) + 0.5 - fcx;
                let dy = f64::from(y) + 0.5 - fcy;
                // Undo a clockwise turn (y points down).
                let sx = dx * cos + dy * sin;
                let sy = -dx * sin + dy * cos;
                let v = sample(&turned, tcx + sx - 0.5, tcy + sy - 0.5);
                px.extend(v.map(T::from_f64));
            }
        }
    }
    drop(turned);
    if let Some(cm) = &plan.colour {
        for p in px.as_chunks_mut::<4>().0.iter_mut() {
            let out = apply_colour(cm, [p[0], p[1], p[2]].map(|v| v.to_f64() / T::MAX));
            for i in 0..3 {
                p[i] = T::from_f64(out[i] * T::MAX);
            }
        }
    }
    (plan, px)
}

/// Renders `edit` on an upright 8-bit picture. The output is the plan's
/// `out` size.
pub fn render_pixels(src: &Pixels, edit: &PhotoEdit) -> (EditPlan, Pixels) {
    let (plan, rgba) = render_rgba(src.width, src.height, &src.rgba, edit);
    let c = plan.crop;
    let out = Pixels {
        width: c.width,
        height: c.height,
        full_width: c.width,
        full_height: c.height,
        rgba,
        has_alpha: src.has_alpha,
    };
    (plan, out)
}

fn failed(reason: impl std::fmt::Display) -> AppError {
    AppError::new(
        ErrorCode::EditRenderFailed,
        text::edit::render_failed(reason),
    )
}

/// Rendered RGBA samples at 8 or 16 bits.
enum Work {
    U8(Vec<u8>),
    U16(Vec<u16>),
}

/// A rendered picture and what goes into its file besides pixels.
struct Rendered<'a> {
    width: u32,
    height: u32,
    has_alpha: bool,
    px: Work,
    /// The profile to embed: the source's, or sRGB after a colour change.
    icc: Option<Vec<u8>>,
    kept: &'a Kept,
    /// True when the pixels were converted to sRGB for a colour change.
    srgb: bool,
}

/// 16 bits to 8, rounded.
#[allow(clippy::cast_possible_truncation)]
fn narrow(v: u16) -> u8 {
    ((u32::from(v) * 255 + 32767) / 65535) as u8
}

impl Rendered<'_> {
    fn rgba8(&self) -> Cow<'_, [u8]> {
        match &self.px {
            Work::U8(v) => Cow::Borrowed(v),
            Work::U16(v) => Cow::Owned(v.iter().map(|&s| narrow(s)).collect()),
        }
    }

    fn exif(&self, limit: usize) -> Option<Vec<u8>> {
        self.kept.exif_for(
            Changes {
                width: self.width,
                height: self.height,
                srgb: self.srgb,
            },
            limit,
        )
    }
}

/// Encodes rendered pixels in `format` (docs/06, "Output"): JPEG at
/// quality 95 with 4:4:4 chroma and transparency flattened onto white; PNG
/// and lossless WebP keep transparency. No metadata.
pub fn encode(px: &Pixels, format: SaveFormat) -> AppResult<Vec<u8>> {
    encode_rendered(
        &Rendered {
            width: px.width,
            height: px.height,
            has_alpha: px.has_alpha,
            px: Work::U8(px.rgba.clone()),
            icc: None,
            kept: &Kept::default(),
            srgb: false,
        },
        format,
    )
}

/// The JPEG APP1 prefix of an XMP packet.
const XMP_PREFIX: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";

fn encode_rendered(r: &Rendered<'_>, format: SaveFormat) -> AppResult<Vec<u8>> {
    let mut out = Vec::new();
    match format {
        SaveFormat::Jpeg => {
            let rgba = r.rgba8();
            let rgb: Vec<u8> = rgba
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| {
                    let a = u16::from(p[3]);
                    [0, 1, 2].map(|i| {
                        // Over white: c·a + 255·(1 − a).
                        let v = (u16::from(p[i]) * a + 255 * (255 - a) + 127) / 255;
                        u8::try_from(v).unwrap_or(255)
                    })
                })
                .collect();
            drop(rgba);
            let (w, h) = (
                u16::try_from(r.width).map_err(|_| failed(text::edit::TOO_WIDE_FOR_JPEG))?,
                u16::try_from(r.height).map_err(|_| failed(text::edit::TOO_WIDE_FOR_JPEG))?,
            );
            let mut e = jpeg_encoder::Encoder::new(&mut out, JPEG_QUALITY);
            e.set_sampling_factor(jpeg_encoder::SamplingFactor::R_4_4_4);
            if let Some(exif) = r.exif(JPEG_EXIF_MAX) {
                let mut seg = b"Exif\0\0".to_vec();
                seg.extend(exif);
                e.add_app_segment(1, seg).map_err(failed)?;
            }
            if let Some(xmp) = &r.kept.xmp {
                let mut seg = XMP_PREFIX.to_vec();
                seg.extend_from_slice(xmp);
                // A packet too large for one segment (extended XMP) is
                // left out rather than split.
                if seg.len() <= 65533 {
                    e.add_app_segment(1, seg).map_err(failed)?;
                }
            }
            if let Some(icc) = &r.icc {
                e.add_icc_profile(icc).map_err(failed)?;
            }
            e.encode(&rgb, w, h, jpeg_encoder::ColorType::Rgb)
                .map_err(failed)?;
        }
        SaveFormat::Png => {
            let mut e = PngEncoder::new(&mut out);
            if let Some(icc) = &r.icc {
                e.set_icc_profile(icc.clone()).map_err(failed)?;
            }
            if let Some(exif) = r.exif(usize::MAX) {
                e.set_exif_metadata(exif).map_err(failed)?;
            }
            let (w, h) = (r.width, r.height);
            match (&r.px, r.has_alpha) {
                (Work::U8(px), true) => e.write_image(px, w, h, image::ExtendedColorType::Rgba8),
                (Work::U8(px), false) => {
                    let rgb: Vec<u8> = px
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .flat_map(|p| [p[0], p[1], p[2]])
                        .collect();
                    e.write_image(&rgb, w, h, image::ExtendedColorType::Rgb8)
                }
                (Work::U16(px), alpha) => {
                    // Native-endian bytes, as `PngEncoder` wants for 16 bits.
                    let take = if alpha { 4 } else { 3 };
                    let mut bytes = Vec::with_capacity(px.len() / 4 * take * 2);
                    for p in px.as_chunks::<4>().0 {
                        for s in &p[..take] {
                            bytes.extend(s.to_ne_bytes());
                        }
                    }
                    let kind = if alpha {
                        image::ExtendedColorType::Rgba16
                    } else {
                        image::ExtendedColorType::Rgb16
                    };
                    e.write_image(&bytes, w, h, kind)
                }
            }
            .map_err(failed)?;
            if let Some(xmp) = &r.kept.xmp {
                out = metadata::png_with_xmp(out, xmp);
            }
        }
        SaveFormat::Webp => {
            let mut e = image_webp::WebPEncoder::new(&mut out);
            if let Some(icc) = &r.icc {
                e.set_icc_profile(icc.clone());
            }
            if let Some(exif) = r.exif(usize::MAX) {
                e.set_exif_metadata(exif);
            }
            if let Some(xmp) = &r.kept.xmp {
                e.set_xmp_metadata(xmp.clone());
            }
            e.encode(&r.rgba8(), r.width, r.height, image_webp::ColorType::Rgba8)
                .map_err(failed)?;
        }
    }
    Ok(out)
}

fn temp_path(out_path: &Path) -> PathBuf {
    let mut name = out_path.as_os_str().to_owned();
    name.push(format!(".{}.tmp", uuid::Uuid::new_v4().simple()));
    PathBuf::from(name)
}

/// Writes `bytes` to `<out_path>.<uuid>.tmp`, flushes it to disk and
/// renames it onto `out_path`. The temporary file is removed on failure.
fn write_through_temp(out_path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = temp_path(out_path);
    let result = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, out_path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// The samples as RGBA: grey becomes three channels, and alpha is added
/// (opaque) when the source has none.
fn to_rgba<T: Sample>(layout: Layout, samples: Vec<T>, opaque: T) -> Vec<T> {
    match layout {
        Layout::Rgba => samples,
        Layout::Rgb => samples
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], opaque])
            .collect(),
        Layout::Grey => samples.iter().flat_map(|&v| [v, v, v, opaque]).collect(),
        Layout::GreyAlpha => samples
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
    }
}

/// Renders a decoded source by the colour-profile rules of docs/06
/// ("Colour profiles"), at the source's own depth.
fn render_source<'k>(src: SourcePixels, edit: &PhotoEdit, kept: &'k Kept) -> Rendered<'k> {
    let colour_change = edit_plan(src.width, src.height, edit, false)
        .colour
        .is_some();
    let icc = src.icc;
    // With no colour change an RGB profile is kept and the pixels stay in
    // it. Otherwise (a colour change, or a grey or other profile that
    // cannot describe RGB output) the pixels are converted to sRGB.
    let keep_profile = !colour_change && icc.as_deref().is_some_and(colour::is_rgb_profile);
    let convert = if keep_profile { None } else { icc.as_deref() };
    let has_alpha = src.layout.has_alpha();
    let (w, h) = (src.width, src.height);
    let (plan, px) = match src.samples {
        Samples::U8(s) => {
            let mut rgba = to_rgba(src.layout, s, u8::MAX);
            if convert.is_some() {
                let px = Pixels {
                    width: w,
                    height: h,
                    full_width: w,
                    full_height: h,
                    rgba,
                    has_alpha,
                };
                rgba = colour::to_srgb(px, convert).rgba;
            }
            let (plan, out) = render_rgba(w, h, &rgba, edit);
            (plan, Work::U8(out))
        }
        Samples::U16(s) => {
            let mut rgba = to_rgba(src.layout, s, u16::MAX);
            colour::to_srgb_16(&mut rgba, convert);
            let (plan, out) = render_rgba(w, h, &rgba, edit);
            (plan, Work::U16(out))
        }
    };
    let icc = if colour_change {
        Some(colour::srgb_profile())
    } else if keep_profile {
        icc
    } else {
        None
    };
    Rendered {
        width: plan.crop.width,
        height: plan.crop.height,
        has_alpha,
        px,
        icc,
        kept,
        srgb: colour_change,
    }
}

/// `render_edit(source, edit, target_format, out_path)`: decodes `source`
/// at full size and its own depth through its format's handler, renders
/// `edit`, encodes it as `target` with the original's metadata and writes
/// `out_path`. Answers the written width and height. Runs on the blocking
/// pool; the page never calls it directly.
pub fn render_edit(
    source: &Path,
    format: &MediaFormat,
    edit: &PhotoEdit,
    target: SaveFormat,
    out_path: &Path,
) -> AppResult<(u32, u32)> {
    let picture = format.handler.source(source).map_err(|e| {
        if e.code == crate::media::MediaErrorCode::TooLarge {
            AppError::new(ErrorCode::EditRenderFailed, text::edit::TOO_LARGE)
        } else {
            failed(e.message)
        }
    })?;
    let kept = metadata::read(source);
    let rendered = render_source(picture, edit, &kept);
    let size = (rendered.width, rendered.height);
    let bytes = encode_rendered(&rendered, target)?;
    drop(rendered);
    write_through_temp(out_path, &bytes).map_err(failed)?;
    Ok(size)
}

#[cfg(test)]
mod tests;
