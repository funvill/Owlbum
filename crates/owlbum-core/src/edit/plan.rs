//! Geometry and colour: `edit_plan`, `colour_matrix`, `auto_colour`, and
//! the drawing rules the page's `EditedImage` follows
//! (docs/06-editing-and-file-actions.md, "Geometry", "Colour", "Auto",
//! "How the page draws a plan").
//!
//! Every number that turns an edit into pixels is computed here. The page
//! draws with the plan Rust gives it; the renderer follows the same plan.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::model::{Colour, PhotoEdit, edit_key, round4};

/// A size in whole pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PlanSize {
    pub w: u32,
    pub h: u32,
}

/// A rectangle in whole pixels inside the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PlanCrop {
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
}

/// Light and colour as one 3×3 matrix plus an offset, on sRGB-encoded
/// values from 0 to 1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ColourMatrix {
    /// Nine numbers, row-major.
    pub m: Vec<f64>,
    /// Three numbers, one per channel.
    pub offset: Vec<f64>,
    /// The 20 values of an SVG `feColorMatrix` of type `matrix`, each
    /// rounded to six decimal places.
    pub svg: Vec<f64>,
}

/// How to draw one edit of one photo, in whole pixels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct EditPlan {
    /// `edit_key` of the edit: equal edits share it.
    pub key: String,
    /// The upright picture's size.
    pub pic: PlanSize,
    /// Mirror left to right first.
    pub flip: bool,
    /// Degrees clockwise: the quarter turn plus the straighten angle.
    pub angle: f64,
    /// The frame, centred on the turned picture's centre.
    pub frame: PlanSize,
    /// The crop inside the frame.
    pub crop: PlanCrop,
    /// The output's size: the crop's.
    pub out: PlanSize,
    /// None when light and colour change nothing.
    pub colour: Option<ColourMatrix>,
}

/// The picture's size after a quarter turn.
pub fn turned_size(w: u32, h: u32, rotate: u16) -> (u32, u32) {
    if rotate == 90 || rotate == 270 {
        (h, w)
    } else {
        (w, h)
    }
}

