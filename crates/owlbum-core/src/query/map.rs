//! `query_map`: the located photos of a result, for the map
//! (docs/04-browsing.md, "What the map holds").
//!
//! The map is the one view that needs every matching photo at once, so it
//! reads the whole result rather than a window. The album index keeps only
//! whether a photo has a place, not where, so the places come from the
//! database in one statement, and from memory for a view-only album.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::engine::QueryEngine;
use crate::album::{Album, sqlite_error};
use crate::edit::EditPlan;
use crate::edit::store::stored_plan;
use crate::error::AppResult;
use crate::scan::memory::Overlay;
use crate::text;

/// One located photo: `[id, lat, lon, thumb_key, file_name, edit_plan]`.
/// Compact, as a whole album's worth crosses to the page at once.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct MapPoint(
    /// The photo's id.
    pub i64,
    /// Latitude of the effective place.
    pub f64,
    /// Longitude of the effective place.
    pub f64,
    /// Builds the marker's `owlbum://thumb` address.
    pub String,
    /// The file name, the marker's tooltip.
    pub String,
    /// The plan for the stored edit, which the marker's thumbnail is drawn
    /// through (docs/06-editing-and-file-actions.md, "Where edits are
    /// shown"); null for none.
    pub Option<EditPlan>,
);

/// What `query_map` answers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct MapPoints {
    /// One per located photo, in the result's order.
    pub points: Vec<MapPoint>,
    /// Photos of the result with no usable place.
    pub unlocated: u32,
}

/// What a photo's row says about its place.
struct Place {
    /// `loc_lat`, `loc_lon`: the typed place, when both are set.
    typed: Option<(f64, f64)>,
    /// `gps_lat`, `gps_lon`, when both are set.
    gps: Option<(f64, f64)>,
    thumb_key: String,
    file_name: String,
    /// The stored edit's plan.
    plan: Option<EditPlan>,
}

impl Place {
    /// The effective place (typed, else GPS) when it lies on the globe.
    fn located(&self) -> Option<(f64, f64)> {
        self.typed
            .or(self.gps)
            .filter(|&(lat, lon)| (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon))
    }
}

impl QueryEngine {
    /// `query_map`: the located photos of the result `token`, in its order,
    /// and how many have no place. `query_expired` for a dropped token.
    pub fn map(&self, album: &Album, overlay: &Overlay, token: &str) -> AppResult<MapPoints> {
        let ids = self.ids(token, 0, u32::MAX)?;
        let mut places = read_places(album)?;
        // A row held in memory (a view-only album) replaces the database's.
        for row in overlay.rows().filter(|r| !r.missing) {
            places.insert(
                row.id,
                Place {
                    typed: None,
                    gps: row.facts.gps_lat.zip(row.facts.gps_lon),
                    thumb_key: row.facts.thumb_key.clone(),
                    file_name: row.file_name().to_owned(),
                    // A row held in memory has no stored edit.
                    plan: None,
                },
            );
        }
        let points: Vec<MapPoint> = ids
            .iter()
            .filter_map(|id| {
                let place = places.get(id)?;
                let (lat, lon) = place.located()?;
                Some(MapPoint(
                    *id,
                    lat,
                    lon,
                    place.thumb_key.clone(),
                    place.file_name.clone(),
                    place.plan.clone(),
                ))
            })
            .collect();
        let unlocated = u32::try_from(ids.len() - points.len()).unwrap_or(u32::MAX);
        Ok(MapPoints { points, unlocated })
    }
}

/// Every photo row that has some place, by id.
fn read_places(album: &Album) -> AppResult<HashMap<i64, Place>> {
    album.db().read(|conn| {
        let failed = sqlite_error(text::query::READING_ROWS);
        let mut stmt = conn
            .prepare_cached(
                "SELECT id, loc_lat, loc_lon, gps_lat, gps_lon, thumb_key, file_name, width, height, edit
                   FROM photos
                  WHERE (loc_lat IS NOT NULL AND loc_lon IS NOT NULL)
                     OR (gps_lat IS NOT NULL AND gps_lon IS NOT NULL)",
            )
            .map_err(&failed)?;
        let rows = stmt
            .query_map([], |r| {
                let pair = |a: usize, b: usize| -> rusqlite::Result<Option<(f64, f64)>> {
                    Ok(r.get::<_, Option<f64>>(a)?.zip(r.get::<_, Option<f64>>(b)?))
                };
                let id = r.get::<_, i64>(0)?;
                let size = |i: usize| -> rusqlite::Result<u32> {
                    Ok(u32::try_from(r.get::<_, Option<i64>>(i)?.unwrap_or(0)).unwrap_or(0))
                };
                let edit: Option<String> = r.get(9)?;
                Ok((
                    id,
                    Place {
                        typed: pair(1, 2)?,
                        gps: pair(3, 4)?,
                        thumb_key: r.get(5)?,
                        file_name: r.get(6)?,
                        plan: stored_plan(id, size(7)?, size(8)?, edit.as_deref()),
                    },
                ))
            })
            .map_err(&failed)?;
        rows.collect::<Result<HashMap<_, _>, _>>().map_err(&failed)
    })
}
