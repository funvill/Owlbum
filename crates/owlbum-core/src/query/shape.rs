//! The shapes the page and the query engine exchange
//! (docs/05-organising.md, "The `Query`" and "`query_run`"). They are
//! defined here and nowhere else; the page's types are generated from them.
//!
//! The whole shape is declared, including the parts the engine does not run
//! yet: [`super::support`] says which parts those are, and a query that
//! asks for one answers `bad_query`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::media::MediaKind;

/// How the result is shown.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum QueryView {
    /// The grid: sort and stack as asked.
    #[default]
    Grid,
    /// The timeline: the sort is always `date`; the answer carries `groups`.
    Timeline,
    /// The map: every photo that passes, in the chosen sort, never stacked.
    Map,
}

/// The timeline's grouping. Used only by the timeline view.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum QueryTimelineUnit {
    /// Month when the dated photos span more than 90 days, else Day.
    #[default]
    Auto,
    /// One heading per day.
    Day,
    /// One heading per month.
    Month,
}

/// The unit a timeline result actually used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum GroupUnit {
    /// One heading per day.
    Day,
    /// One heading per month.
    Month,
}

/// The order of the result (docs/05-organising.md, "Sorting").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum QuerySort {
    /// Effective date, oldest first; photos with no date last.
    #[default]
    Date,
    /// Highest photo score first.
    Score,
    /// Analysed photos first, then most interesting first.
    Interesting,
    /// By main colour, then lightness.
    Colour,
    /// The name order.
    Name,
}

/// What the page sends to `query_run`.
///
/// Its bindings file is `QueryShape.ts`: `Query.ts` would be the same file
/// as the area's `query.ts` on a file system that ignores case.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export_to = "QueryShape.ts")]
pub struct Query {
    /// How the result is shown.
    pub view: QueryView,
    /// The timeline's grouping; ignored by the other views.
    pub timeline_unit: QueryTimelineUnit,
    /// The order.
    pub sort: QuerySort,
    /// Stack similar.
    pub stack: bool,
    /// Ids of the stacks shown unfolded.
    pub open_stacks: Vec<i64>,
    /// The filter. A filter that is off is absent.
    pub filter: Filter,
    /// The search text, as `query_parse` returned it.
    pub search: Option<String>,
    /// True while the filter panel is open.
    pub want_facets: bool,
}

/// Star filter values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum StarFilter {
    /// Starred.
    Yes,
    /// Not starred.
    No,
}

/// Shape filter values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ShapeFilter {
    /// `height > width`.
    Portrait,
    /// `width > height`.
    Landscape,
}

/// Hidden filter values. Absent: hidden photos are left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum HiddenFilter {
    /// Every photo passes.
    Show,
    /// Only hidden photos pass.
    Only,
}

/// Which kind of label a tag filter names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum TagKind {
    /// A keyword.
    Keyword,
    /// An object label.
    Object,
}

/// One keyword or object a photo must have.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct TagFilter {
    /// Keyword or object.
    pub kind: TagKind,
    /// The label as typed.
    pub value: String,
}

/// The filters (docs/05-organising.md, "The list of filters"). A filter
/// that is off is absent; `false`, `null` and empty lists count as absent.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct Filter {
    /// Starred or not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub star: Option<StarFilter>,
    /// Portrait or landscape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub shape: Option<ShapeFilter>,
    /// Show hidden photos, or only them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub hidden: Option<HiddenFilter>,
    /// The first day, `YYYY-MM-DD`, inclusive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub from: Option<String>,
    /// The last day, `YYYY-MM-DD`, inclusive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub to: Option<String>,
    /// A folder relative to the album root; `""` is the root itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub folder: Option<String>,
    /// Has a place.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub has_location: Option<bool>,
    /// Has at least one person.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub has_people: Option<bool>,
    /// Every listed person is on the photo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub people: Option<Vec<String>>,
    /// Every listed tag is present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tags: Option<Vec<TagFilter>>,
    /// At least one quality flag is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub low_quality: Option<bool>,
    /// In a burst.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub near_duplicates: Option<bool>,
}

impl Filter {
    /// The filter with every off value (`false`, an empty list) taken out,
    /// so "absent" has one form.
    #[must_use]
    pub fn normalised(&self) -> Self {
        let on = |b: Option<bool>| b.filter(|&b| b);
        Self {
            has_location: on(self.has_location),
            has_people: on(self.has_people),
            low_quality: on(self.low_quality),
            near_duplicates: on(self.near_duplicates),
            people: self.people.clone().filter(|p| !p.is_empty()),
            tags: self.tags.clone().filter(|t| !t.is_empty()),
            ..self.clone()
        }
    }

    /// True when no filter is on.
    pub fn is_empty(&self) -> bool {
        self.normalised() == Self::default()
    }
}

/// A photo and its place in a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct PhotoAt {
    /// The photo's id.
    pub id: i64,
    /// Its index in the result's tile list.
    pub index: u32,
}

