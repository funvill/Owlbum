//! The photo score (docs/05-organising.md, "Photo score").
//!
//! The score is a sum of lines, each worked out from one photo's record in
//! the album index and, for Focus, the sorted sharpness values of every
//! analysed photo in the index. It is kept in the index
//! ([`crate::index::Derived::score`]); this module only works it out.
//! [`photo_score`] answers the breakdown the score card shows.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::album::Album;
use crate::error::{AppError, AppResult, ErrorCode};
use crate::index::{Kind, Photo};
use crate::text;

/// One line of a score breakdown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ScoreLine {
    /// "Focus", "Exposure", ...
    pub label: String,
    /// Why it scored this, e.g. "sharper than 80 % of this album".
    pub why: String,
    /// Signed whole points; never 0 (a line worth 0 is left out).
    pub points: i32,
}

/// `photo_score`'s answer: the score card's breakdown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ScoreBreakdown {
    /// The sum of the lines. For a photo that is not analysed this is the
    /// sum of the lines it has, though its tile shows "–".
    pub total: i32,
    /// False while the photo has not been read: Focus, Exposure, Clipping,
    /// Colour variety and Composition are then left out.
    pub analysed: bool,
    /// The non-zero lines, in the order of the spec's table.
    pub lines: Vec<ScoreLine>,
}

/// Which line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Line {
    Focus,
    Exposure,
    Clipping,
    Variety,
    Composition,
    People,
    Size,
    Burst,
    Star,
    Hidden,
    Nudges,
}

/// `min(1, max(0, x))`.
fn clamp(x: f64) -> f64 {
    x.clamp(0.0, 1.0)
}

/// Halves away from zero, to a whole number of points.
fn round(x: f64) -> i32 {
    x.round() as i32
}

/// The share of `sorted` strictly below `v`, over `n - 1`; 0.5 when there
/// are at most one value.
pub fn percentile(sorted: &[f32], v: f32) -> f64 {
    let n = sorted.len();
    if n <= 1 {
        return 0.5;
    }
    let below = sorted.partition_point(|&s| s < v);
    // A photo re-read since the list was made may not be in it; it can
    // then be above every value, but its share stays at most 1.
    (below as f64 / (n - 1) as f64).min(1.0)
}

/// Colour variety: the mean distance of the 9 grid colours from their mean
/// colour, over the largest possible RGB distance; 0 with no grid.
pub(crate) fn variety(grid: Option<&[u8; 27]>) -> f64 {
    let Some(grid) = grid else { return 0.0 };
    // An array, not a Vec: the Most interesting sort calls this for every
    // photo, and 100,000 allocations cost a few milliseconds.
    let mut cells = [[0.0_f64; 3]; 9];
    for (cell, c) in cells.iter_mut().zip(grid.as_chunks::<3>().0) {
        *cell = [f64::from(c[0]), f64::from(c[1]), f64::from(c[2])];
    }
    let mut mean = [0.0; 3];
    for c in &cells {
        for i in 0..3 {
            mean[i] += c[i] / 9.0;
        }
    }
    let spread: f64 = cells
        .iter()
        .map(|c| {
            ((c[0] - mean[0]).powi(2) + (c[1] - mean[1]).powi(2) + (c[2] - mean[2]).powi(2)).sqrt()
        })
        .sum::<f64>()
        / 9.0;
    clamp(spread / (3.0 * 255.0_f64 * 255.0).sqrt())
}

fn megapixels(p: &Photo) -> f64 {
    f64::from(p.width) * f64::from(p.height) / 1_000_000.0
}

/// Every line's points, unrounded where a line has a fraction, in table
/// order. Lines that do not apply are left out.
fn raw_lines(p: &Photo, sharpness: &[f32]) -> Vec<(Line, i32)> {
    let mut out = Vec::with_capacity(11);
    if let Some(a) = p.analysis() {
        out.push((
            Line::Focus,
            round(35.0 * percentile(sharpness, a.sharpness)),
        ));
        let luma = f64::from(a.mean_luma);
        out.push((
            Line::Exposure,
            round(20.0 * clamp(1.0 - (luma - 0.48).abs() * 2.2)),
        ));
        let clipped = f64::from(a.clipped_highlights) + f64::from(a.clipped_shadows);
        if clipped > 0.02 {
            out.push((
                Line::Clipping,
                round(-15.0 * clamp((clipped - 0.02) / (0.12 - 0.02))),
            ));
        }
        out.push((
            Line::Variety,
            round(10.0 * clamp(variety(a.colour_grid()) / 0.3)),
        ));
        let d = (f64::from(a.saliency_x) - 0.5).hypot(f64::from(a.saliency_y) - 0.5);
        let big_d = (1.0_f64 / 6.0).hypot(1.0 / 6.0);
        out.push((
            Line::Composition,
            round(5.0 * clamp(1.0 - (d - big_d).abs() / big_d)),
        ));
    }
    out.push((Line::People, (5 * i32::from(p.people)).min(10)));
    if p.kind != Kind::Video {
        out.push((Line::Size, round(10.0 * clamp(megapixels(p) / 8.0))));
    }
    if p.derived.burst.is_some() && !p.derived.keeper {
        out.push((Line::Burst, -10));
    }
    if p.star() {
        out.push((Line::Star, 50));
    }
    if p.hidden() {
        out.push((Line::Hidden, -50));
    }
    out.push((Line::Nudges, p.score_adjust));
    out.retain(|(_, points)| *points != 0);
    out
}

