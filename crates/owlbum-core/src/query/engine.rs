//! The query engine: runs queries and keeps their results under tokens.

use crate::album::Album;
use crate::error::AppResult;
use crate::scan::memory::Overlay;

use super::shape::{Keep, Located, Query, QueryResult, Row};

/// How many results are kept.
pub const TOKENS_KEPT: usize = 4;

/// The most rows one `query_window` answers.
pub const WINDOW_MAX: u32 = 500;

/// Runs queries for one open album and keeps the last results.
#[derive(Debug, Default)]
pub struct QueryEngine {}

impl QueryEngine {
    /// An engine holding no results.
    pub fn new() -> Self {
        Self::default()
    }

    /// `query_run`.
    pub fn run(
        &self,
        _album: &Album,
        _query: &Query,
        _keep: Option<&Keep>,
    ) -> AppResult<QueryResult> {
        unimplemented!()
    }

    /// `query_window`.
    pub fn window(
        &self,
        _album: &Album,
        _overlay: &Overlay,
        _token: &str,
        _offset: u32,
        _limit: u32,
    ) -> AppResult<Vec<Row>> {
        unimplemented!()
    }

    /// `query_locate`.
    pub fn locate(&self, _token: &str, _id: i64) -> AppResult<Option<Located>> {
        unimplemented!()
    }
}