/// The optional second argument of `query_run`: what to follow from the
/// result the page was showing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Keep {
    /// The token of that result.
    pub token: String,
    /// The current photo.
    pub current: Option<PhotoAt>,
    /// Where a Shift range starts.
    pub anchor_id: Option<i64>,
    /// Every selected photo id.
    pub selection: Vec<i64>,
}

/// Where the current photo and the selection landed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Kept {
    /// The new current photo; null when the result is empty (or there was
    /// none).
    pub current: Option<PhotoAt>,
    /// The anchor; the current photo when the old anchor is gone.
    pub anchor: Option<PhotoAt>,
    /// The old selection limited to photos still shown, plus the new
    /// current photo.
    pub selection: Vec<i64>,
}

/// One timeline heading.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Group {
    /// `2026-07-14`, `2026-07` or `none`.
    pub key: String,
    /// Index of the group's first tile.
    pub start: u32,
    /// Tiles in the group.
    pub count: u32,
    /// Photos in the group, stacks unfolded.
    pub photos: u32,
}

/// One filter chip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Chip {
    /// Stable: `star`, `dates`, `tag:keyword:lake`, `person:Ann`, ...
    pub id: String,
    /// The chip's text.
    pub text: String,
    /// The whole filter as it would be with this chip removed.
    pub remove: Filter,
}

/// Facet counts for the star select.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct StarFacets {
    /// Starred.
    pub yes: u32,
    /// Not starred.
    pub no: u32,
}

/// Facet counts for the shape select.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ShapeFacets {
    /// Portrait.
    pub portrait: u32,
    /// Landscape.
    pub landscape: u32,
}

/// The facet count of one folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FolderFacet {
    /// The folder, relative to the album root.
    pub folder: String,
    /// "Top folder" or the path.
    pub label: String,
    /// Photos.
    pub count: u32,
}

/// The counts beside the filter panel's options.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Facets {
    /// Stars.
    pub star: StarFacets,
    /// Shape.
    pub shape: ShapeFacets,
    /// Hidden photos passing the other filters.
    pub hidden: u32,
    /// Folders.
    pub folders: Vec<FolderFacet>,
    /// Has location.
    pub has_location: u32,
    /// Has people.
    pub has_people: u32,
    /// Low quality.
    pub low_quality: u32,
    /// Near-duplicates.
    pub near_duplicates: u32,
}

/// What `query_run` answers.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct QueryResult {
    /// Names this result for `query_window`, `query_locate` and the rest.
    pub token: String,
    /// The album revision this result was built from.
    pub revision: u64,
    /// Tiles: what `query_window` pages through.
    pub count: u32,
    /// X in "Showing X of Y".
    pub photos: u32,
    /// Y, and the "N photos" figure.
    pub total: u32,
    /// Photos in the scope that are not missing and could be read, hidden
    /// ones included.
    pub album_photos: u32,
    /// Is anything filtered.
    pub filtered: bool,
    /// N in "Filters · N".
    pub panel_count: u32,
    /// One chip per filter that is on.
    pub chips: Vec<Chip>,
    /// Timeline view only: one entry per heading, in order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub groups: Option<Vec<Group>>,
    /// Timeline view only: the unit actually used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub unit: Option<GroupUnit>,
    /// Where the current photo and the selection landed; null when `keep`
    /// was not passed.
    pub kept: Option<Kept>,
    /// Present when `want_facets` was true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub facets: Option<Facets>,
}

/// A tile's stack, on a [`Row`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RowStack {
    /// The stack's id.
    pub id: i64,
    /// Members passing the filter.
    pub size: u32,
    /// The stack is shown unfolded.
    pub open: bool,
    /// This tile is the face.
    pub face: bool,
    /// This photo's position in the stack, from 1.
    pub place: u32,
}

/// The record `query_window` returns for each tile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Row {
    /// The photo's id.
    pub id: i64,
    /// Relative to the album root, `/` separators.
    pub rel_path: String,
    /// The file name.
    pub file_name: String,
    /// Builds the `owlbum://thumb` addresses.
    pub thumb_key: String,
    /// Width / height, upright; 1 when the dimensions are not known yet.
    pub aspect: f64,
    /// Image, RAW or video.
    pub kind: MediaKind,
    /// Video length; on kind `image` it marks an animated picture.
    pub duration_ms: Option<u64>,
    /// Starred.
    pub star: bool,
    /// Hidden.
    pub hidden: bool,
    /// The photo score; null draws as "–" (not analysed yet).
    pub score: Option<i32>,
    /// The tile's stack, if it is in one.
    pub stack: Option<RowStack>,
    /// The plan for the stored edit (docs/06-editing-and-file-actions.md).
    /// Always null until the edit model exists (#43), which gives it its
    /// type.
    pub edit_plan: Option<serde_json::Value>,
}

/// What `query_locate` answers when the photo is in the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum Located {
    /// The photo is a tile at this index.
    At {
        /// The tile's index.
        index: u32,
    },
    /// The photo is folded inside a closed stack.
    Folded {
        /// The id of the closed stack that holds it.
        folded_in: i64,
    },
}
