//! The renderer: turns an edit into pixels and an edited file
//! (docs/06-editing-and-file-actions.md, "Rendering").
//!
//! [`render_pixels`] follows the plan exactly: flip and quarter turn as
//! pixel moves, then straighten and crop together with Catmull-Rom
//! sampling of only the cropped pixels, then the colour matrix on the
//! sRGB-encoded values. [`render_edit`] decodes a file through the format
//! registry, renders it and writes the result through a temporary file.
//!
//! Version-1 gaps recorded on #43 and left to the saving work (#47): the
//! renderer works on the handlers' 8-bit sRGB `working` picture, so it
//! does not yet keep 16-bit samples or a wide-gamut profile, and it writes
//! no metadata (EXIF, XMP).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use image::ImageEncoder;
use image::codecs::png::PngEncoder;
use image::codecs::webp::WebPEncoder;

use super::model::PhotoEdit;
use super::plan::{EditPlan, apply_colour, edit_plan};
use crate::error::{AppError, AppResult, ErrorCode};
use crate::media::{MediaFormat, Pixels, SaveFormat};
use crate::text;

/// JPEG quality for an edited file.
pub const JPEG_QUALITY: u8 = 95;

/// An RGBA picture being rendered.
struct Rgba {
    w: u32,
    h: u32,
    px: Vec<u8>,
}

impl Rgba {
    fn at(&self, x: u32, y: u32) -> &[u8] {
        let i = (y as usize * self.w as usize + x as usize) * 4;
        &self.px[i..i + 4]
    }
}

/// Mirror (when asked) and turn by quarter turns: exact pixel moves.
fn flip_and_turn(src: &Pixels, flip: bool, rotate: u16) -> Rgba {
    let (w, h) = (src.width, src.height);
    let (tw, th) = if rotate == 90 || rotate == 270 {
        (h, w)
    } else {
        (w, h)
    };
    let mut px = vec![0u8; tw as usize * th as usize * 4];
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
            px[d..d + 4].copy_from_slice(&src.rgba[s..s + 4]);
        }
    }
    Rgba { w: tw, h: th, px }
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
fn sample(img: &Rgba, x: f64, y: f64) -> [f64; 4] {
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
                out[c] += w * f64::from(p[c]);
            }
        }
    }
    out
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn to_u8(v: f64) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

