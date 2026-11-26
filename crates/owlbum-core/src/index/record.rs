//! One photo's record in the album index, and the facts it is built from
//! (docs/05-organising.md, "What it holds per photo").
//!
//! A [`Photo`] is fixed-size (no pointers), so the index is one flat `Vec`
//! and a filter or sort is a pass over contiguous memory. Values that may be
//! absent are kept as plain fields with a presence bit and read through
//! methods that answer `Option`.

use std::num::NonZeroU32;

use serde::Deserialize;

/// The date of a photo with no effective date. It sorts after every real
/// date.
pub const NO_DATE: i64 = i64::MAX;

/// What sort of file a photo is (`photos.kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Kind {
    /// A still or animated picture.
    Image,
    /// A camera RAW file.
    Raw,
    /// A video.
    Video,
}

impl Kind {
    /// The kind for the database's `kind` text. Anything unknown is an
    /// image.
    pub fn from_db(kind: &str) -> Self {
        match kind {
            "raw" => Self::Raw,
            "video" => Self::Video,
            _ => Self::Image,
        }
    }
}

/// The colour buckets of the Main colour sort, in their sort order
/// (docs/05-organising.md, "Colour").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ColourBucket {
    /// Red.
    Red,
    /// Orange.
    Orange,
    /// Yellow.
    Yellow,
    /// Green.
    Green,
    /// Cyan.
    Cyan,
    /// Blue.
    Blue,
    /// Purple.
    Purple,
    /// Pink.
    Pink,
    /// Neutral; also every photo with no average colour.
    #[default]
    Neutral,
}

/// Quality flag bits in [`Derived::quality_flags`].
pub mod quality {
    /// `blurry`.
    pub const BLURRY: u8 = 1;
    /// `dark`.
    pub const DARK: u8 = 2;
    /// `bright`.
    pub const BRIGHT: u8 = 4;
    /// `low_res`.
    pub const LOW_RES: u8 = 8;
}

/// The measures from `photos.analysis` that the index keeps.
///
/// Scalars are `f32`: their stored values have 2 or 3 decimals and fit in
/// `f32` well within that precision for the ranges the analysis gives.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Analysis {
    /// `sharpness`.
    pub sharpness: f32,
    /// `mean_luma`, 0 to 1.
    pub mean_luma: f32,
    /// `clipped_shadows`, 0 to 1.
    pub clipped_shadows: f32,
    /// `clipped_highlights`, 0 to 1.
    pub clipped_highlights: f32,
    /// `saliency_x`, 0 to 1.
    pub saliency_x: f32,
    /// `saliency_y`, 0 to 1.
    pub saliency_y: f32,
    dhash: u64,
    avg_colour: [u8; 3],
    colour_grid: [u8; 27],
    present: u8,
}

const HAS_DHASH: u8 = 1;
const HAS_AVG_COLOUR: u8 = 2;
const HAS_COLOUR_GRID: u8 = 4;

impl Analysis {
    /// Reads the `analysis` JSON. Answers None when it is not analysed: no
    /// JSON, `{}` (a video with no poster frame), JSON that does not parse,
    /// or one of the six scalar measures missing. `dhash`, `avg_colour` and
    /// `colour_grid` are each optional.
    pub fn parse(json: &str) -> Option<Self> {
        let raw: RawAnalysis = serde_json::from_str(json).ok()?;
        let mut a = Self {
            sharpness: raw.sharpness? as f32,
            mean_luma: raw.mean_luma? as f32,
            clipped_shadows: raw.clipped_shadows? as f32,
            clipped_highlights: raw.clipped_highlights? as f32,
            saliency_x: raw.saliency_x? as f32,
            saliency_y: raw.saliency_y? as f32,
            ..Self::default()
        };
        if let Some(h) = raw
            .dhash
            .as_deref()
            .filter(|h| h.len() == 16)
            .and_then(|h| u64::from_str_radix(h, 16).ok())
        {
            a.dhash = h;
            a.present |= HAS_DHASH;
        }
        if let Some(c) = raw.avg_colour {
            a.avg_colour = c;
            a.present |= HAS_AVG_COLOUR;
        }
        if let Some(g) = raw.colour_grid.and_then(|g| <[u8; 27]>::try_from(g).ok()) {
            a.colour_grid = g;
            a.present |= HAS_COLOUR_GRID;
        }
        Some(a)
    }

