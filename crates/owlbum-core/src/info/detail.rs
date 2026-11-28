//! One photo's details for the details panel: `photo_detail`
//! (docs/04-browsing.md, "The details panel" and "Shapes").
//!
//! The scan facts come from the row the picture URLs would serve: the one
//! the scanner holds in memory for a view-only album, else the database's.
//! A photo is found only when that row is in the album's scope, is not
//! missing and has no scan error, so the panel never shows a photo the
//! grid cannot. The typed info (star, hidden, date, place, people, text)
//! always comes from the database row with the same id.

use std::sync::{Mutex, MutexGuard};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::album::{Album, sqlite_error};
use crate::error::{AppError, AppResult, ErrorCode};
use crate::media::MediaKind;
use crate::scan::memory::Overlay;
use crate::scan::rows::{self, By, Facts};
use crate::scan::walk;
use crate::text;
use crate::thumbs::lookup::servable;

#[cfg(test)]
mod tests;

/// What `photo_detail` answers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PhotoDetail {
    pub id: i64,
    /// Relative to the album root, `/` separators.
    pub rel_path: String,
    /// The folder relative to the album root; empty for the root.
    pub rel_dir: String,
    pub file_name: String,
    /// The file on this computer, for tooltips only; never sent back.
    pub abs_path: String,
    pub kind: MediaKind,
    /// The format registry's id, as `media::format_by_id` takes it.
    pub format: String,
    /// Upright, after orientation; 0 when not known.
    pub width: u32,
    pub height: u32,
    /// The file's size in bytes.
    pub size: u64,
    /// Video length; on kind `image` it marks an animated picture.
    pub duration_ms: Option<u64>,
    /// Builds the `owlbum://thumb` addresses.
    pub thumb_key: String,
    pub date: DateInfo,
    pub place: Place,
    pub camera: Camera,
    /// From the scan's analysis; null until the pixels pass has run.
    pub clipping: Option<Clipping>,
    pub star: bool,
    pub hidden: bool,
    /// In name order, ignoring case.
    pub people: Vec<String>,
    pub description: Option<String>,
    pub notes: Option<String>,
    /// The album revision when this detail was read. The panel reads the
    /// detail again on an `album_changed` with a newer revision.
    pub revision: u64,
    /// The plan for the stored edit, as on `Row`; null for none.
    pub edit_plan: Option<crate::edit::EditPlan>,
    /// The stored edit itself, where edit mode starts
    /// (docs/06-editing-and-file-actions.md, "Edit mode"); null for none.
    pub edit: Option<crate::edit::PhotoEdit>,
    /// What tagging found (docs/07-tagging.md, "\"What's in it\" in the
    /// details panel"). Filled by the shell, which knows this computer's
    /// choice of models; null where it does not.
    pub tags: Option<crate::tagging::tags::PhotoTags>,
}

/// A photo's dates: `YYYY-MM-DDTHH:MM:SS`, wall-clock, no zone.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct DateInfo {
    /// The typed date, else the camera's.
    pub effective: Option<String>,
    /// The file's own date.
    pub camera: Option<String>,
    /// The date typed in Owlbum.
    pub typed: Option<String>,
}

/// A photo's place.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct Place {
    /// The effective place: the typed one, else the file's GPS.
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    /// The file's own GPS.
    pub gps: Option<LatLon>,
    /// True when a place was typed in Owlbum.
    pub typed: bool,
    /// The typed place name.
    pub name: Option<String>,
    /// The country that holds the effective place, from the bundled
    /// outlines; null when there is no place or it is out at sea.
    pub country: Option<String>,
}

/// A latitude and longitude in degrees.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct LatLon {
    pub lat: f64,
    pub lon: f64,
}

/// The camera details; each is null when the file does not say.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct Camera {
    pub camera: Option<String>,
    pub lens: Option<String>,
    pub focal_mm: Option<f64>,
    /// The 35 mm equivalent focal length the file gives.
    pub focal35_mm: Option<f64>,
    /// The f-number.
    pub aperture: Option<f64>,
    /// The exposure time in seconds.
    pub shutter_s: Option<f64>,
    pub iso: Option<i64>,
    /// The exposure bias in EV.
    pub bias_ev: Option<f64>,
    pub flash: Option<bool>,
}

/// How much of the picture is clipped, as fractions from 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct Clipping {
    pub shadows: f64,
    pub highlights: f64,
}

/// The `exif` JSON's keys this module reads (docs/03, "The `exif` JSON").
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ExifJson {
    lens: Option<String>,
    focal_length: Option<f64>,
    focal_length_35: Option<f64>,
    aperture: Option<f64>,
    exposure_time: Option<f64>,
    iso: Option<i64>,
    exposure_bias: Option<f64>,
    flash: Option<bool>,
}

/// The `analysis` JSON's keys this module reads.
#[derive(Debug, Deserialize)]
struct AnalysisJson {
    clipped_shadows: f64,
    clipped_highlights: f64,
}

/// The typed info of a database row.
#[derive(Debug, Default)]
struct Typed {
    star: bool,
    hidden: bool,
    date_override: Option<String>,
    loc: Option<(f64, f64)>,
    loc_name: Option<String>,
    description: Option<String>,
    notes: Option<String>,
    people: Vec<String>,
    /// `photos.edit` as stored.
    edit: Option<String>,
}

fn not_found() -> AppError {
    AppError::new(ErrorCode::PhotoNotFound, text::photo::NOT_FOUND)
}

fn lock(overlay: &Mutex<Overlay>) -> MutexGuard<'_, Overlay> {
    overlay.lock().unwrap_or_else(|p| p.into_inner())
}

