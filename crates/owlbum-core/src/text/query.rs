//! Messages for the query area: running a query and reading its rows.

/// `bad_query`: the query sent by the page could not be read at all.
pub const UNREADABLE: &str = "Owlbum couldn't understand that search. Try clearing the filter.";

/// `bad_query`: the query asks for something this version cannot do yet.
/// `what` names it ("the timeline view", "sorting by score").
pub fn not_yet(what: impl std::fmt::Display) -> String {
    format!("Owlbum can't show {what} yet.")
}

/// `query_expired`: the token was dropped; the page runs the query again.
pub const EXPIRED: &str = "The list of photos changed. Showing it again.";

/// What was being done, for an `internal` error while reading rows.
pub const READING_ROWS: &str = "reading the photos to show";

/// What a query asks for that is not built yet, for [`not_yet`].
pub const TIMELINE: &str = "the timeline view";
/// See [`TIMELINE`].
pub const SEARCH: &str = "search results";

/// `bad_query`: a filter value that is not allowed (a date that is not a
/// real day, a blank tag or person).
pub fn bad_value(value: &str) -> String {
    format!("Owlbum couldn't use \"{value}\" as a filter. Try clearing the filter.")
}

/// The label of the open scope's own folder, in the folder list and its
/// chip (docs/05-organising.md, "The list of filters").
pub const TOP_FOLDER: &str = "Top folder";

/// Chip texts (docs/05-organising.md, "The list of filters").
pub mod chip {
    /// `star: yes`.
    pub const STARRED: &str = "Starred";
    /// `star: no`.
    pub const NOT_STARRED: &str = "Not starred";
    /// `shape: portrait`.
    pub const PORTRAIT: &str = "Portrait";
    /// `shape: landscape`.
    pub const LANDSCAPE: &str = "Landscape";
    /// `hidden: show`.
    pub const HIDDEN_SHOWN: &str = "Hidden shown";
    /// `hidden: only`.
    pub const ONLY_HIDDEN: &str = "Only hidden";
    /// `has_location`.
    pub const HAS_LOCATION: &str = "Has location";
    /// `has_people`.
    pub const HAS_PEOPLE: &str = "Has people";
    /// `low_quality`.
    pub const LOW_QUALITY: &str = "Low quality";
    /// `near_duplicates`.
    pub const NEAR_DUPLICATES: &str = "Near-duplicates";

    /// The one chip of `from` and `to`: "{from} → {to}", "From {from}" or
    /// "Until {to}".
    pub fn dates(from: Option<&str>, to: Option<&str>) -> String {
        match (from, to) {
            (Some(from), Some(to)) => format!("{from} \u{2192} {to}"),
            (Some(from), None) => format!("From {from}"),
            (None, Some(to)) => format!("Until {to}"),
            (None, None) => String::new(),
        }
    }
}
