//! The RAW handler: camera RAW files read with `rawler` (LGPL-2.1)
//! (docs/03-media-and-scanning.md, "The RAW handler").
//!
//! `metadata` reads the maker's headers (`rawler`'s decode without the
//! pixels) for the size after the default crop and the orientation; the
//! EXIF tags come from the file's own TIFF structure when it has one, else
//! from `rawler`'s reading of the maker's headers. `preview` is the largest
//! embedded picture the camera stored, turned upright; when none is large
//! enough, the sensor data is developed. `working` and `source` use the
//! embedded picture when it is at least 90% of the developed size, so zoom
//! keeps the camera's own colours, and develop otherwise.
//!
//! `rawler` has `todo!` and `panic!` on paths a damaged or unusual file can
//! reach, so every call into it is caught and turned into a decode error.

use std::fs::File;
use std::io::BufReader;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use image::DynamicImage;
use image::metadata::Orientation;
use rawler::decoders::{Decoder, RawDecodeParams, RawMetadata};
use rawler::imgop::develop::RawDevelop;
use rawler::rawsource::RawSource;

use super::exif::{Dir, Exif, Value};
use super::handler::{
    Cost, Layout, MediaError, MediaHandler, Metadata, Pixels, Samples, SourcePixels, VideoInfo,
    check_size,
};
use super::{registry, sniff, still};

/// The RAW handler, one instance for every RAW row.
#[derive(Debug)]
pub struct Raw {
    // Not zero-sized, so the static has an address of its own (the
    // registry tells handlers apart by address).
    _addr: u8,
}

pub static RAW: Raw = Raw { _addr: 0 };

/// The embedded picture stands in for the developed one when its longest
/// side is at least this share of the developed picture's.
const EMBEDDED_SHARE: f64 = 0.9;

/// The decode error for `file`, worded with its format's label.
fn decode_error(file: &Path) -> MediaError {
    let label = file
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(registry::format_of)
        .map_or("RAW", |f| f.label);
    MediaError::decode(label)
}

/// Runs `f`, turning a panic inside `rawler` into a decode error.
fn guarded<T>(file: &Path, f: impl FnOnce() -> Result<T, MediaError>) -> Result<T, MediaError> {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| Err(decode_error(file)))
}

/// An open RAW file: its bytes (memory-mapped, released on drop) and the
/// decoder `rawler` picked for them.
struct Opened {
    source: RawSource,
    decoder: Box<dyn Decoder>,
}

/// What the headers say: the developed picture's size before turning, and
/// the orientation to turn it by.
struct Facts {
    width: u32,
    height: u32,
    orientation: Orientation,
    meta: Option<RawMetadata>,
}

impl Facts {
    /// The upright size of the developed picture.
    fn upright(&self) -> (u32, u32) {
        if swaps_sides(self.orientation) {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }
}

fn params() -> RawDecodeParams {
    RawDecodeParams::default()
}

fn open(file: &Path) -> Result<Opened, MediaError> {
    let source = RawSource::new(file).map_err(|e| MediaError::io(&e))?;
    let decoder = rawler::get_decoder(&source).map_err(|_| decode_error(file))?;
    Ok(Opened { source, decoder })
}

fn swaps_sides(o: Orientation) -> bool {
    matches!(
        o,
        Orientation::Rotate90
            | Orientation::Rotate270
            | Orientation::Rotate90FlipH
            | Orientation::Rotate270FlipH
    )
}

impl Opened {
    /// The headers, without decoding the sensor data where the maker's
    /// decoder allows it.
    fn facts(&self, file: &Path) -> Result<Facts, MediaError> {
        let raw = self
            .decoder
            .raw_image(&self.source, &params(), true)
            .map_err(|_| decode_error(file))?;
        let area = raw.crop_area.or(raw.active_area);
        let (w, h) = area.map_or((raw.width, raw.height), |r| (r.d.w, r.d.h));
        let width = u32::try_from(w).map_err(|_| decode_error(file))?;
        let height = u32::try_from(h).map_err(|_| decode_error(file))?;
        let meta = self.decoder.raw_metadata(&self.source, &params()).ok();
        // Most of `rawler`'s decoders leave the image's orientation at
        // Normal; the EXIF tag is the camera's word.
        let tag = meta
            .as_ref()
            .and_then(|m| m.exif.orientation)
            .or_else(|| Some(raw.orientation.to_u16()))
            .and_then(|o| u8::try_from(o).ok())
            .and_then(Orientation::from_exif)
            .unwrap_or(Orientation::NoTransforms);
        Ok(Facts {
            width,
            height,
            orientation: tag,
            meta,
        })
    }

