//! `preview_reference`: a small interpreter of the page's drawing rules
//! (docs/06-editing-and-file-actions.md, "How the page draws a plan"),
//! used by the agreement tests 2 and 3 ("The test that pins agreement").
//!
//! It draws a plan as the browser does: the thumbnail scaled to the
//! `<img>` box, moved by the same CSS transform as `EditedImage` with
//! bilinear sampling, clipped by the outer box, then the colour matrix on
//! the 8-bit sRGB values (`color-interpolation-filters="sRGB"`).

use super::plan::{EditPlan, apply_colour};
use crate::media::Pixels;

/// Draws `plan` from `thumbnail` (upright, any size with the photo's
/// shape) into a box of `box_w × box_h` CSS pixels, contained. Answers the
/// outer element's pixels, rounded to whole pixels.
pub fn preview_reference(plan: &EditPlan, thumbnail: &Pixels, box_w: f64, box_h: f64) -> Pixels {
    let ow = f64::from(plan.out.w);
    let oh = f64::from(plan.out.h);
    let f = (box_w / ow).min(box_h / oh);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (w, h) = (
        ((ow * f).round() as u32).max(1),
        ((oh * f).round() as u32).max(1),
    );
    let img_w = f64::from(plan.pic.w) * f;
    let img_h = f64::from(plan.pic.h) * f;
    let tx = (f64::from(plan.frame.w) / 2.0 - f64::from(plan.crop.left)) * f;
    let ty = (f64::from(plan.frame.h) / 2.0 - f64::from(plan.crop.top)) * f;
    let (sin, cos) = plan.angle.to_radians().sin_cos();
    // Thumbnail pixels per `<img>` CSS pixel.
    let sx = f64::from(thumbnail.width) / img_w;
    let sy = f64::from(thumbnail.height) / img_h;
    let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
    for y in 0..h {
        for x in 0..w {
            // Undo translate(tx, ty), then rotate(angle), then scaleX(-1),
            // then translate(-img_w / 2, -img_h / 2).
            let dx = f64::from(x) + 0.5 - tx;
            let dy = f64::from(y) + 0.5 - ty;
            let mut px = dx * cos + dy * sin;
            let py = -dx * sin + dy * cos;
            if plan.flip {
                px = -px;
            }
            let (ix, iy) = (px + img_w / 2.0, py + img_h / 2.0);
            let v = bilinear(thumbnail, ix * sx - 0.5, iy * sy - 0.5);
            rgba.extend(v.map(to_u8));
        }
    }
    if let Some(cm) = &plan.colour {
        for p in rgba.as_chunks_mut::<4>().0.iter_mut() {
            let out = apply_colour(cm, [p[0], p[1], p[2]].map(|v| f64::from(v) / 255.0));
            for i in 0..3 {
                p[i] = to_u8(out[i] * 255.0);
            }
        }
    }
    Pixels {
        width: w,
        height: h,
        full_width: w,
        full_height: h,
        rgba,
        has_alpha: thumbnail.has_alpha,
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn to_u8(v: f64) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

/// Bilinear sample at pixel-centre coordinates; the edge repeats.
fn bilinear(img: &Pixels, x: f64, y: f64) -> [f64; 4] {
    let max_x = f64::from(img.width - 1);
    let max_y = f64::from(img.height - 1);
    let x = x.clamp(0.0, max_x);
    let y = y.clamp(0.0, max_y);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (x0, y0) = (x0 as u32, y0 as u32);
    let x1 = (x0 + 1).min(img.width - 1);
    let y1 = (y0 + 1).min(img.height - 1);
    let mut out = [0.0; 4];
    for (px, py, wt) in [
        (x0, y0, (1.0 - fx) * (1.0 - fy)),
        (x1, y0, fx * (1.0 - fy)),
        (x0, y1, (1.0 - fx) * fy),
        (x1, y1, fx * fy),
    ] {
        let p = img.pixel(px, py);
        for c in 0..4 {
            out[c] += wt * f64::from(p[c]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::fs;

    use image::{RgbImage, imageops};
    use owlbum_fixtures::picture;

    use super::*;
    use crate::edit::fixtures::cases;
    use crate::edit::model::{Colour, IDENTITY, PhotoEdit};
    use crate::edit::plan::edit_plan;
    use crate::edit::render::{encode, render_edit, render_pixels};
    use crate::media::{SaveFormat, format_of};

    fn pixels(img: &RgbImage) -> Pixels {
        let rgba = image::DynamicImage::ImageRgb8(img.clone()).to_rgba8();
        let (width, height) = rgba.dimensions();
        Pixels {
            width,
            height,
            full_width: width,
            full_height: height,
            rgba: rgba.into_raw(),
            has_alpha: false,
        }
    }

    /// Mean absolute difference per channel, and the share of pixels with a
    /// channel more than 12 levels apart.
    fn compare(a: &Pixels, b: &Pixels) -> (f64, f64) {
        assert_eq!((a.width, a.height), (b.width, b.height));
        let mut sum = 0u64;
        let mut far = 0usize;
        for (p, q) in a
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .zip(b.rgba.as_chunks::<4>().0)
        {
            let d: Vec<u8> = (0..3).map(|i| p[i].abs_diff(q[i])).collect();
            sum += d.iter().map(|&v| u64::from(v)).sum::<u64>();
            far += usize::from(d.iter().any(|&v| v > 12));
        }
        let n = a.rgba.len() / 4;
        #[allow(clippy::cast_precision_loss)]
        (sum as f64 / (n * 3) as f64, far as f64 / n as f64)
    }

    /// Test 2: the renderer at full size, scaled down to the box, agrees
    /// with `preview_reference` on the medium thumbnail.
    #[test]
    fn the_renderer_agrees_with_the_reference_preview() {
        for (name, edit, (w, h)) in cases() {
            // A quarter of the fixture size keeps the test quick; the
            // medium thumbnail keeps its share of the photo (1600 of 3000).
            let (w, h) = if w > 1000 { (w / 4, h / 4) } else { (w, h) };
            let photo = picture::blur(&picture::scene(7, w, h), 6);
            let long = w.max(h);
            let side = if long > 600 { long * 1600 / 3000 } else { long };
            let thumb = imageops::thumbnail(&photo, w * side / long, h * side / long);
            let (_, full) = render_pixels(&pixels(&photo), &edit);
            for (bw, bh) in [(200.0, 200.0), (640.0, 360.0)] {
                let plan = edit_plan(w, h, &edit, false);
                let reference = preview_reference(&plan, &pixels(&thumb), bw, bh);
                let full_img =
                    image::RgbaImage::from_raw(full.width, full.height, full.rgba.clone()).unwrap();
                let scaled = imageops::resize(
                    &full_img,
                    reference.width,
                    reference.height,
                    imageops::FilterType::Triangle,
                );
                let scaled = Pixels {
                    width: reference.width,
                    height: reference.height,
                    full_width: reference.width,
                    full_height: reference.height,
                    rgba: scaled.into_raw(),
                    has_alpha: false,
                };
                let (mean, far) = compare(&scaled, &reference);
                assert!(
                    mean <= 2.0 && far <= 0.01,
                    "{name} in {bw}×{bh}: mean {mean:.2}, {:.2}% far",
                    far * 100.0
                );
            }
        }
    }

    /// Test 3: on a flat colour each fixture colour edit lands within 1
    /// level of `apply` for PNG and within 3 for JPEG.
    #[test]
    fn colour_edits_are_exact_on_a_flat_colour() {
        let base = [180u8, 120, 60];
        let flat = RgbImage::from_pixel(32, 24, image::Rgb(base));
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("flat.png");
        fs::write(&src, encode(&pixels(&flat), SaveFormat::Png).unwrap()).unwrap();
        let format = format_of("flat.png").unwrap();
        let mut edits: Vec<(String, PhotoEdit)> = cases()
            .into_iter()
            .filter(|(_, e, _)| e.colour != Colour::NONE)
            .map(|(n, e, _)| (n.to_owned(), e))
            .collect();
        edits.push((
            "darker".into(),
            PhotoEdit {
                colour: Colour {
                    brightness: -40.0,
                    contrast: -20.0,
                    saturation: 0.0,
                    warmth: 25.0,
                },
                ..IDENTITY
            },
        ));
        for (name, edit) in edits {
            let cm = edit_plan(32, 24, &edit, false)
                .colour
                .expect("a colour edit");
            let want = apply_colour(&cm, base.map(|v| f64::from(v) / 255.0)).map(|v| v * 255.0);
            for (target, tolerance) in [(SaveFormat::Png, 1.0), (SaveFormat::Jpeg, 3.0)] {
                let out = tmp.path().join("out");
                render_edit(&src, format, &edit, target, &out).unwrap();
                let img = image::load_from_memory(&fs::read(&out).unwrap())
                    .unwrap()
                    .to_rgb8();
                let (cx, cy) = (img.width() / 2, img.height() / 2);
                let got = img.get_pixel(cx, cy).0;
                for i in 0..3 {
                    assert!(
                        (f64::from(got[i]) - want[i]).abs() <= tolerance,
                        "{name} {target:?}: {got:?} against {want:?}"
                    );
                }
            }
        }
    }
}