    /// The 64-bit dHash, when there is one.
    pub fn dhash(&self) -> Option<u64> {
        (self.present & HAS_DHASH != 0).then_some(self.dhash)
    }

    /// `avg_colour` as `[R, G, B]`, when there is one.
    pub fn avg_colour(&self) -> Option<[u8; 3]> {
        (self.present & HAS_AVG_COLOUR != 0).then_some(self.avg_colour)
    }

    /// `colour_grid`: 27 integers, 3 × 3 row-major RGB, when there is one.
    pub fn colour_grid(&self) -> Option<&[u8; 27]> {
        (self.present & HAS_COLOUR_GRID != 0).then_some(&self.colour_grid)
    }
}

/// Values worked out over the whole album index by the derived-values
/// rebuild (#27 to #31). A photo that no rebuild has covered yet has the
/// default: no score, no burst, no flags.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Derived {
    /// The photo score; None draws as "–" (not analysed, or not scored yet).
    pub score: Option<i32>,
    /// `quality`; meaningful only for an analysed photo.
    pub quality: f32,
    /// `interesting`; meaningful only for an analysed photo.
    pub interesting: f32,
    /// Lightness of the average colour, 0 to 1; 0 with no average colour.
    pub lightness: f32,
    /// The burst this photo is in: the burst's id. None when in no burst.
    pub burst: Option<NonZeroU32>,
    /// [`quality`] bits.
    pub quality_flags: u8,
    /// The Main colour bucket.
    pub colour: ColourBucket,
    /// True for the keeper of its burst.
    pub keeper: bool,
}

const STAR: u8 = 1;
const HIDDEN: u8 = 2;
const HAS_LOCATION: u8 = 4;
const HAS_EDIT: u8 = 8;
const ANALYSED: u8 = 16;

/// One photo in the album index.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Photo {
    /// `photos.id`. Ids above `u32::MAX` are never loaded.
    pub id: u32,
    /// The photo's folder: an index into the index's folder list
    /// ([`super::IndexView::folder`]).
    pub folder: u32,
    /// The photo's position in the name order, from 0
    /// ([`super::IndexView::name_order`]).
    pub name_rank: u32,
    /// The effective date (`date_override`, else `captured_at`) as seconds
    /// from 1970-01-01T00:00:00 of the same wall clock, with no time zone;
    /// [`NO_DATE`] when there is none.
    pub date: i64,
    /// `score_adjust`.
    pub score_adjust: i32,
    /// `width`, upright; 0 when unknown.
    pub width: u32,
    /// `height`, upright; 0 when unknown.
    pub height: u32,
    /// How many people are named on the photo (`photo_people` rows).
    pub people: u16,
    /// `kind`.
    pub kind: Kind,
    flags: u8,
    pub(super) name_at: u32,
    pub(super) name_len: u16,
    analysis: Analysis,
    /// Values from the last derived-values rebuild.
    pub derived: Derived,
}

impl Photo {
    /// The effective date, as [`Photo::date`] counts it; None when there is
    /// none.
    pub fn date(&self) -> Option<i64> {
        (self.date != NO_DATE).then_some(self.date)
    }

    /// Starred.
    pub fn star(&self) -> bool {
        self.flags & STAR != 0
    }

    /// Hidden.
    pub fn hidden(&self) -> bool {
        self.flags & HIDDEN != 0
    }

    /// Has a place: a typed location or GPS.
    pub fn has_location(&self) -> bool {
        self.flags & HAS_LOCATION != 0
    }

    /// Has a stored edit (`photos.edit` is not NULL).
    pub fn has_edit(&self) -> bool {
        self.flags & HAS_EDIT != 0
    }

    /// The analysis, when the photo is analysed.
    pub fn analysis(&self) -> Option<&Analysis> {
        (self.flags & ANALYSED != 0).then_some(&self.analysis)
    }

    /// Sets the star and hidden flags, for tests of rules that read them.
    #[cfg(test)]
    pub(crate) fn set_flags(&mut self, star: bool, hidden: bool) {
        self.flags &= !(STAR | HIDDEN);
        if star {
            self.flags |= STAR;
        }
        if hidden {
            self.flags |= HIDDEN;
        }
    }
}

