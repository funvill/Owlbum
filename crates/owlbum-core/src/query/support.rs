//! What the engine runs so far, and the check that refuses the rest.
//!
//! The query engine (#13) runs the grid and the map, in any of the five
//! sorts (#29), with every filter, its chips and its facets (#31). The
//! other parts of a [`Query`] belong to later issues; each is one entry
//! below, so the issue that builds it removes its entry:
//!
//! | Part | Issue | Until then |
//! |---|---|---|
//! | `view: timeline` (groups, unit) | #35 | `bad_query` |
//! | Search text of the minimum length | #32 | `bad_query`; shorter text is ignored, as the doc says |
//! | `stack`, `open_stacks` | #30 | accepted; nothing folds (no bursts yet), unknown ids are ignored |
//!
//! Values the doc treats as absent (`false`, `null`, empty lists) are
//! absent here too, so the default page query always runs.

use super::shape::{Query, QueryView};
use crate::error::{AppError, AppResult, ErrorCode};
use crate::text;

/// The fewest letters or digits search text needs to be a filter
/// (docs/05-organising.md, "Matching").
pub const SEARCH_MIN: usize = 2;

/// True when `search` is a filter: it has at least [`SEARCH_MIN`] letters
/// or digits.
pub fn is_search(search: Option<&str>) -> bool {
    search.is_some_and(|s| s.chars().filter(|c| c.is_alphanumeric()).count() >= SEARCH_MIN)
}

/// Refuses a query that asks for a part the engine does not run yet.
pub fn check(query: &Query) -> AppResult<()> {
    let refuse = |what: &str| {
        Err(AppError::new(
            ErrorCode::BadQuery,
            text::query::not_yet(what),
        ))
    };
    if query.view == QueryView::Timeline {
        return refuse(text::query::TIMELINE);
    }
    if is_search(query.search.as_deref()) {
        return refuse(text::query::SEARCH);
    }
    Ok(())
}