    /// The largest embedded picture, as stored (not turned).
    fn embedded(&self) -> Option<DynamicImage> {
        let p = params();
        [
            self.decoder.full_image(&self.source, &p),
            self.decoder.preview_image(&self.source, &p),
            self.decoder.thumbnail_image(&self.source, &p),
        ]
        .into_iter()
        .filter_map(|r| r.ok().flatten())
        .max_by_key(|img| img.width().max(img.height()))
    }

    /// The sensor data developed with `rawler`'s default pipeline, as
    /// stored (not turned).
    fn develop(&self, file: &Path, facts: &Facts) -> Result<DynamicImage, MediaError> {
        check_size(facts.width, facts.height)?;
        let raw = self
            .decoder
            .raw_image(&self.source, &params(), false)
            .map_err(|_| decode_error(file))?;
        RawDevelop::default()
            .develop_intermediate(&raw)
            .ok()
            .and_then(|i| i.to_dynamic_image())
            .ok_or_else(|| decode_error(file))
    }

    /// The full-quality picture: the embedded one when it is nearly as
    /// large as the developed one, else the development. Upright.
    fn best(&self, file: &Path) -> Result<(DynamicImage, Facts), MediaError> {
        let facts = self.facts(file)?;
        let longest = f64::from(facts.width.max(facts.height));
        let mut img = match self.embedded() {
            Some(e) if f64::from(e.width().max(e.height())) >= longest * EMBEDDED_SHARE => e,
            _ => self.develop(file, &facts)?,
        };
        img.apply_orientation(facts.orientation);
        Ok((img, facts))
    }
}

/// The EXIF tags of a RAW file. A TIFF-based RAW (CR2, NEF, ARW, DNG) is
/// read like a TIFF; the others get the fields `rawler` read from the
/// maker's headers.
fn exif_of(file: &Path, meta: Option<&RawMetadata>) -> Option<Exif> {
    if let Some(exif) = File::open(file)
        .ok()
        .and_then(|f| Exif::read(&mut BufReader::new(f)))
    {
        return Some(exif);
    }
    let meta = meta?;
    let mut exif = Exif::default();
    let text = |s: &str| Value::Text(s.to_owned());
    let ratio =
        |r: &rawler::formats::tiff::Rational| Value::Ratios(vec![(i64::from(r.n), i64::from(r.d))]);
    let sratio = |r: &rawler::formats::tiff::SRational| {
        Value::Ratios(vec![(i64::from(r.n), i64::from(r.d))])
    };
    if !meta.make.is_empty() {
        exif.set(Dir::Main, 0x010f, text(&meta.make));
    }
    if !meta.model.is_empty() {
        exif.set(Dir::Main, 0x0110, text(&meta.model));
    }
    let e = &meta.exif;
    if let Some(o) = e.orientation {
        exif.set(Dir::Main, 0x0112, Value::Ints(vec![i64::from(o)]));
    }
    if let Some(d) = &e.date_time_original {
        exif.set(Dir::Exif, 0x9003, text(d));
    }
    if let Some(d) = &e.create_date {
        exif.set(Dir::Exif, 0x9004, text(d));
    }
    if let Some(d) = &e.modify_date {
        exif.set(Dir::Main, 0x0132, text(d));
    }
    if let Some(r) = &e.exposure_time {
        exif.set(Dir::Exif, 0x829a, ratio(r));
    }
    if let Some(r) = &e.fnumber {
        exif.set(Dir::Exif, 0x829d, ratio(r));
    }
    if let Some(iso) = e.iso_speed_ratings {
        exif.set(Dir::Exif, 0x8827, Value::Ints(vec![i64::from(iso)]));
    }
    if let Some(r) = &e.exposure_bias {
        exif.set(Dir::Exif, 0x9204, sratio(r));
    }
    if let Some(f) = e.flash {
        exif.set(Dir::Exif, 0x9209, Value::Ints(vec![i64::from(f)]));
    }
    if let Some(r) = &e.focal_length {
        exif.set(Dir::Exif, 0x920a, ratio(r));
    }
    if let Some(s) = &e.lens_make {
        exif.set(Dir::Exif, 0xa433, text(s));
    }
    if let Some(s) = &e.lens_model {
        exif.set(Dir::Exif, 0xa434, text(s));
    }
    if let Some(gps) = &e.gps {
        let triple = |v: &[rawler::formats::tiff::Rational; 3]| {
            Value::Ratios(v.iter().map(|r| (i64::from(r.n), i64::from(r.d))).collect())
        };
        if let (Some(r), Some(v)) = (&gps.gps_latitude_ref, &gps.gps_latitude) {
            exif.set(Dir::Gps, 0x0001, text(r));
            exif.set(Dir::Gps, 0x0002, triple(v));
        }
        if let (Some(r), Some(v)) = (&gps.gps_longitude_ref, &gps.gps_longitude) {
            exif.set(Dir::Gps, 0x0003, text(r));
            exif.set(Dir::Gps, 0x0004, triple(v));
        }
    }
    Some(exif)
}

impl MediaHandler for Raw {
    fn sniff(&self, head: &[u8]) -> bool {
        sniff::raw_family(head)
    }

