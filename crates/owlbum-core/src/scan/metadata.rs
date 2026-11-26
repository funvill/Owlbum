//! The metadata reader (docs/03-media-and-scanning.md, "Metadata
//! reading"): the scan facts that come from a file's metadata (capture
//! date, place, camera and the `exif` JSON), worked out from the tags a
//! handler read ([`crate::media::exif`]) and, for video, its probe.
//!
//! A file with no metadata, or with values that make no sense, is not an
//! error: the facts are NULL.

use chrono::{NaiveDate, NaiveDateTime, TimeZone};
use serde::Serialize;

use crate::media::exif::{Dir, Exif};
use crate::media::{Metadata, VideoInfo};

/// The metadata facts of one file, as the facts pass writes them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MetaFacts {
    /// `YYYY-MM-DDTHH:MM:SS`, wall-clock, no zone.
    pub captured_at: Option<String>,
    /// Latitude and longitude in degrees.
    pub gps: Option<(f64, f64)>,
    pub camera: Option<String>,
    /// The `exif` JSON; None when it would be an empty object.
    pub exif: Option<String>,
}

/// EXIF tags used here.
mod tag {
    pub const MAKE: u16 = 0x010f;
    pub const MODEL: u16 = 0x0110;
    pub const DATE_TIME: u16 = 0x0132;
    pub const EXPOSURE_TIME: u16 = 0x829a;
    pub const F_NUMBER: u16 = 0x829d;
    pub const ISO: u16 = 0x8827;
    pub const DATE_TIME_ORIGINAL: u16 = 0x9003;
    pub const DATE_TIME_DIGITIZED: u16 = 0x9004;
    pub const EXPOSURE_BIAS: u16 = 0x9204;
    pub const FLASH: u16 = 0x9209;
    pub const FOCAL_LENGTH: u16 = 0x920a;
    pub const FOCAL_LENGTH_35: u16 = 0xa405;
    pub const LENS_MAKE: u16 = 0xa433;
    pub const LENS_MODEL: u16 = 0xa434;
    pub const GPS_LAT_REF: u16 = 0x0001;
    pub const GPS_LAT: u16 = 0x0002;
    pub const GPS_LON_REF: u16 = 0x0003;
    pub const GPS_LON: u16 = 0x0004;
}

/// The `exif` JSON, keys in the order docs/03 lists them.
#[derive(Debug, Default, Serialize)]
struct ExifJson {
    #[serde(skip_serializing_if = "Option::is_none")]
    lens: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    focal_length: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    focal_length_35: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    aperture: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exposure_time: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    iso: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exposure_bias: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    flash: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    video_codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    audio_codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    frames: Option<u32>,
}

/// The metadata facts of a picture (`meta`) or a video (`meta` and its
/// probe). A video's date, place and camera come from its container,
/// which the video handler reads (#50); here only its codecs are used.
pub fn read(meta: &Metadata, video: Option<&VideoInfo>) -> MetaFacts {
    let exif = meta.exif.as_ref();
    let json = ExifJson {
        lens: exif.and_then(|e| {
            lens(
                e.text(Dir::Exif, tag::LENS_MAKE),
                e.text(Dir::Exif, tag::LENS_MODEL),
            )
        }),
        focal_length: exif.and_then(|e| positive(e.number(Dir::Exif, tag::FOCAL_LENGTH))),
        focal_length_35: exif.and_then(|e| positive(e.number(Dir::Exif, tag::FOCAL_LENGTH_35))),
        aperture: exif.and_then(|e| positive(e.number(Dir::Exif, tag::F_NUMBER))),
        exposure_time: exif.and_then(|e| positive(e.number(Dir::Exif, tag::EXPOSURE_TIME))),
        iso: exif
            .and_then(|e| e.int(Dir::Exif, tag::ISO))
            .filter(|&n| n > 0),
        exposure_bias: exif
            .and_then(|e| e.number(Dir::Exif, tag::EXPOSURE_BIAS))
            .filter(|n| n.is_finite()),
        flash: exif
            .and_then(|e| e.int(Dir::Exif, tag::FLASH))
            .map(|f| f & 1 == 1),
        video_codec: video
            .and_then(|v| v.video_codec.clone())
            .filter(|s| !s.is_empty()),
        audio_codec: video
            .and_then(|v| v.audio_codec.clone())
            .filter(|s| !s.is_empty()),
        frames: meta.frames.filter(|&n| n > 1),
    };
    MetaFacts {
        captured_at: exif.and_then(captured_at),
        gps: exif.and_then(gps_of),
        camera: exif
            .and_then(|e| camera(e.text(Dir::Main, tag::MAKE), e.text(Dir::Main, tag::MODEL))),
        exif: exif_json(&json),
    }
}