/// The columns of one `photos` row that the index is built from. The
/// database load fills it from SQL; a view-only album's scanner fills it
/// from the facts it keeps in memory. Either way [`super::AlbumIndex`] turns
/// it into a [`Photo`] the same way.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PhotoFacts {
    /// `id`.
    pub id: i64,
    /// `rel_dir`: `''` for the root, `/` separators.
    pub rel_dir: String,
    /// `file_name`.
    pub file_name: String,
    /// `kind` as the database writes it: `image`, `raw` or `video`.
    pub kind: String,
    /// `captured_at`, `YYYY-MM-DDTHH:MM:SS`.
    pub captured_at: Option<String>,
    /// `date_override`, same form.
    pub date_override: Option<String>,
    /// `star`.
    pub star: bool,
    /// `hidden`.
    pub hidden: bool,
    /// `score_adjust`.
    pub score_adjust: i64,
    /// `width`.
    pub width: i64,
    /// `height`.
    pub height: i64,
    /// `gps_lat`, `gps_lon` when both are set.
    pub gps: Option<(f64, f64)>,
    /// `loc_lat`, `loc_lon` when both are set.
    pub loc: Option<(f64, f64)>,
    /// `edit IS NOT NULL`.
    pub has_edit: bool,
    /// `analysis` JSON.
    pub analysis: Option<String>,
    /// The number of `photo_people` rows for the photo.
    pub people: u32,
}

