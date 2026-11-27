//! Messages for the edit area: the edit model, rotate and flip, rendering
//! (docs/06-editing-and-file-actions.md).

/// `edit_invalid`: the edit sent by the page did not parse.
pub const INVALID: &str = "That edit could not be understood.";

/// `edit_not_editable`: a video.
pub const VIDEO: &str = "Videos cannot be edited";

/// `edit_not_editable`: an animated picture.
pub const ANIMATED: &str = "Animated pictures cannot be edited";

/// `view_only`: edits cannot be changed in a view-only album.
pub const VIEW_ONLY: &str = "This album is view-only.";

/// `edit_render_failed`: the picture is over the pixel limit.
pub const TOO_LARGE: &str = "This photo is too large to edit.";

/// `edit_render_failed`: a JPEG side is limited to 65,535 pixels.
pub const TOO_WIDE_FOR_JPEG: &str = "the picture is too large for a JPEG";

/// What was being done, for an `internal` error from `edit_auto`.
pub const AUTO: &str = "working out Auto for this photo";

/// What was being done, for an `internal` error while reading edits.
pub const READING: &str = "reading this photo's edit";

/// `edit_render_failed`: the edited photo could not be made.
pub fn render_failed(reason: impl std::fmt::Display) -> String {
    format!("Couldn't make the edited photo: {reason}")
}