fn exif_json(json: &ExifJson) -> Option<String> {
    let s = serde_json::to_string(json).ok()?;
    (s != "{}").then_some(s)
}

/// A finite number above 0.
fn positive(n: Option<f64>) -> Option<f64> {
    n.filter(|n| n.is_finite() && *n > 0.0)
}

/// The first of DateTimeOriginal, DateTimeDigitized and DateTime that
/// parses.
fn captured_at(exif: &Exif) -> Option<String> {
    [
        (Dir::Exif, tag::DATE_TIME_ORIGINAL),
        (Dir::Exif, tag::DATE_TIME_DIGITIZED),
        (Dir::Main, tag::DATE_TIME),
    ]
    .into_iter()
    .find_map(|(dir, t)| exif.text(dir, t).and_then(parse_date))
}

/// An EXIF date as `YYYY-MM-DDTHH:MM:SS`, when it starts with
/// `^(\d{4})[:-](\d{2})[:-](\d{2})[ T](\d{2}):(\d{2}):(\d{2})`, the year is
/// not 0000 and the numbers are a real date and time. What follows
/// (sub-seconds, an offset) is ignored.
pub fn parse_date(value: &str) -> Option<String> {
    let b = value.as_bytes();
    if b.len() < 19 {
        return None;
    }
    let num = |from: usize, len: usize| -> Option<u32> {
        let digits = &b[from..from + len];
        digits.iter().all(u8::is_ascii_digit).then(|| {
            digits
                .iter()
                .fold(0u32, |n, d| n * 10 + u32::from(d - b'0'))
        })
    };
    let date_sep = |c: u8| c == b':' || c == b'-';
    if !(date_sep(b[4]) && date_sep(b[7]) && (b[10] == b' ' || b[10] == b'T'))
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    let year = num(0, 4)?;
    let (month, day) = (num(5, 2)?, num(8, 2)?);
    let (hour, minute, second) = (num(11, 2)?, num(14, 2)?, num(17, 2)?);
    if year == 0 {
        return None;
    }
    let at: NaiveDateTime = NaiveDate::from_ymd_opt(i32::try_from(year).ok()?, month, day)?
        .and_hms_opt(hour, minute, second)?;
    Some(at.format("%Y-%m-%dT%H:%M:%S").to_string())
}

/// The `camera` rule: Make and Model, trimmed.
pub fn camera(make: Option<&str>, model: Option<&str>) -> Option<String> {
    joined(make, model)
}

/// A make and a model, trimmed: the model alone when it already starts
/// with the make (ignoring case), else both, else whichever is not empty.
fn joined(make: Option<&str>, model: Option<&str>) -> Option<String> {
    let make = make.map(str::trim).filter(|s| !s.is_empty());
    let model = model.map(str::trim).filter(|s| !s.is_empty());
    match (make, model) {
        (None, None) => None,
        (None, Some(m)) | (Some(m), None) => Some(m.to_owned()),
        (Some(make), Some(model)) => {
            if model.to_lowercase().starts_with(&make.to_lowercase()) {
                Some(model.to_owned())
            } else {
                Some(format!("{make} {model}"))
            }
        }
    }
}

/// The `lens` rule: LensModel, prefixed with LensMake and a space unless
/// the model already starts with the make.
pub fn lens(make: Option<&str>, model: Option<&str>) -> Option<String> {
    let model = model.map(str::trim).filter(|s| !s.is_empty())?;
    joined(make, Some(model))
}

/// A latitude and longitude kept only when both are finite, in range, and
/// not both exactly 0.
pub fn gps(lat: Option<f64>, lon: Option<f64>) -> Option<(f64, f64)> {
    let (lat, lon) = (lat?, lon?);
    let ok = lat.is_finite()
        && lon.is_finite()
        && (-90.0..=90.0).contains(&lat)
        && (-180.0..=180.0).contains(&lon)
        && !(lat == 0.0 && lon == 0.0);
    ok.then_some((lat, lon))
}

