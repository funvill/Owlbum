//! What the engine runs so far, and the check that refuses the rest.
//!
//! The query engine (#13) runs the grid, the timeline (#35) and the map,
//! in any of the five sorts (#29), with every filter, its chips and its
//! facets (#31), and search text (#32). The other parts of a [`Query`]
//! belong to later issues; each is one entry below, so the issue that
//! builds it removes its entry:
//!
//! | Part | Issue | Until then |
//! |---|---|---|
//! | `stack`, `open_stacks` | #30 | accepted; nothing folds (no bursts yet), unknown ids are ignored |
//!
//! Values the doc treats as absent (`false`, `null`, empty lists) are
//! absent here too, so the default page query always runs.

use super::shape::Query;
use crate::error::AppResult;

/// The fewest letters or digits search text needs to be a filter
/// (docs/05-organising.md, "Matching").
pub const SEARCH_MIN: usize = 2;

/// True when `search` is a filter: it has at least [`SEARCH_MIN`] letters
/// or digits.
pub fn is_search(search: Option<&str>) -> bool {
    search.is_some_and(|s| s.chars().filter(|c| c.is_alphanumeric()).count() >= SEARCH_MIN)
}

/// Refuses a query that asks for a part the engine does not run yet. Every
/// part runs now; the check stays for the next part that does not.
pub fn check(_query: &Query) -> AppResult<()> {
    Ok(())
}
