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
pub const SORT_SCORE: &str = "photos sorted by score";
/// See [`TIMELINE`].
pub const SORT_INTERESTING: &str = "photos sorted by how interesting they are";
/// See [`TIMELINE`].
pub const SORT_COLOUR: &str = "photos sorted by colour";
/// See [`TIMELINE`].
pub const FILTERS: &str = "filtered photos";
/// See [`TIMELINE`].
pub const SEARCH: &str = "search results";
/// See [`TIMELINE`].
pub const FACETS: &str = "counts beside the filters";