/// GPSLatitude and GPSLongitude (degrees, minutes, seconds) with their
/// reference tags.
fn gps_of(exif: &Exif) -> Option<(f64, f64)> {
    let coordinate = |value: u16, reference: u16, negative: char| -> Option<f64> {
        // Degrees, minutes, seconds; cameras sometimes write fewer parts.
        let parts = exif.get(Dir::Gps, value)?.numbers();
        if parts.is_empty() || parts.len() > 3 {
            return None;
        }
        let mut degrees = 0.0;
        for (part, scale) in parts.iter().zip([1.0, 60.0, 3600.0]) {
            degrees += (*part)? / scale;
        }
        let sign = match exif.text(Dir::Gps, reference) {
            Some(r) if r.to_ascii_uppercase().starts_with(negative) => -1.0,
            _ => 1.0,
        };
        Some(sign * degrees)
    };
    gps(
        coordinate(tag::GPS_LAT, tag::GPS_LAT_REF, 'S'),
        coordinate(tag::GPS_LON, tag::GPS_LON_REF, 'W'),
    )
}

/// A video's `captured_at` (docs/03, "Video"): a creation date written with
/// its own local time (QuickTime `com.apple.quicktime.creationdate`) gives
/// its wall-clock part; otherwise the container's UTC creation time, in
/// seconds since 1970, is shown in `zone` (this computer's, at scan time).
/// A zero value or the 1904 epoch means none. For the video handler (#50).
pub fn video_captured_at<Tz: TimeZone>(
    local: Option<&str>,
    utc_seconds: Option<i64>,
    zone: &Tz,
) -> Option<String> {
    if let Some(wall) = local.and_then(parse_date) {
        return Some(wall);
    }
    /// 1904-01-01 00:00:00 UTC, the QuickTime epoch, in Unix seconds.
    const EPOCH_1904: i64 = -2_082_844_800;
    let secs = utc_seconds.filter(|&s| s != 0 && s != EPOCH_1904)?;
    let at = zone.timestamp_opt(secs, 0).single()?;
    Some(at.naive_local().format("%Y-%m-%dT%H:%M:%S").to_string())
}

#[cfg(test)]
mod tests {
    use chrono::FixedOffset;
    use owlbum_fixtures::encode::Meta;
    use owlbum_fixtures::tiff::{self, Ifd, Val};

    use super::*;
    use crate::media::exif::Value;

    fn date(s: &str) -> Option<String> {
        parse_date(s)
    }

    #[test]
    fn date_strings() {
        let ok = Some("2025-07-14T09:12:00".to_owned());
        assert_eq!(date("2025:07:14 09:12:00"), ok);
        assert_eq!(date("2025-07-14 09:12:00"), ok);
        assert_eq!(date("2025-07-14T09:12:00"), ok);
        assert_eq!(date("2025:07:14T09:12:00"), ok);
        // Sub-seconds and offsets are ignored, never applied.
        assert_eq!(date("2025:07:14 09:12:00.123"), ok);
        assert_eq!(date("2025:07:14 09:12:00+02:00"), ok);
        assert_eq!(date("2025-07-14T09:12:00Z"), ok);
        // Year 0000, impossible dates and times, and other shapes.
        assert_eq!(date("0000:00:00 00:00:00"), None);
        assert_eq!(date("0000:01:01 00:00:00"), None);
        assert_eq!(date("2025:13:01 00:00:00"), None);
        assert_eq!(date("2025:02:30 00:00:00"), None);
        assert_eq!(date("2025:07:14 24:00:00"), None);
        assert_eq!(date("2025:07:14 09:60:00"), None);
        assert_eq!(date("2025:07:14 09:12:61"), None);
        assert_eq!(date("2025/07/14 09:12:00"), None);
        assert_eq!(date("2025:07:14  09:12:00"), None);
        assert_eq!(date("25:07:14 09:12:00"), None);
        assert_eq!(date("    :  :     :  :  "), None);
        assert_eq!(date(""), None);
        assert_eq!(
            date("2024:02:29 23:59:59"),
            Some("2024-02-29T23:59:59".into())
        );
        assert_eq!(date("２025:07:14 09:12:00"), None, "non-ASCII digits");
    }

