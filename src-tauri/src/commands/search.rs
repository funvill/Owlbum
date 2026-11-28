//! Search by meaning (`search_`): docs/07-tagging.md, "Search by meaning".

use owlbum_core::tagging::meaning::{MeaningResult, MeaningState};
use owlbum_core::text;
use tauri::{AppHandle, Runtime, State};

use crate::commands::off_main;
use crate::commands::tagging::tagging;
use crate::session::Session;
use crate::{area, command};

area! {
    prefix: "search",
    commands: [search_meaning],
    events: [],
}

command! {
    /// Appends the photos close in meaning to the search text of the
    /// result `token` names to the end of its list, as its meaning segment,
    /// and answers `{ state, count?, photos?, meaning_start }`. `state` is
    /// `ok`, `too_short`, `off` or `unavailable`; only `ok` gives counts.
    /// `query_expired` for a token that was dropped; `no_album` when no
    /// album is open.
    pub async fn search_meaning<R: Runtime>(token: String; app: AppHandle<R>, session: State<'_, Session>) -> MeaningResult {
        let (scanner, queries) = session.queries()?;
        let Some(t) = tagging(&app) else {
            return Ok(MeaningResult::nothing(MeaningState::Off));
        };
        off_main(text::query::READING_ROWS, move || {
            t.search_meaning(scanner.album(), &queries, &token)
        })
        .await
    }
}
