//! What the details panel shows of one photo's tags (docs/07-tagging.md,
//! "\"What's in it\" in the details panel"): the `PhotoTags` shape and how
//! Rust fills it from the photo's `tags` rows and this computer's choice.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::catalogue::{Catalogue, Part};
use super::results::ObjectCount;
use super::store::{self, Status, TagRow};
use crate::album::sqlite_error;
use crate::error::AppResult;
use crate::text;

/// Where a photo is with tagging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum TagsState {
    /// No chosen model, and no done row.
    Off,
    /// A chosen model has no row for the photo yet.
    Waiting,
    /// A chosen model's row is failed.
    Failed,
    Done,
}

/// One photo's tags as the details panel shows them. Vectors are never
/// sent to the page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PhotoTags {
    pub state: TagsState,
    pub description: Option<String>,
    /// The label of the model that wrote the description.
    pub written_by: Option<String>,
    /// Labels only, at most 12, best first, lower case.
    pub keywords: Vec<String>,
    pub objects: Vec<ObjectCount>,
    /// The first failed row's message, when the state is failed.
    pub error: Option<String>,
    pub can_regenerate: bool,
    /// The label of the first chosen model with no row yet, while waiting
    /// ("Waiting for {label}").
    pub waiting_for: Option<String>,
}

/// What [`photo_tags`] needs to know about this computer.
#[derive(Debug, Clone, Copy)]
pub struct TagsContext<'a> {
    pub catalogue: Catalogue,
    /// The chosen model id of each part (the settings), describe first.
    pub chosen: [Option<&'a str>; 2],
    /// The active describe model: chosen, installed and working.
    pub active_describe: Option<&'a str>,
    pub view_only: bool,
}

/// The newest done row of a part, of any model.
fn newest_done(conn: &Connection, id: i64, part: Part) -> AppResult<Option<TagRow>> {
    let model: Option<String> = conn
        .query_row(
            "SELECT model_id FROM tags WHERE photo_id = ?1 AND part = ?2 AND status = 'done'
             ORDER BY made_at DESC LIMIT 1",
            params![id, part.as_str()],
            |r| r.get(0),
        )
        .optional()
        .map_err(sqlite_error(text::photo::READING_DETAIL))?;
    match model {
        Some(m) => store::row(conn, id, &m),
        None => Ok(None),
    }
}

/// Fills `PhotoTags` for one photo by the table under "How Rust fills it".
pub fn photo_tags(conn: &Connection, id: i64, cx: &TagsContext<'_>) -> AppResult<PhotoTags> {
    let label = |model: &str| {
        cx.catalogue
            .get(model)
            .map_or_else(|| model.to_owned(), |m| m.label.to_owned())
    };
    let mut chosen_rows: [Option<TagRow>; 2] = [None, None];
    let mut shown: [Option<TagRow>; 2] = [None, None];
    for part in Part::ALL {
        let i = part.index();
        if let Some(m) = cx.chosen[i] {
            chosen_rows[i] = store::row(conn, id, m)?;
        }
        shown[i] = match &chosen_rows[i] {
            Some(r) => Some(r.clone()),
            None => newest_done(conn, id, part)?,
        };
    }
    let any_chosen = cx.chosen.iter().any(Option::is_some);
    let any_done = shown.iter().flatten().any(|r| r.status == Status::Done);
    let waiting_for = Part::ALL
        .iter()
        .find(|p| cx.chosen[p.index()].is_some() && chosen_rows[p.index()].is_none())
        .and_then(|p| cx.chosen[p.index()])
        .map(label);
    let failed_row = chosen_rows
        .iter()
        .flatten()
        .find(|r| r.status == Status::Failed);
    let state = if !any_chosen && !any_done {
        TagsState::Off
    } else if waiting_for.is_some() {
        TagsState::Waiting
    } else if failed_row.is_some() {
        TagsState::Failed
    } else {
        TagsState::Done
    };
    let done = |i: usize| shown[i].as_ref().filter(|r| r.status == Status::Done);
    let describe = done(Part::Describe.index());
    let mut keywords = done(Part::Keywords.index())
        .map(|r| r.keywords.clone())
        .unwrap_or_default();
    keywords.sort_by(|a, b| b.score.total_cmp(&a.score));
    Ok(PhotoTags {
        state,
        description: describe.and_then(|r| r.description.clone()),
        written_by: describe
            .filter(|r| r.description.is_some())
            .map(|r| label(&r.model_id)),
        keywords: keywords
            .into_iter()
            .map(|k| k.label.to_lowercase())
            .take(super::results::MAX_KEYWORDS)
            .collect(),
        objects: describe.map(|r| r.objects.clone()).unwrap_or_default(),
        error: if state == TagsState::Failed {
            failed_row.and_then(|r| r.error.clone())
        } else {
            None
        },
        can_regenerate: !cx.view_only
            && describe.is_some_and(|r| Some(r.model_id.as_str()) == cx.active_describe),
        waiting_for: if state == TagsState::Waiting {
            waiting_for
        } else {
            None
        },
    })
}