    /// An EXIF with the three date tags set as given.
    fn dated(original: Option<&str>, digitized: Option<&str>, modified: Option<&str>) -> Metadata {
        let mut exif = Exif::default();
        for (dir, t, v) in [
            (Dir::Exif, tag::DATE_TIME_ORIGINAL, original),
            (Dir::Exif, tag::DATE_TIME_DIGITIZED, digitized),
            (Dir::Main, tag::DATE_TIME, modified),
        ] {
            if let Some(v) = v {
                exif.set(dir, t, Value::Text(v.into()));
            }
        }
        Metadata {
            exif: Some(exif),
            ..Metadata::default()
        }
    }

    #[test]
    fn the_first_date_tag_that_parses_wins() {
        let a = "2021:01:01 01:01:01";
        let b = "2022:02:02 02:02:02";
        let c = "2023:03:03 03:03:03";
        let at = |m: Metadata| read(&m, None).captured_at;
        assert_eq!(
            at(dated(Some(a), Some(b), Some(c))),
            Some("2021-01-01T01:01:01".into())
        );
        assert_eq!(
            at(dated(None, Some(b), Some(c))),
            Some("2022-02-02T02:02:02".into())
        );
        assert_eq!(
            at(dated(None, None, Some(c))),
            Some("2023-03-03T03:03:03".into())
        );
        assert_eq!(
            at(dated(Some("0000:00:00 00:00:00"), Some("garbage"), Some(c))),
            Some("2023-03-03T03:03:03".into())
        );
        assert_eq!(at(dated(None, None, None)), None);
        assert_eq!(read(&Metadata::default(), None).captured_at, None);
    }

    #[test]
    fn camera_rule() {
        assert_eq!(camera(None, None), None);
        assert_eq!(camera(Some("  "), Some("")), None);
        assert_eq!(camera(None, Some("Pocket 2")), Some("Pocket 2".into()));
        assert_eq!(camera(Some("Owlbum"), None), Some("Owlbum".into()));
        assert_eq!(
            camera(Some("Canon"), Some("Canon EOS R5")),
            Some("Canon EOS R5".into())
        );
        assert_eq!(
            camera(Some("NIKON CORPORATION"), Some("NIKON Z 6")),
            Some("NIKON CORPORATION NIKON Z 6".into())
        );
        assert_eq!(
            camera(Some("SONY"), Some("sony a7")),
            Some("sony a7".into())
        );
        assert_eq!(
            camera(Some(" Owlbum "), Some(" Fixture One ")),
            Some("Owlbum Fixture One".into())
        );
    }

    #[test]
    fn lens_rule() {
        assert_eq!(lens(None, None), None);
        assert_eq!(lens(Some("Canon"), None), None);
        assert_eq!(lens(Some("Canon"), Some("  ")), None);
        assert_eq!(lens(None, Some("24-70mm")), Some("24-70mm".into()));
        assert_eq!(
            lens(Some("Sigma"), Some("24-70mm F2.8")),
            Some("Sigma 24-70mm F2.8".into())
        );
        assert_eq!(
            lens(Some("sigma"), Some("SIGMA 24-70mm")),
            Some("SIGMA 24-70mm".into())
        );
        assert_eq!(lens(Some(" "), Some(" 50mm ")), Some("50mm".into()));
    }

    fn json(m: &Metadata, v: Option<&VideoInfo>) -> serde_json::Value {
        read(m, v).exif.map_or(serde_json::Value::Null, |s| {
            serde_json::from_str(&s).unwrap()
        })
    }

    /// Metadata whose Exif IFD holds `tags`.
    fn with_exif(tags: &[(u16, Val)]) -> Metadata {
        let mut ifd = Ifd::new();
        for (t, v) in tags {
            ifd.set(*t, v.clone());
        }
        let mut ifd0 = Ifd::new();
        ifd0.set(0x8769, Val::Ifds(vec![ifd]));
        Metadata {
            exif: Exif::parse(&tiff::write(&ifd0)),
            ..Metadata::default()
        }
    }

