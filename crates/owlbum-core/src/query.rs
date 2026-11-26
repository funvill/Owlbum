//! The query engine (docs/05-organising.md, "The `Query`";
//! docs/01-architecture.md, "The query model").
//!
//! Opening a result is two steps, so a whole album never crosses to the
//! page at once. [`QueryEngine::run`] works out the ordered list of tiles
//! over the album index and keeps it under a token; the page then asks for
//! the rows it is about to show with [`QueryEngine::window`], and finds a
//! photo's place with [`QueryEngine::locate`]. The engine keeps the last
//! [`TOKENS_KEPT`] results; an older token answers `query_expired`.
//!
//! The whole [`Query`] shape is declared in [`shape`]. What the engine
//! runs so far is listed in [`support`]: a query that asks for anything
//! else answers `bad_query`, so a missing feature never passes for an
//! empty result.

pub mod arg;
mod engine;
mod filter;
mod keep;
pub mod shape;
pub mod sort;
pub mod support;

pub use arg::Arg;
pub use engine::{QueryEngine, TOKENS_KEPT, WINDOW_MAX};
pub use keep::settle;
pub use shape::{
    Chip, Facets, Filter, FolderFacet, Group, GroupUnit, HiddenFilter, Keep, Kept, Located,
    PhotoAt, Query, QueryResult, QuerySort, QueryTimelineUnit, QueryView, Row, RowStack,
    ShapeFacets, ShapeFilter, StarFacets, StarFilter, TagFilter, TagKind,
};

#[cfg(test)]
mod tests;
