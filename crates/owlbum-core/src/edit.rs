//! The edit model, rendering and saving (`edit_`;
//! docs/06-editing-and-file-actions.md).
//!
//! - [`auto`]: Auto's statistics, measured on the medium thumbnail.
//! - [`model`]: `PhotoEdit`, parsing, the stored form, identity, keys, and
//!   the quarter turns and flip.
//! - [`plan`]: `edit_plan`, `colour_matrix`, `auto_colour`, and the drawing
//!   rules the page's `EditedImage` follows.
//! - [`metadata`]: the EXIF and XMP an edited file keeps from its original.
//! - [`render`]: the renderer that writes edited pictures.
//! - [`save`]: Save into the file, Save all, Save as…, the save record
//!   and its recovery.
//! - [`store`]: `photos.edit`, `edit_set`, `edit_turn`, `edit_revert` and
//!   their undo steps.

pub mod auto;
pub mod metadata;
pub mod model;
pub mod plan;
pub mod render;
pub mod save;
pub mod store;

pub use model::{Colour, CropRect, IDENTITY, PhotoEdit, TurnOp, parse_edit};
pub use plan::{EditPlan, edit_plan};

use crate::error::{AppError, AppResult, ErrorCode};
use crate::text;

/// [`parse_edit`] of an edit sent by the page: `edit_invalid` when it does
/// not parse.
pub fn parse_input(json: &serde_json::Value) -> AppResult<PhotoEdit> {
    parse_edit(json).ok_or_else(|| AppError::new(ErrorCode::EditInvalid, text::edit::INVALID))
}

#[cfg(test)]
mod fixtures;

/// An edit as the page sends it. Kept as JSON until [`parse_input`] checks
/// it, so a bad edit answers `edit_invalid` instead of failing to arrive;
/// the page's type is `PhotoEdit`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(transparent)]
#[ts(as = "PhotoEdit")]
pub struct EditInput(pub serde_json::Value);

impl EditInput {
    /// The edit, parsed and clamped; `edit_invalid` when it does not parse.
    pub fn parse(&self) -> AppResult<PhotoEdit> {
        parse_input(&self.0)
    }
}