    #[test]
    fn numeric_fields() {
        let m = with_exif(&[
            (tag::FOCAL_LENGTH, Val::Rational(vec![[35, 1]])),
            (tag::FOCAL_LENGTH_35, Val::Short(vec![50])),
            (tag::F_NUMBER, Val::Rational(vec![[56, 10]])),
            (tag::EXPOSURE_TIME, Val::Rational(vec![[1, 250]])),
            (tag::ISO, Val::Short(vec![200, 400])),
            (tag::EXPOSURE_BIAS, Val::SRational(vec![[-2, 3]])),
            (tag::FLASH, Val::Short(vec![0x19])),
        ]);
        let j = json(&m, None);
        assert_eq!(j["focal_length"], 35.0);
        assert_eq!(j["focal_length_35"], 50.0);
        assert_eq!(j["aperture"], 5.6);
        assert_eq!(j["exposure_time"], 0.004);
        assert_eq!(j["iso"], 200, "the first of several ISO values");
        assert!((j["exposure_bias"].as_f64().unwrap() + 2.0 / 3.0).abs() < 1e-12);
        assert_eq!(j["flash"], true);
        assert!(j.get("lens").is_none());

        // Zero, negative and zero-denominator values are left out; a zero
        // or negative bias is kept; flash with bit 0 clear is false.
        let m = with_exif(&[
            (tag::FOCAL_LENGTH, Val::Rational(vec![[0, 1]])),
            (tag::FOCAL_LENGTH_35, Val::Short(vec![0])),
            (tag::F_NUMBER, Val::Rational(vec![[28, 0]])),
            (tag::EXPOSURE_TIME, Val::SRational(vec![[-1, 50]])),
            (tag::ISO, Val::Short(vec![0])),
            (tag::EXPOSURE_BIAS, Val::SRational(vec![[0, 1]])),
            (tag::FLASH, Val::Short(vec![16])),
        ]);
        let j = json(&m, None);
        assert_eq!(
            j,
            serde_json::json!({"exposure_bias": 0.0, "flash": false}),
            "{j}"
        );
        let m = with_exif(&[(tag::EXPOSURE_BIAS, Val::SRational(vec![[1, 0]]))]);
        assert_eq!(read(&m, None).exif, None, "an empty object is NULL");
    }