/// Reads a wall-clock date `YYYY-MM-DDTHH:MM:SS` (a space may stand for the
/// `T`) as seconds from 1970-01-01T00:00:00, by calendar arithmetic with no
/// time zone. None when the text is not such a date.
pub fn parse_date(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    if b.len() != 19
        || b[4] != b'-'
        || b[7] != b'-'
        || !matches!(b[10], b'T' | b' ')
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    let num = |from: usize, to: usize| -> Option<i64> {
        b[from..to].iter().try_fold(0_i64, |n, &d| {
            d.is_ascii_digit().then(|| n * 10 + i64::from(d - b'0'))
        })
    };
    let (y, mo, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (h, mi, s) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let month_days = match mo {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    if d < 1 || d > month_days || h > 23 || mi > 59 || s > 59 {
        return None;
    }
    Some(days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + s)
}

/// Builds the record for `facts`, with its name not yet placed and the
/// default derived values. None when the id does not fit in 32 bits.
pub(crate) fn build(facts: &PhotoFacts, folder: u32) -> Option<Photo> {
    let id = u32::try_from(facts.id).ok()?;
    let analysis = facts.analysis.as_deref().and_then(Analysis::parse);
    let date = facts
        .date_override
        .as_deref()
        .and_then(parse_date)
        .or_else(|| facts.captured_at.as_deref().and_then(parse_date))
        .unwrap_or(NO_DATE);
    let mut flags = 0;
    for (on, bit) in [
        (facts.star, STAR),
        (facts.hidden, HIDDEN),
        (facts.gps.is_some() || facts.loc.is_some(), HAS_LOCATION),
        (facts.has_edit, HAS_EDIT),
        (analysis.is_some(), ANALYSED),
    ] {
        if on {
            flags |= bit;
        }
    }
    Some(Photo {
        id,
        folder,
        name_rank: 0,
        date,
        score_adjust: clamp_i32(facts.score_adjust),
        width: clamp_u32(facts.width),
        height: clamp_u32(facts.height),
        people: u16::try_from(facts.people).unwrap_or(u16::MAX),
        kind: Kind::from_db(&facts.kind),
        flags,
        name_at: 0,
        name_len: 0,
        analysis: analysis.unwrap_or_default(),
        derived: Derived::default(),
    })
}

fn clamp_i32(n: i64) -> i32 {
    i32::try_from(n).unwrap_or(if n < 0 { i32::MIN } else { i32::MAX })
}

fn clamp_u32(n: i64) -> u32 {
    u32::try_from(n).unwrap_or(if n < 0 { 0 } else { u32::MAX })
}

/// Days from 1970-01-01 to a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The `analysis` JSON as stored; unknown keys are ignored.
#[derive(Deserialize)]
struct RawAnalysis {
    sharpness: Option<f64>,
    mean_luma: Option<f64>,
    clipped_shadows: Option<f64>,
    clipped_highlights: Option<f64>,
    saliency_x: Option<f64>,
    saliency_y: Option<f64>,
    dhash: Option<String>,
    avg_colour: Option<[u8; 3]>,
    colour_grid: Option<Vec<u8>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = r#"{"sharpness":152.25,"mean_luma":0.481,"clipped_shadows":0.012,"clipped_highlights":0.004,"saliency_x":0.43,"saliency_y":0.61,"dhash":"8f0c3c3e1e0e0603","avg_colour":[120,98,77],"colour_grid":[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27],"transparent":false}"#;

    fn facts() -> PhotoFacts {
        PhotoFacts {
            id: 7,
            rel_dir: "2024/06".into(),
            file_name: "IMG_1.jpg".into(),
            kind: "image".into(),
            width: 4000,
            height: 3000,
            ..PhotoFacts::default()
        }
    }

    #[test]
    fn a_record_is_small() {
        // docs/05 budgets about 150 bytes a photo for the whole index.
        assert!(
            size_of::<Photo>() <= 136,
            "Photo is {} bytes",
            size_of::<Photo>()
        );
    }

    #[test]
    fn dates_are_wall_clock_seconds() {
        assert_eq!(parse_date("1970-01-01T00:00:00"), Some(0));
        assert_eq!(parse_date("1970-01-02T00:00:01"), Some(86_401));
        assert_eq!(parse_date("1969-12-31T23:59:59"), Some(-1));
        assert_eq!(parse_date("2000-03-01T00:00:00"), Some(951_868_800));
        assert_eq!(parse_date("2024-02-29 12:00:00"), Some(1_709_208_000));
        let a = parse_date("2024-06-01T14:03:00").unwrap();
        let b = parse_date("2024-06-01T16:03:00").unwrap();
        assert_eq!(b - a, 7200, "two hours, with no time zone in the way");
    }

    #[test]
    fn a_date_that_is_not_a_date_is_none() {
        for bad in [
            "",
            "2024-06-01",
            "2024-13-01T00:00:00",
            "2023-02-29T00:00:00",
            "2024-06-31T00:00:00",
            "2024-06-01T24:00:00",
            "2024-06-01T23:60:00",
            "2024-06-01T23:59:60",
            "2024-06-01T23:59:59Z",
            "24-06-01T23:59:59",
            "2024/06/01T23:59:59",
            "２０２４-06-01T00:00:00",
        ] {
            assert_eq!(parse_date(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_typed_date_wins_over_the_cameras() {
        let p = build(
            &PhotoFacts {
                captured_at: Some("2020-01-01T00:00:00".into()),
                date_override: Some("2021-01-01T00:00:00".into()),
                ..facts()
            },
            0,
        )
        .unwrap();
        assert_eq!(p.date(), parse_date("2021-01-01T00:00:00"));
        let p = build(
            &PhotoFacts {
                captured_at: Some("2020-01-01T00:00:00".into()),
                ..facts()
            },
            0,
        )
        .unwrap();
        assert_eq!(p.date(), parse_date("2020-01-01T00:00:00"));
        let p = build(&facts(), 0).unwrap();
        assert_eq!(p.date(), None);
        assert_eq!(p.date, NO_DATE);
    }

    #[test]
    fn the_filter_columns_are_kept() {
        let p = build(
            &PhotoFacts {
                star: true,
                hidden: true,
                score_adjust: -3,
                gps: Some((49.2, -123.1)),
                has_edit: true,
                people: 2,
                kind: "video".into(),
                ..facts()
            },
            5,
        )
        .unwrap();
        assert_eq!(p.id, 7);
        assert_eq!(p.folder, 5);
        assert!(p.star() && p.hidden() && p.has_location() && p.has_edit());
        assert_eq!(p.score_adjust, -3);
        assert_eq!((p.width, p.height), (4000, 3000));
        assert_eq!(p.people, 2);
        assert_eq!(p.kind, Kind::Video);
        assert_eq!(p.derived, Derived::default());

        let p = build(&facts(), 0).unwrap();
        assert!(!p.star() && !p.hidden() && !p.has_location() && !p.has_edit());
        let p = build(
            &PhotoFacts {
                loc: Some((0.0, 0.0)),
                ..facts()
            },
            0,
        )
        .unwrap();
        assert!(p.has_location(), "a typed place counts");
    }

    #[test]
    fn out_of_range_numbers_are_clamped_not_wrapped() {
        let p = build(
            &PhotoFacts {
                width: -5,
                height: i64::MAX,
                score_adjust: i64::MIN,
                people: 100_000,
                ..facts()
            },
            0,
        )
        .unwrap();
        assert_eq!((p.width, p.height), (0, u32::MAX));
        assert_eq!(p.score_adjust, i32::MIN);
        assert_eq!(p.people, u16::MAX);
    }

    #[test]
    fn an_id_beyond_32_bits_is_not_built() {
        assert!(
            build(
                &PhotoFacts {
                    id: i64::from(u32::MAX) + 1,
                    ..facts()
                },
                0
            )
            .is_none()
        );
        assert!(build(&PhotoFacts { id: -1, ..facts() }, 0).is_none());
        assert!(
            build(
                &PhotoFacts {
                    id: i64::from(u32::MAX),
                    ..facts()
                },
                0
            )
            .is_some()
        );
    }

    #[test]
    fn kinds() {
        assert_eq!(Kind::from_db("image"), Kind::Image);
        assert_eq!(Kind::from_db("raw"), Kind::Raw);
        assert_eq!(Kind::from_db("video"), Kind::Video);
        assert_eq!(Kind::from_db("hologram"), Kind::Image);
    }

    #[test]
    fn a_full_analysis_is_read() {
        let a = Analysis::parse(FULL).unwrap();
        assert_eq!(a.sharpness, 152.25);
        assert_eq!(a.mean_luma, 0.481);
        assert_eq!(a.clipped_shadows, 0.012);
        assert_eq!(a.clipped_highlights, 0.004);
        assert_eq!((a.saliency_x, a.saliency_y), (0.43, 0.61));
        assert_eq!(a.dhash(), Some(0x8f0c_3c3e_1e0e_0603));
        assert_eq!(a.avg_colour(), Some([120, 98, 77]));
        let grid: Vec<u8> = (1..=27).collect();
        assert_eq!(a.colour_grid().unwrap().as_slice(), grid.as_slice());

        let p = build(
            &PhotoFacts {
                analysis: Some(FULL.into()),
                ..facts()
            },
            0,
        )
        .unwrap();
        assert_eq!(p.analysis(), Some(&a));
    }

    #[test]
    fn the_optional_measures_may_be_absent() {
        let a = Analysis::parse(
            r#"{"sharpness":0,"mean_luma":0.5,"clipped_shadows":0,"clipped_highlights":0,"saliency_x":0.5,"saliency_y":0.5}"#,
        )
        .unwrap();
        assert_eq!(a.dhash(), None);
        assert_eq!(a.avg_colour(), None);
        assert_eq!(a.colour_grid(), None);
        let a = Analysis::parse(
            r#"{"sharpness":0,"mean_luma":0.5,"clipped_shadows":0,"clipped_highlights":0,"saliency_x":0.5,"saliency_y":0.5,"dhash":"0000000000000000","colour_grid":[1,2,3]}"#,
        )
        .unwrap();
        assert_eq!(a.dhash(), Some(0), "a zero dHash is a dHash");
        assert_eq!(a.colour_grid(), None, "a grid must have 27 values");
    }

    #[test]
    fn what_is_not_analysed() {
        assert_eq!(Analysis::parse("{}"), None, "a video with no poster frame");
        assert_eq!(Analysis::parse("not json"), None);
        assert_eq!(Analysis::parse(r#"{"sharpness":1}"#), None);
        assert_eq!(
            Analysis::parse(
                r#"{"sharpness":"x","mean_luma":0.5,"clipped_shadows":0,"clipped_highlights":0,"saliency_x":0.5,"saliency_y":0.5}"#
            ),
            None
        );
        let p = build(&facts(), 0).unwrap();
        assert_eq!(p.analysis(), None, "NULL analysis");
        let p = build(
            &PhotoFacts {
                analysis: Some("{}".into()),
                ..facts()
            },
            0,
        )
        .unwrap();
        assert_eq!(p.analysis(), None);
    }

    #[test]
    fn a_bad_dhash_leaves_the_rest_of_the_analysis() {
        let a = Analysis::parse(
            r#"{"sharpness":1,"mean_luma":0.5,"clipped_shadows":0,"clipped_highlights":0,"saliency_x":0.5,"saliency_y":0.5,"dhash":"xyz"}"#,
        )
        .unwrap();
        assert_eq!(a.dhash(), None);
        assert_eq!(a.sharpness, 1.0);
    }
}