/// `photo_detail`: the details of photo `id`. `photo_not_found` when it is
/// not a photo the album shows (unknown, out of scope, missing, or a scan
/// error).
pub fn photo_detail(album: &Album, overlay: &Mutex<Overlay>, id: i64) -> AppResult<PhotoDetail> {
    // Read before the row: a change landing in between makes this detail
    // newer than its revision says, never older, so the panel can only
    // read it once too often.
    let revision = album.revision();
    let held = lock(overlay)
        .get(id)
        .map(|r| (r.id, r.rel_path.clone(), r.missing, r.facts.clone()));
    let (row, typed) = album.db().read(|conn| {
        let row = match held {
            Some(row) => Some(row),
            None => rows::load_row(conn, By::Id(id))?,
        };
        let typed = match &row {
            Some(_) => typed(conn, id)?,
            None => None,
        };
        Ok((row, typed))
    })?;
    let (id, rel_path, missing, facts) = row.ok_or_else(not_found)?;
    if !servable(album, &rel_path, missing, &facts) {
        return Err(not_found());
    }
    Ok(build(
        album,
        id,
        rel_path,
        &facts,
        typed.unwrap_or_default(),
        revision,
    ))
}

/// The typed info of database row `id`, and its people; None when the
/// database has no such row (a row only memory holds).
fn typed(conn: &Connection, id: i64) -> AppResult<Option<Typed>> {
    let failed = sqlite_error(text::photo::READING_DETAIL);
    let typed = conn
        .prepare_cached(concat!(
            "SELECT star, hidden, date_override, loc_lat, loc_lon, loc_name, description, notes, ",
            crate::edit::save::drawn_edit!(),
            " FROM photos WHERE id = ?1",
        ))
        .and_then(|mut st| {
            st.query_row([id], |r| {
                Ok(Typed {
                    star: r.get::<_, i64>(0)? != 0,
                    hidden: r.get::<_, i64>(1)? != 0,
                    date_override: r.get(2)?,
                    loc: r.get::<_, Option<f64>>(3)?.zip(r.get::<_, Option<f64>>(4)?),
                    loc_name: r.get(5)?,
                    description: r.get(6)?,
                    notes: r.get(7)?,
                    people: Vec::new(),
                    edit: r.get(8)?,
                })
            })
            .optional()
        })
        .map_err(&failed)?;
    let Some(mut typed) = typed else {
        return Ok(None);
    };
    typed.people = conn
        .prepare_cached(
            "SELECT p.name FROM photo_people pp JOIN people p ON p.id = pp.person_id
              WHERE pp.photo_id = ?1 ORDER BY p.name COLLATE NOCASE, p.name",
        )
        .and_then(|mut st| st.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>())
        .map_err(&failed)?;
    Ok(Some(typed))
}

fn build(
    album: &Album,
    id: i64,
    rel_path: String,
    f: &Facts,
    t: Typed,
    revision: u64,
) -> PhotoDetail {
    let gps = f
        .gps_lat
        .zip(f.gps_lon)
        .map(|(lat, lon)| LatLon { lat, lon });
    let effective = t.loc.or(gps.map(|g| (g.lat, g.lon)));
    let country = effective
        .and_then(|(lat, lon)| crate::info::country::country_at(lat, lon))
        .map(str::to_owned);
    PhotoDetail {
        id,
        rel_dir: rows::rel_dir(&rel_path).to_owned(),
        file_name: rows::file_name(&rel_path).to_owned(),
        abs_path: walk::absolute(&album.location().root, &rel_path)
            .to_string_lossy()
            .into_owned(),
        rel_path,
        kind: f.kind,
        format: f.format.clone(),
        width: f.width,
        height: f.height,
        size: f.size,
        duration_ms: f.duration_ms,
        thumb_key: f.thumb_key.clone(),
        date: DateInfo {
            effective: t.date_override.clone().or_else(|| f.captured_at.clone()),
            camera: f.captured_at.clone(),
            typed: t.date_override,
        },
        place: Place {
            lat: effective.map(|p| p.0),
            lon: effective.map(|p| p.1),
            gps,
            typed: t.loc.is_some(),
            name: t.loc_name,
            country,
        },
        camera: camera(f.camera.clone(), f.exif.as_deref()),
        clipping: clipping(f.analysis.as_deref()),
        star: t.star,
        hidden: t.hidden,
        people: t.people,
        description: t.description,
        notes: t.notes,
        revision,
        edit_plan: crate::edit::store::stored_plan(id, f.width, f.height, t.edit.as_deref()),
        edit: crate::edit::store::read_stored(id, t.edit.as_deref()),
        tags: None,
    }
}

/// The camera details from the `camera` column and the `exif` JSON. A JSON
/// that does not parse gives no details, never an error.
fn camera(name: Option<String>, exif: Option<&str>) -> Camera {
    let e: ExifJson = exif
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();
    Camera {
        camera: name,
        lens: e.lens,
        focal_mm: e.focal_length,
        focal35_mm: e.focal_length_35,
        aperture: e.aperture,
        shutter_s: e.exposure_time,
        iso: e.iso,
        bias_ev: e.exposure_bias,
        flash: e.flash,
    }
}

/// The clipping from the `analysis` JSON; None while waiting for the
/// pixels pass, or when the JSON does not hold it.
fn clipping(analysis: Option<&str>) -> Option<Clipping> {
    let a: AnalysisJson = serde_json::from_str(analysis?).ok()?;
    Some(Clipping {
        shadows: a.clipped_shadows,
        highlights: a.clipped_highlights,
    })
}
