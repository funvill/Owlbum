//! Ask for rows (`query_`): docs/05-organising.md, "Commands and events".

use crate::session::Session;
use crate::{area, command};
use owlbum_core::query::{
    Arg, Facets, Filter, Keep, Located, MapPoints, ParsedBox, Query, QueryResult, Row, parse_box,
};
use owlbum_core::{AppError, text};
use tauri::State;

area! {
    prefix: "query",
    commands: [query_facets, query_ids, query_locate, query_map, query_parse, query_run, query_window],
    events: [],
}

command! {
    /// Runs a query and keeps its ordered list under a new token. `keep`
    /// names the current photo and selection to follow from the result the
    /// page was showing. `bad_query` for a value that is not allowed, or a
    /// part of the query that is not built yet; `no_album` when no album is
    /// open.
    pub async fn query_run(query: Arg<Query>, keep: Option<Arg<Keep>>; session: State<'_, Session>) -> QueryResult {
        let query = query.get()?;
        let keep = keep.map(Arg::get).transpose()?;
        let (scanner, queries) = session.queries()?;
        blocking(move || {
            let overlay = scanner.overlay();
            queries.run_with(scanner.album(), &overlay, &query, keep.as_ref())
        })
        .await
    }
}

command! {
    /// Reads the search box: turns its `key:value` words into filters and
    /// answers what the box should hold and the search text. `enter` is
    /// true when Enter was pressed, so the unfinished last word is read
    /// too. `filter` is the filter the page holds, for the order of `from`
    /// and `to` and for tags and people already in it. `no_album` when no
    /// album is open.
    pub async fn query_parse(text: String, enter: bool, filter: Option<Arg<Filter>>; session: State<'_, Session>) -> ParsedBox {
        let filter = filter.map(Arg::get).transpose()?.unwrap_or_default();
        let (scanner, _) = session.queries()?;
        blocking(move || parse_box(scanner.album(), &text, enter, &filter)).await
    }
}

command! {
    /// The counts beside the filter panel's options for the query `token`
    /// was made from. `query_expired` for a token that was dropped.
    pub async fn query_facets(token: String; session: State<'_, Session>) -> Facets {
        let (scanner, queries) = session.queries()?;
        blocking(move || queries.facets(scanner.album(), &token)).await
    }
}

command! {
    /// The rows at `offset` of a result, at most `limit` and at most 500.
    /// Fewer rows than asked only at the end. `query_expired` for a token
    /// that was dropped.
    pub async fn query_window(token: String, offset: u32, limit: u32; session: State<'_, Session>) -> Vec<Row> {
        let (scanner, queries) = session.queries()?;
        blocking(move || {
            let overlay = scanner.overlay();
            queries.window(scanner.album(), &overlay, &token, offset, limit)
        })
        .await
    }
}

command! {
    /// Where a photo sits in a result: `{ index }`, `{ folded_in }` (the
    /// closed stack that holds it), or null when it is not in the result.
    /// `query_expired` for a token that was dropped.
    pub fn query_locate(token: String, id: i64; session: State<'_, Session>) -> Option<Located> {
        let (_, queries) = session.queries()?;
        queries.locate(&token, id)
    }
}

command! {
    /// The ids of the tiles at `offset` of a result, at most `limit`, in
    /// order: one per tile, so a closed stack's face and not its folded
    /// members. No cap on `limit`. `query_expired` for a token that was
    /// dropped.
    pub fn query_ids(token: String, offset: u32, limit: u32; session: State<'_, Session>) -> Vec<i64> {
        let (_, queries) = session.queries()?;
        queries.ids(&token, offset, limit)
    }
}

command! {
    /// Every located photo of a result made with `view: "map"`, in its
    /// order, as compact `[id, lat, lon, thumb_key, file_name]` entries,
    /// and how many photos of it have no place. `query_expired` for a
    /// token that was dropped.
    pub async fn query_map(token: String; session: State<'_, Session>) -> MapPoints {
        let (scanner, queries) = session.queries()?;
        blocking(move || {
            let overlay = scanner.overlay();
            queries.map(scanner.album(), &overlay, &token)
        })
        .await
    }
}

/// Runs `job` on the blocking pool: it reads the album index and database.
async fn blocking<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tauri::async_runtime::spawn_blocking(job)
        .await
        .map_err(|e| {
            AppError::internal(text::query::READING_ROWS)
                .with_details(serde_json::json!({ "cause": e.to_string() }))
        })?
}
