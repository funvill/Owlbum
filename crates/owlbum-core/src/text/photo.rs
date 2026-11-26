//! Messages for the photo area: one photo's details and histogram.

/// `photo_not_found`: the id is not a photo of the open album.
pub const NOT_FOUND: &str = "That photo is no longer in this album.";

/// What was being done, for an `internal` error while reading a photo's
/// details.
pub const READING_DETAIL: &str = "reading this photo's details";

/// What was being done, for an `internal` error while counting a photo's
/// histogram.
pub const HISTOGRAM: &str = "drawing this photo's histogram";