/// Renders `edit` on an upright 8-bit picture. The output is the plan's
/// `out` size.
pub fn render_pixels(src: &Pixels, edit: &PhotoEdit) -> (EditPlan, Pixels) {
    let plan = edit_plan(src.width, src.height, edit, false);
    let turned = flip_and_turn(src, edit.flip, edit.rotate);
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
                px.extend(v.map(to_u8));
            }
        }
    }
    if let Some(cm) = &plan.colour {
        for p in px.as_chunks_mut::<4>().0.iter_mut() {
            let out = apply_colour(cm, [p[0], p[1], p[2]].map(|v| f64::from(v) / 255.0));
            for i in 0..3 {
                p[i] = to_u8(out[i] * 255.0);
            }
        }
    }
    let out = Pixels {
        width: c.width,
        height: c.height,
        full_width: c.width,
        full_height: c.height,
        rgba: px,
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

/// Encodes rendered pixels in `format` (docs/06, "Output"): JPEG at
/// quality 95 with 4:4:4 chroma and transparency flattened onto white; PNG
/// and lossless WebP keep transparency.
pub fn encode(px: &Pixels, format: SaveFormat) -> AppResult<Vec<u8>> {
    let mut out = Vec::new();
    match format {
        SaveFormat::Jpeg => {
            let rgb: Vec<u8> = px
                .rgba
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
            let (w, h) = (
                u16::try_from(px.width).map_err(|_| failed(text::edit::TOO_WIDE_FOR_JPEG))?,
                u16::try_from(px.height).map_err(|_| failed(text::edit::TOO_WIDE_FOR_JPEG))?,
            );
            let mut e = jpeg_encoder::Encoder::new(&mut out, JPEG_QUALITY);
            e.set_sampling_factor(jpeg_encoder::SamplingFactor::R_4_4_4);
            e.encode(&rgb, w, h, jpeg_encoder::ColorType::Rgb)
                .map_err(failed)?;
        }
        SaveFormat::Png => {
            let e = PngEncoder::new(&mut out);
            if px.has_alpha {
                e.write_image(
                    &px.rgba,
                    px.width,
                    px.height,
                    image::ExtendedColorType::Rgba8,
                )
            } else {
                let rgb: Vec<u8> = px
                    .rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|p| [p[0], p[1], p[2]])
                    .collect();
                e.write_image(&rgb, px.width, px.height, image::ExtendedColorType::Rgb8)
            }
            .map_err(failed)?;
        }
        SaveFormat::Webp => {
            WebPEncoder::new_lossless(&mut out)
                .write_image(
                    &px.rgba,
                    px.width,
                    px.height,
                    image::ExtendedColorType::Rgba8,
                )
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

/// `render_edit(source, edit, target_format, out_path)`: decodes `source`
/// at full size through its format's handler, renders `edit`, encodes it
/// as `target` and writes `out_path`. Answers the written width and
/// height. Runs on the blocking pool; the page never calls it directly.
pub fn render_edit(
    source: &Path,
    format: &MediaFormat,
    edit: &PhotoEdit,
    target: SaveFormat,
    out_path: &Path,
) -> AppResult<(u32, u32)> {
    let picture = format.handler.working(source).map_err(|e| {
        if e.code == crate::media::MediaErrorCode::TooLarge {
            AppError::new(ErrorCode::EditRenderFailed, text::edit::TOO_LARGE)
        } else {
            failed(e.message)
        }
    })?;
    let (_, px) = render_pixels(&picture, edit);
    let bytes = encode(&px, target)?;
    write_through_temp(out_path, &bytes).map_err(failed)?;
    Ok((px.width, px.height))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::model::{Colour, CropRect, IDENTITY};
    use crate::media::format_of;

    fn picture(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Pixels {
        let mut rgba = Vec::new();
        for y in 0..h {
            for x in 0..w {
                rgba.extend(f(x, y));
            }
        }
        Pixels {
            width: w,
            height: h,
            full_width: w,
            full_height: h,
            rgba,
            has_alpha: false,
        }
    }

    const RED: [u8; 4] = [255, 0, 0, 255];
    const BLUE: [u8; 4] = [0, 0, 255, 255];
    const GREEN: [u8; 4] = [0, 255, 0, 255];
    const WHITE: [u8; 4] = [255, 255, 255, 255];

    #[test]
    fn a_flip_puts_red_on_the_right() {
        let src = picture(4, 2, |x, _| if x < 2 { RED } else { BLUE });
        let (_, out) = render_pixels(
            &src,
            &PhotoEdit {
                flip: true,
                ..IDENTITY
            },
        );
        assert_eq!(out.pixel(3, 0), RED);
        assert_eq!(out.pixel(0, 1), BLUE);
    }

    #[test]
    fn the_marked_corner_follows_each_quarter_turn() {
        // The top-left pixel is marked.
        let src = picture(4, 2, |x, y| if (x, y) == (0, 0) { RED } else { WHITE });
        for (rotate, size, at) in [
            (0, (4, 2), (0, 0)),
            (90, (2, 4), (1, 0)),
            (180, (4, 2), (3, 1)),
            (270, (2, 4), (0, 3)),
        ] {
            let (_, out) = render_pixels(&src, &PhotoEdit { rotate, ..IDENTITY });
            assert_eq!((out.width, out.height), size, "{rotate}");
            assert_eq!(out.pixel(at.0, at.1), RED, "{rotate}");
        }
    }

    #[test]
    fn a_quadrant_crop_is_one_colour() {
        let src = picture(400, 200, |x, y| match (x < 200, y < 100) {
            (true, true) => RED,
            (false, true) => GREEN,
            (true, false) => BLUE,
            (false, false) => WHITE,
        });
        let e = PhotoEdit {
            crop: CropRect {
                x: 0.5,
                y: 0.0,
                w: 0.5,
                h: 0.5,
            },
            ..IDENTITY
        };
        let (_, out) = render_pixels(&src, &e);
        assert_eq!((out.width, out.height), (200, 100));
        assert!(out.rgba.as_chunks::<4>().0.iter().all(|p| *p == GREEN));
    }

    #[test]
    fn a_turned_crop_keeps_the_same_part_of_the_picture() {
        // The top-left quadrant is red; crop it, then turn and flip the edit.
        let src = picture(40, 20, |x, y| if x < 20 && y < 10 { RED } else { WHITE });
        let e = PhotoEdit {
            crop: CropRect {
                x: 0.0,
                y: 0.0,
                w: 0.5,
                h: 0.5,
            },
            ..IDENTITY
        };
        for op in [
            crate::edit::model::rotate_left,
            crate::edit::model::rotate_right,
            crate::edit::model::flip,
        ] {
            let (_, out) = render_pixels(&src, &op(&e));
            assert!(out.rgba.as_chunks::<4>().0.iter().all(|p| *p == RED));
        }
    }

    #[test]
    fn straightening_never_samples_outside_the_picture() {
        // A solid red picture: any empty corner would show as black.
        let src = picture(60, 40, |_, _| RED);
        let (plan, out) = render_pixels(
            &src,
            &PhotoEdit {
                straighten: 10.0,
                ..IDENTITY
            },
        );
        assert!(plan.frame.w < 60 && plan.frame.h < 40);
        assert!(out.rgba.as_chunks::<4>().0.iter().all(|p| *p == RED));
    }

    #[test]
    fn colour_on_pixels() {
        let grey = picture(8, 8, |_, _| [128, 128, 128, 255]);
        let e = PhotoEdit {
            colour: Colour {
                brightness: 50.0,
                ..Colour::NONE
            },
            ..IDENTITY
        };
        let (_, out) = render_pixels(&grey, &e);
        assert_eq!(out.pixel(0, 0), [179, 179, 179, 255], "+51 levels");
        let red = picture(2, 2, |_, _| RED);
        let e = PhotoEdit {
            colour: Colour {
                saturation: -100.0,
                ..Colour::NONE
            },
            ..IDENTITY
        };
        let p = render_pixels(&red, &e).1.pixel(0, 0);
        assert!(p[0] == p[1] && p[1] == p[2], "{p:?}");
    }

    #[test]
    fn render_edit_writes_a_file_through_a_temporary() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("in.png");
        let px = picture(40, 20, |x, _| if x < 20 { RED } else { BLUE });
        fs::write(&src, encode(&px, SaveFormat::Png).unwrap()).unwrap();
        let out = tmp.path().join("out.jpg");
        let format = format_of("in.png").unwrap();
        let e = PhotoEdit {
            rotate: 90,
            ..IDENTITY
        };
        assert_eq!(
            render_edit(&src, format, &e, SaveFormat::Jpeg, &out).unwrap(),
            (20, 40)
        );
        let back = image::open(&out).unwrap().to_rgb8();
        assert_eq!(back.dimensions(), (20, 40));
        let files: Vec<_> = fs::read_dir(tmp.path()).unwrap().collect();
        assert_eq!(files.len(), 2, "no temporary left behind");
        let png_out = tmp.path().join("out.png");
        render_edit(&src, format, &e, SaveFormat::Png, &png_out).unwrap();
        assert_eq!(
            image::open(&png_out).unwrap().to_rgb8().get_pixel(0, 0).0,
            [255, 0, 0]
        );
    }

    #[test]
    fn transparency_is_flattened_onto_white_for_jpeg() {
        let mut px = picture(8, 8, |_, _| [0, 0, 0, 0]);
        px.has_alpha = true;
        let jpeg = encode(&px, SaveFormat::Jpeg).unwrap();
        let back = image::load_from_memory(&jpeg).unwrap().to_rgb8();
        assert!(back.pixels().all(|p| p.0.iter().all(|v| *v > 250)));
    }
}