    fn metadata(&self, file: &Path) -> Result<Metadata, MediaError> {
        guarded(file, || {
            let opened = open(file)?;
            let facts = opened.facts(file)?;
            check_size(facts.width, facts.height)?;
            let (width, height) = facts.upright();
            drop(opened);
            Ok(Metadata {
                width,
                height,
                exif: exif_of(file, facts.meta.as_ref()),
                ..Metadata::default()
            })
        })
    }

    fn preview(&self, file: &Path, min_side: u32) -> Result<Pixels, MediaError> {
        guarded(file, || {
            let opened = open(file)?;
            let facts = opened.facts(file)?;
            let mut img = match opened.embedded() {
                Some(e) if e.width().max(e.height()) >= min_side => e,
                _ => opened.develop(file, &facts)?,
            };
            img.apply_orientation(facts.orientation);
            let mut px = still::to_pixels(&img);
            (px.full_width, px.full_height) = facts.upright();
            Ok(px)
        })
    }

    fn working(&self, file: &Path) -> Result<Pixels, MediaError> {
        guarded(file, || {
            let (img, _) = open(file)?.best(file)?;
            Ok(still::to_pixels(&img))
        })
    }

    fn source(&self, file: &Path) -> Result<SourcePixels, MediaError> {
        guarded(file, || {
            let (img, _) = open(file)?.best(file)?;
            let (width, height) = (img.width(), img.height());
            let samples = match img {
                DynamicImage::ImageRgb8(b) => Samples::U8(b.into_raw()),
                DynamicImage::ImageRgb16(b) => Samples::U16(b.into_raw()),
                other => Samples::U16(other.to_rgb16().into_raw()),
            };
            Ok(SourcePixels {
                width,
                height,
                layout: Layout::Rgb,
                samples,
                icc: None,
            })
        })
    }

    fn probe(&self, _file: &Path) -> Result<Option<VideoInfo>, MediaError> {
        Ok(None)
    }

    fn cost(&self, _file_size: u64) -> Cost {
        Cost::Heavy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hand check of real RAW files (#157): for every file in the folder
    /// `OWLBUM_RAWS`, the sensor size, the embedded pictures' sizes, and
    /// the time of `metadata`, `preview(1600)` and `working`, and whether
    /// each used an embedded picture or developed. Run on a release build:
    /// `OWLBUM_RAWS=<folder> cargo test --release -p owlbum-core --lib
    /// raw::tests::real_raw_files -- --ignored --nocapture`.
    #[test]
    #[ignore = "a hand check with real RAW files"]
    fn real_raw_files() {
        use std::time::Instant;
        let dir = std::env::var_os("OWLBUM_RAWS").expect("OWLBUM_RAWS");
        let size = |r: Result<Option<DynamicImage>, _>| {
            r.ok()
                .flatten()
                .map(|i: DynamicImage| (i.width(), i.height()))
        };
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let t = Instant::now();
            let meta = RAW.metadata(&path);
            let meta_took = t.elapsed();
            let Ok(meta) = meta else {
                println!("{name}: metadata failed: {:?}", meta.err());
                continue;
            };
            let opened = open(&path).unwrap();
            let p = params();
            let embedded = (
                size(opened.decoder.full_image(&opened.source, &p)),
                size(opened.decoder.preview_image(&opened.source, &p)),
                size(opened.decoder.thumbnail_image(&opened.source, &p)),
            );
            let t = Instant::now();
            let preview = RAW.preview(&path, 1600).map(|px| (px.width, px.height));
            let preview_took = t.elapsed();
            let t = Instant::now();
            let working = RAW.working(&path).map(|px| (px.width, px.height));
            let working_took = t.elapsed();
            println!(
                "{name}: {}x{}; embedded full/preview/thumbnail {embedded:?}; metadata {meta_took:?}; preview {preview:?} in {preview_took:?}; working {working:?} in {working_took:?}",
                meta.width, meta.height
            );
        }
    }
}