/// How much the turned picture shrinks to give the frame: the largest
/// centred rectangle of the same shape inside the picture turned by `deg`.
pub fn straighten_scale(w: f64, h: f64, deg: f64) -> f64 {
    let a = deg.abs().to_radians();
    let (s, c) = (a.sin(), a.cos());
    let k = (w / (w * c + h * s)).min(h / (w * s + h * c));
    k.min(1.0)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn round_px(v: f64) -> i64 {
    v.round() as i64
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn to_u32(v: i64) -> u32 {
    u32::try_from(v.max(0)).unwrap_or(u32::MAX)
}

/// The plan of `e` for an upright photo of `w × h`. With `whole` the crop
/// is ignored, so the plan shows the whole frame (edit mode's stage).
pub fn edit_plan(w: u32, h: u32, e: &PhotoEdit, whole: bool) -> EditPlan {
    let (tw, th) = turned_size(w.max(1), h.max(1), e.rotate);
    let k = straighten_scale(f64::from(tw), f64::from(th), e.straighten);
    let fw = round_px(f64::from(tw) * k).max(1);
    let fh = round_px(f64::from(th) * k).max(1);
    let (left, top, width, height) = if whole {
        (0, 0, fw, fh)
    } else {
        let c = e.crop;
        let left = round_px(c.x * fw as f64).clamp(0, fw - 1);
        let top = round_px(c.y * fh as f64).clamp(0, fh - 1);
        let width = round_px(c.w * fw as f64).clamp(1, fw - left);
        let height = round_px(c.h * fh as f64).clamp(1, fh - top);
        (left, top, width, height)
    };
    EditPlan {
        key: edit_key(e),
        pic: PlanSize { w, h },
        flip: e.flip,
        angle: round4(f64::from(e.rotate) + e.straighten),
        frame: PlanSize {
            w: to_u32(fw),
            h: to_u32(fh),
        },
        crop: PlanCrop {
            left: to_u32(left),
            top: to_u32(top),
            width: to_u32(width),
            height: to_u32(height),
        },
        out: PlanSize {
            w: to_u32(width),
            h: to_u32(height),
        },
        colour: (!e.colour.is_none()).then(|| colour_matrix(&e.colour)),
    }
}

/// Rec. 709 luma weights.
const LUMA: [f64; 3] = [0.2126, 0.7152, 0.0722];

fn tidy(v: f64) -> f64 {
    if v.abs() < 1e-12 { 0.0 } else { v }
}

fn round6(v: f64) -> f64 {
    let r = (v * 1e6).round() / 1e6;
    if r == 0.0 { 0.0 } else { r }
}

/// The four sliders as one matrix and offset, in the order brightness,
/// contrast, saturation, warmth.
pub fn colour_matrix(c: &Colour) -> ColourMatrix {
    let b = (c.brightness / 100.0) * 0.4;
    let k = (1.0 + c.contrast / 100.0).max(0.0);
    let s = 1.0 + c.saturation / 100.0;
    let w = c.warmth / 100.0;
    let warm = [1.0 + 0.15 * w, 1.0, 1.0 - 0.15 * w];
    let mut m = vec![0.0; 9];
    for r in 0..3 {
        for j in 0..3 {
            let diag = if r == j { s } else { 0.0 };
            m[r * 3 + j] = tidy(warm[r] * k * ((1.0 - s) * LUMA[j] + diag));
        }
    }
    let o = k * b + 0.5 * (1.0 - k);
    let offset: Vec<f64> = warm.iter().map(|wr| tidy(wr * o)).collect();
    let mut svg = Vec::with_capacity(20);
    for r in 0..3 {
        svg.extend([m[r * 3], m[r * 3 + 1], m[r * 3 + 2], 0.0, offset[r]].map(round6));
    }
    svg.extend([0.0, 0.0, 0.0, 1.0, 0.0]);
    ColourMatrix { m, offset, svg }
}

/// Applies a colour matrix to one sRGB-encoded pixel, values 0 to 1.
pub fn apply_colour(cm: &ColourMatrix, rgb: [f64; 3]) -> [f64; 3] {
    let mut out = [0.0; 3];
    for (r, o) in out.iter_mut().enumerate() {
        let v = cm.m[r * 3] * rgb[0]
            + cm.m[r * 3 + 1] * rgb[1]
            + cm.m[r * 3 + 2] * rgb[2]
            + cm.offset[r];
        *o = v.clamp(0.0, 1.0);
    }
    out
}

/// A photo's statistics for Auto: the mean and standard deviation of R,
/// G and B in levels from 0 to 255.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct ColourStats {
    pub mean: [f64; 3],
    pub stdev: [f64; 3],
}

/// Sets the four sliders from the photo's own statistics, each within ±60;
/// saturation is always 0.
pub fn auto_colour(stats: &ColourStats) -> Colour {
    let ok = |v: f64| if v.is_finite() { v } else { 0.0 };
    let [r, g, b] = stats.mean.map(ok);
    let [sr, sg, sb] = stats.stdev.map(|v| ok(v).max(0.0));
    let mean_l = LUMA[0] * r + LUMA[1] * g + LUMA[2] * b;
    let spread = LUMA[0] * sr + LUMA[1] * sg + LUMA[2] * sb;
    let limit = |v: f64| {
        let r = v.clamp(-60.0, 60.0).round();
        if r == 0.0 { 0.0 } else { r }
    };
    Colour {
        brightness: limit((118.0 - mean_l) / 1.02),
        contrast: limit((55.0 / spread.max(1.0) - 1.0) * 100.0),
        saturation: 0.0,
        warmth: if r + b < 1.0 {
            0.0
        } else {
            limit((b - r) / (0.15 * (r + b)) * 100.0)
        },
    }
}

/// How a box is filled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// The whole output fits inside the box (preview, viewer, edit stage).
    Contain,
    /// The output covers the box; the box cuts the overflow (tiles).
    Cover,
}

/// What `EditedImage` sets, as text, for a plan in a box.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Drawn {
    /// The outer element's width and height.
    pub outer_w: f64,
    pub outer_h: f64,
    /// The `<img>`'s width and height.
    pub img_w: f64,
    pub img_h: f64,
    /// The `<img>`'s CSS transform.
    pub transform: String,
    /// The filter's `values` attribute; None without a colour change.
    pub filter: Option<String>,
}

/// Writes a number as the page does: four decimal places, no `-0`.
fn css(v: f64) -> String {
    format!("{}", round4(v))
}