    #[test]
    fn the_exif_json_keeps_the_documented_key_order() {
        let m = with_exif(&[
            (tag::FLASH, Val::Short(vec![1])),
            (tag::LENS_MODEL, Val::Ascii("50mm".into())),
            (tag::ISO, Val::Short(vec![100])),
        ]);
        assert_eq!(
            read(&m, None).exif.as_deref(),
            Some(r#"{"lens":"50mm","iso":100,"flash":true}"#)
        );
    }

    #[test]
    fn frames_and_codecs_need_no_exif() {
        let m = Metadata {
            frames: Some(3),
            ..Metadata::default()
        };
        assert_eq!(read(&m, None).exif.as_deref(), Some(r#"{"frames":3}"#));
        let v = VideoInfo {
            duration_ms: 2000,
            width: 160,
            height: 120,
            rotation: 0,
            video_codec: Some("h264".into()),
            audio_codec: Some("aac".into()),
        };
        assert_eq!(
            read(&Metadata::default(), Some(&v)).exif.as_deref(),
            Some(r#"{"video_codec":"h264","audio_codec":"aac"}"#)
        );
        assert_eq!(read(&Metadata::default(), None), MetaFacts::default());
    }

    #[test]
    fn gps_range_rule() {
        assert_eq!(gps(Some(49.3), Some(-123.0)), Some((49.3, -123.0)));
        assert_eq!(gps(Some(-90.0), Some(180.0)), Some((-90.0, 180.0)));
        assert_eq!(gps(Some(0.0), Some(12.0)), Some((0.0, 12.0)));
        assert_eq!(gps(Some(0.0), Some(0.0)), None, "no fix");
        assert_eq!(gps(Some(90.5), Some(0.0)), None);
        assert_eq!(gps(Some(10.0), Some(-180.5)), None);
        assert_eq!(gps(Some(f64::NAN), Some(1.0)), None);
        assert_eq!(gps(Some(1.0), Some(f64::INFINITY)), None);
        assert_eq!(gps(Some(1.0), None), None);
        assert_eq!(gps(None, Some(1.0)), None);
    }

    fn located(lat: Val, lat_ref: &str, lon: Val, lon_ref: &str) -> Option<(f64, f64)> {
        let mut g = Ifd::new();
        g.ascii(tag::GPS_LAT_REF, lat_ref);
        g.set(tag::GPS_LAT, lat);
        g.ascii(tag::GPS_LON_REF, lon_ref);
        g.set(tag::GPS_LON, lon);
        let mut ifd0 = Ifd::new();
        ifd0.set(0x8825, Val::Ifds(vec![g]));
        let m = Metadata {
            exif: Exif::parse(&tiff::write(&ifd0)),
            ..Metadata::default()
        };
        read(&m, None).gps
    }

    fn dms(d: u32, m: u32, s_x100: u32) -> Val {
        Val::Rational(vec![[d, 1], [m, 1], [s_x100, 100]])
    }

    fn near(got: Option<(f64, f64)>, want: (f64, f64)) {
        let (a, b) = got.unwrap_or_else(|| panic!("no place, want {want:?}"));
        assert!(
            (a - want.0).abs() < 1e-9 && (b - want.1).abs() < 1e-9,
            "{a}, {b}"
        );
    }

    #[test]
    fn gps_signs_and_checks() {
        near(
            located(dms(49, 30, 0), "N", dms(123, 15, 0), "W"),
            (49.5, -123.25),
        );
        near(
            located(dms(33, 52, 0), "S", dms(151, 12, 36_00), "E"),
            (-(33.0 + 52.0 / 60.0), 151.21),
        );
        // 0, 0 means no fix; a zero denominator is nonsense.
        assert_eq!(located(dms(0, 0, 0), "N", dms(0, 0, 0), "E"), None);
        assert_eq!(
            located(
                Val::Rational(vec![[1, 0], [0, 1], [0, 1]]),
                "N",
                dms(1, 0, 0),
                "E"
            ),
            None
        );
        // Out of range.
        assert_eq!(located(dms(91, 0, 0), "N", dms(1, 0, 0), "E"), None);
        assert_eq!(located(dms(1, 0, 0), "N", dms(181, 0, 0), "W"), None);
    }

    #[test]
    fn a_trip_photo_reads_as_its_fixture_says() {
        let meta = Meta {
            make: Some("Owlbum"),
            model: Some("Fixture One"),
            lens: Some("Fixture 24-70mm F2.8"),
            orientation: 1,
            taken: Some("2025:07:14 09:12:00".into()),
            offset: Some("-07:00"),
            exposure: Some([1, 250]),
            f_number_x10: Some(56),
            iso: Some(100),
            focal_mm: Some(35),
            flash: Some(16),
            gps: Some((49_341_210, -123_002_330)),
            pixels: Some((640, 480)),
        };
        let m = Metadata {
            exif: Exif::parse(&meta.exif_block().unwrap()),
            ..Metadata::default()
        };
        let f = read(&m, None);
        // The offset is not applied: the wall-clock time stays.
        assert_eq!(f.captured_at.as_deref(), Some("2025-07-14T09:12:00"));
        assert_eq!(f.camera.as_deref(), Some("Owlbum Fixture One"));
        let (lat, lon) = f.gps.unwrap();
        assert!((lat - 49.341_21).abs() < 1e-5 && (lon + 123.002_33).abs() < 1e-5);
        assert_eq!(
            f.exif.as_deref(),
            Some(
                r#"{"lens":"Fixture 24-70mm F2.8","focal_length":35.0,"aperture":5.6,"exposure_time":0.004,"iso":100,"flash":false}"#
            )
        );
    }

    #[test]
    fn video_dates() {
        let pacific = FixedOffset::west_opt(7 * 3600).unwrap();
        // Its own local time wins, offset dropped.
        assert_eq!(
            video_captured_at(
                Some("2025-07-14T09:12:00+0200"),
                Some(1_752_480_000),
                &pacific
            ),
            Some("2025-07-14T09:12:00".into())
        );
        // Otherwise UTC shown in the computer's zone: 2025-07-14 08:00 UTC.
        assert_eq!(
            video_captured_at(None, Some(1_752_480_000), &pacific),
            Some("2025-07-14T01:00:00".into())
        );
        assert_eq!(
            video_captured_at(
                Some("nonsense"),
                Some(1_752_480_000),
                &FixedOffset::east_opt(0).unwrap()
            ),
            Some("2025-07-14T08:00:00".into())
        );
        // Zero and the 1904 epoch mean none.
        assert_eq!(video_captured_at(None, Some(0), &pacific), None);
        assert_eq!(
            video_captured_at(None, Some(-2_082_844_800), &pacific),
            None
        );
        assert_eq!(video_captured_at(None, None, &pacific), None);
    }
}