/// The score the album index keeps and a tile shows: None while the photo
/// is not analysed ("–").
pub fn score(p: &Photo, sharpness: &[f32]) -> Option<i32> {
    p.analysis()?;
    Some(
        raw_lines(p, sharpness)
            .iter()
            .fold(0_i32, |sum, (_, n)| sum.saturating_add(*n)),
    )
}

/// The breakdown of `p`'s score, with the "why" of each line.
pub fn breakdown(p: &Photo, sharpness: &[f32]) -> ScoreBreakdown {
    let lines: Vec<ScoreLine> = raw_lines(p, sharpness)
        .into_iter()
        .map(|(line, points)| ScoreLine {
            label: label(line).to_owned(),
            why: why(line, p, sharpness),
            points,
        })
        .collect();
    ScoreBreakdown {
        total: lines
            .iter()
            .fold(0_i32, |sum, l| sum.saturating_add(l.points)),
        analysed: p.analysis().is_some(),
        lines,
    }
}

fn label(line: Line) -> &'static str {
    use text::score as t;
    match line {
        Line::Focus => t::FOCUS,
        Line::Exposure => t::EXPOSURE,
        Line::Clipping => t::CLIPPING,
        Line::Variety => t::VARIETY,
        Line::Composition => t::COMPOSITION,
        Line::People => t::PEOPLE,
        Line::Size => t::SIZE,
        Line::Burst => t::BURST,
        Line::Star => t::STAR,
        Line::Hidden => t::HIDDEN,
        Line::Nudges => t::NUDGES,
    }
}

/// A share 0 to 1 as a whole percent.
fn percent(share: f64) -> i64 {
    (share * 100.0).round() as i64
}

fn why(line: Line, p: &Photo, sharpness: &[f32]) -> String {
    use text::score as t;
    let a = p.analysis().copied().unwrap_or_default();
    match line {
        Line::Focus => t::sharper_than(percent(percentile(sharpness, a.sharpness))),
        Line::Exposure => {
            if a.mean_luma < 0.3 {
                t::DARK_SIDE.to_owned()
            } else if a.mean_luma > 0.68 {
                t::BRIGHT_SIDE.to_owned()
            } else {
                t::WELL_EXPOSED.to_owned()
            }
        }
        Line::Clipping => {
            let mut parts = Vec::new();
            if a.clipped_highlights > 0.005 {
                parts.push(t::blown(percent(f64::from(a.clipped_highlights))));
            }
            if a.clipped_shadows > 0.005 {
                parts.push(t::crushed(percent(f64::from(a.clipped_shadows))));
            }
            if parts.is_empty() {
                t::CLIPPED.to_owned()
            } else {
                parts.join(", ")
            }
        }
        Line::Variety => {
            if variety(a.colour_grid()) >= 0.198 {
                t::MANY_COLOURS.to_owned()
            } else {
                t::SOME_COLOUR.to_owned()
            }
        }
        Line::Composition => t::NEAR_A_THIRD.to_owned(),
        Line::People => t::people(p.people),
        Line::Size => t::megapixels(megapixels(p)),
        Line::Burst => t::BETTER_IN_BURST.to_owned(),
        Line::Star => t::STARRED.to_owned(),
        Line::Hidden => t::HIDDEN_WHY.to_owned(),
        Line::Nudges => t::nudged(p.score_adjust),
    }
}

/// `photo_score(id)`: the breakdown of one photo's score, from the album
/// index. `photo_not_found` when the photo is not in it (missing, unreadable
/// or outside the open scope).
pub fn photo_score(album: &Album, id: i64) -> AppResult<ScoreBreakdown> {
    let not_found = || AppError::new(ErrorCode::PhotoNotFound, text::photo::NOT_FOUND);
    let id = u32::try_from(id).map_err(|_| not_found())?;
    album
        .index()
        .read(|v| v.get(id).map(|p| breakdown(p, v.sharpness())))
        .ok_or_else(not_found)
}

#[cfg(test)]
mod tests;
