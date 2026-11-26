//! Ask for rows (`query_`): docs/05-organising.md, "Commands and events".

use crate::session::Session;
use crate::{area, command};
use owlbum_core::query::{Arg, Keep, Located, Query, QueryResult, Row};
use owlbum_core::{AppError, text};
use tauri::State;

area! {
    prefix: "query",
    commands: [query_locate, query_run, query_window],
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
        blocking(move || queries.run(scanner.album(), &query, keep.as_ref())).await
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
