//! Messages for the photo area: one photo's details and histogram.

/// `photo_not_found`: the id is not a photo of the open album.
pub const NOT_FOUND: &str = "That photo is no longer in this album.";

/// What was being done, for an `internal` error while reading a photo's
/// details.
pub const READING_DETAIL: &str = "reading this photo's details";

/// What was being done, for an `internal` error while counting a photo's
/// histogram.
pub const HISTOGRAM: &str = "drawing this photo's histogram";

/// `view_only`: typed info cannot be changed in a view-only album.
pub const VIEW_ONLY: &str = "This album is view-only.";

/// `invalid_date`: a typed date that is not a real date and time.
pub const INVALID_DATE: &str = "That is not a valid date and time. Use YYYY-MM-DD HH:MM.";

/// `invalid_location`: a latitude out of range.
pub const LATITUDE_RANGE: &str = "Latitude must be between -90 and 90.";

/// `invalid_location`: a longitude out of range.
pub const LONGITUDE_RANGE: &str = "Longitude must be between -180 and 180.";