/// The drawing rules of `EditedImage` (docs/06, "How the page draws a
/// plan"). The page test reads the fixtures this produces and must give
/// the same text.
pub fn draw(plan: &EditPlan, box_w: f64, box_h: f64, fit: Fit) -> Drawn {
    let ow = f64::from(plan.out.w);
    let oh = f64::from(plan.out.h);
    let f = match fit {
        Fit::Contain => (box_w / ow).min(box_h / oh),
        Fit::Cover => (box_w / ow).max(box_h / oh),
    };
    let width = f64::from(plan.pic.w) * f;
    let height = f64::from(plan.pic.h) * f;
    let tx = (f64::from(plan.frame.w) / 2.0 - f64::from(plan.crop.left)) * f;
    let ty = (f64::from(plan.frame.h) / 2.0 - f64::from(plan.crop.top)) * f;
    let mut transform = format!(
        "translate({}px, {}px) rotate({}deg)",
        css(tx),
        css(ty),
        css(plan.angle)
    );
    if plan.flip {
        transform.push_str(" scaleX(-1)");
    }
    transform.push_str(&format!(
        " translate({}px, {}px)",
        css(-width / 2.0),
        css(-height / 2.0)
    ));
    Drawn {
        outer_w: round4(ow * f),
        outer_h: round4(oh * f),
        img_w: round4(width),
        img_h: round4(height),
        transform,
        filter: plan.colour.as_ref().map(|c| {
            c.svg
                .iter()
                .map(|v| format!("{v}"))
                .collect::<Vec<_>>()
                .join(" ")
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::model::{CropRect, IDENTITY};

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn straighten_scale_by_hand() {
        assert_eq!(straighten_scale(3.0, 2.0, 0.0), 1.0);
        // 3:2 at 10°: min(3 / (3c + 2s), 2 / (3s + 2c)).
        let (s, c) = (10f64.to_radians().sin(), 10f64.to_radians().cos());
        let want = (3.0 / (3.0 * c + 2.0 * s)).min(2.0 / (3.0 * s + 2.0 * c));
        assert!(close(straighten_scale(3000.0, 2000.0, 10.0), want));
        assert!(close(straighten_scale(3000.0, 2000.0, -10.0), want));
        assert!((want - 0.803_0).abs() < 1e-3, "{want}");
        // 1:1 at 45°: 1 / √2.
        assert!(close(straighten_scale(1.0, 1.0, 45.0), 1.0 / 2f64.sqrt()));
        // 3:2 at 45°: 2 / (5 / √2).
        assert!(close(
            straighten_scale(3.0, 2.0, 45.0),
            2.0 * 2f64.sqrt() / 5.0
        ));
    }

    #[test]
    fn plans_at_each_quarter_turn() {
        for (rotate, fw, fh) in [
            (0, 3000, 2000),
            (90, 2000, 3000),
            (180, 3000, 2000),
            (270, 2000, 3000),
        ] {
            let p = edit_plan(3000, 2000, &PhotoEdit { rotate, ..IDENTITY }, false);
            assert_eq!(p.pic, PlanSize { w: 3000, h: 2000 });
            assert_eq!(p.frame, PlanSize { w: fw, h: fh });
            assert_eq!(p.out, PlanSize { w: fw, h: fh });
            assert_eq!(p.angle, f64::from(rotate));
            assert_eq!(
                p.crop,
                PlanCrop {
                    left: 0,
                    top: 0,
                    width: fw,
                    height: fh
                }
            );
            assert!(p.colour.is_none());
        }
    }

    #[test]
    fn the_crop_clamps_and_the_output_is_never_empty() {
        let e = PhotoEdit {
            crop: CropRect {
                x: 0.99,
                y: 0.99,
                w: 0.01,
                h: 0.01,
            },
            ..IDENTITY
        };
        let p = edit_plan(3000, 2000, &e, false);
        assert_eq!(
            p.crop,
            PlanCrop {
                left: 2970,
                top: 1980,
                width: 30,
                height: 20
            }
        );
        let p = edit_plan(10, 10, &e, false);
        assert_eq!(
            p.crop,
            PlanCrop {
                left: 9,
                top: 9,
                width: 1,
                height: 1
            }
        );
        let p = edit_plan(10, 10, &e, true);
        assert_eq!(p.out, PlanSize { w: 10, h: 10 }, "whole ignores the crop");
        let p = edit_plan(
            1,
            1,
            &PhotoEdit {
                straighten: 45.0,
                ..IDENTITY
            },
            false,
        );
        assert_eq!(p.out, PlanSize { w: 1, h: 1 });
    }

    #[test]
    fn the_colour_check_table() {
        let id = colour_matrix(&Colour::NONE);
        assert_eq!(id.m, [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
        assert_eq!(id.offset, [0.0, 0.0, 0.0]);
        let b = colour_matrix(&Colour {
            brightness: 50.0,
            ..Colour::NONE
        });
        assert_eq!(b.m, id.m);
        assert!(b.offset.iter().all(|o| close(*o, 0.2)));
        let c = colour_matrix(&Colour {
            contrast: -100.0,
            ..Colour::NONE
        });
        assert!(c.m.iter().all(|v| *v == 0.0));
        assert!(c.offset.iter().all(|o| close(*o, 0.5)));
        let s = colour_matrix(&Colour {
            saturation: -100.0,
            ..Colour::NONE
        });
        for r in 0..3 {
            for j in 0..3 {
                assert!(close(s.m[r * 3 + j], LUMA[j]));
            }
        }
        let w = colour_matrix(&Colour {
            warmth: 100.0,
            ..Colour::NONE
        });
        assert!(close(w.m[0], 1.15) && close(w.m[4], 1.0) && close(w.m[8], 0.85));
        assert_eq!(w.svg.len(), 20);
        assert_eq!(&w.svg[15..], &[0.0, 0.0, 0.0, 1.0, 0.0]);
        // The same as the four steps one after another.
        let all = Colour {
            brightness: 20.0,
            contrast: 30.0,
            saturation: -40.0,
            warmth: 50.0,
        };
        let cm = colour_matrix(&all);
        let px = [0.2, 0.5, 0.7];
        let step = px.map(|v| 1.3 * (v + 0.08 - 0.5) + 0.5);
        let l = LUMA[0] * step[0] + LUMA[1] * step[1] + LUMA[2] * step[2];
        let sat = step.map(|v| 0.6 * v + 0.4 * l);
        let want = [sat[0] * 1.075, sat[1], sat[2] * 0.925].map(|v| v.clamp(0.0, 1.0));
        let got = apply_colour(&cm, px);
        for i in 0..3 {
            assert!(close(got[i], want[i]), "{got:?} {want:?}");
        }
    }

    #[test]
    fn auto_colour_rules() {
        let stats = |m: f64, s: f64| ColourStats {
            mean: [m; 3],
            stdev: [s; 3],
        };
        assert!(auto_colour(&stats(40.0, 55.0)).brightness > 0.0, "dark");
        assert!(auto_colour(&stats(118.0, 10.0)).contrast > 0.0, "flat");
        let blue = ColourStats {
            mean: [100.0, 118.0, 140.0],
            stdev: [55.0; 3],
        };
        assert!(auto_colour(&blue).warmth > 0.0, "a blue cast warms");
        assert_eq!(auto_colour(&stats(118.0, 55.0)), Colour::NONE, "balanced");
        for c in [
            auto_colour(&stats(0.0, 0.0)),
            auto_colour(&stats(255.0, 0.0)),
        ] {
            for v in c.values() {
                assert!(v.is_finite() && v.abs() <= 60.0, "{c:?}");
            }
            assert_eq!(c.saturation, 0.0);
        }
        let nan = ColourStats {
            mean: [f64::NAN; 3],
            stdev: [-1.0; 3],
        };
        assert!(auto_colour(&nan).values().iter().all(|v| v.is_finite()));
    }

    #[test]
    fn drawing_follows_the_rules() {
        let p = edit_plan(
            4,
            2,
            &PhotoEdit {
                rotate: 90,
                flip: true,
                ..IDENTITY
            },
            false,
        );
        let d = draw(&p, 10.0, 10.0, Fit::Contain);
        // out is 2 × 4; contain gives f = 2.5.
        assert_eq!(
            (d.outer_w, d.outer_h, d.img_w, d.img_h),
            (5.0, 10.0, 10.0, 5.0)
        );
        assert_eq!(
            d.transform,
            "translate(2.5px, 5px) rotate(90deg) scaleX(-1) translate(-5px, -2.5px)"
        );
        assert_eq!(d.filter, None);
        let d = draw(&p, 10.0, 10.0, Fit::Cover);
        assert_eq!((d.outer_w, d.outer_h), (10.0, 20.0));
    }
}
