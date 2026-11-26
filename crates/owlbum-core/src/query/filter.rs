//! The list of filters (docs/05-organising.md, "The list of filters").
//!
//! One place drives what the doc's table drives: which photos pass
//! ([`Matcher`]), the chips ([`chips`]), "Filters · N" (the number of
//! chips: every filter with a panel group has one chip per value, and
//! search text has none), and the facets ([`pass`]).
//!
//! [`Matcher::new`] and [`chips`] take a [`Filter`] apart field by field
//! with no `..`, so a field added to `Filter` without an entry here does not
//! compile.
//!
//! Each filter that is on is one bit of a photo's "fails" mask. A photo
//! passes when the mask is empty. The facets come from the same mask in the
//! same pass: a photo that fails nothing counts toward the matching option
//! of every group, a photo that fails exactly one filter counts toward that
//! filter's group only, and a photo that fails two or more counts nowhere.

use std::cmp::Ordering;
use std::collections::HashSet;

use super::shape::{
    Chip, Facets, Filter, FolderFacet, HiddenFilter, ShapeFilter, StarFilter, TagFilter, TagKind,
};
use crate::album::{Album, sqlite_error};
use crate::error::{AppError, AppResult, ErrorCode};
use crate::index::{IndexView, Photo, parse_date};
use crate::scan::order::natural_cmp;
use crate::text;

const STAR: u16 = 1;
const SHAPE: u16 = 1 << 1;
const HIDDEN: u16 = 1 << 2;
const DATES: u16 = 1 << 3;
const FOLDER: u16 = 1 << 4;
const LOCATION: u16 = 1 << 5;
const HAS_PEOPLE: u16 = 1 << 6;
/// People and tags: one bit, as neither has a facet.
const CONTENT: u16 = 1 << 7;
const LOW_QUALITY: u16 = 1 << 8;
const NEAR_DUPLICATES: u16 = 1 << 9;

/// The photo ids of the people and tags of a filter: those with every
/// listed person and every listed tag. None when neither is on.
pub(super) type Content = Option<HashSet<u32>>;

/// Refuses a filter value that is not allowed: a date that is not a real
/// calendar day `YYYY-MM-DD`, or a blank person or tag.
pub(super) fn validate(filter: &Filter) -> AppResult<()> {
    let bad = |what: &str| {
        Err(AppError::new(
            ErrorCode::BadQuery,
            text::query::bad_value(what),
        ))
    };
    for day in [&filter.from, &filter.to].into_iter().flatten() {
        if day_of(day).is_none() {
            return bad(day);
        }
    }
    for person in filter.people.iter().flatten() {
        if person.trim().is_empty() {
            return bad(person);
        }
    }
    for tag in filter.tags.iter().flatten() {
        if tag.value.trim().is_empty() {
            return bad(&tag.value);
        }
    }
    Ok(())
}

/// A `YYYY-MM-DD` day as days from 1970-01-01; None when it is not a real
/// calendar day.
fn day_of(day: &str) -> Option<i64> {
    if day.len() != 10 {
        return None;
    }
    parse_date(&format!("{day}T00:00:00")).map(|s| s.div_euclid(86_400))
}

/// Looks up the photo ids of the filter's people and tags (docs/05-organising.md,
/// "How Rust runs a query", step 2). People match ignoring case (the
/// `people.name` column is `NOCASE`); a tag's value is trimmed and
/// lower-cased and must equal a whole label.
pub(super) fn content(album: &Album, filter: &Filter) -> AppResult<Content> {
    let people = filter.people.as_deref().unwrap_or_default();
    let tags = filter.tags.as_deref().unwrap_or_default();
    if people.is_empty() && tags.is_empty() {
        return Ok(None);
    }
    album.db().read(|conn| {
        let failed = sqlite_error(text::query::READING_ROWS);
        let mut all: Option<HashSet<u32>> = None;
        let mut keep = |ids: HashSet<u32>| {
            all = Some(match all.take() {
                None => ids,
                Some(had) => had.intersection(&ids).copied().collect(),
            });
        };
        let mut person = conn
            .prepare_cached(
                "SELECT pp.photo_id FROM photo_people pp JOIN people pe ON pe.id = pp.person_id
                  WHERE pe.name = ?1",
            )
            .map_err(&failed)?;
        for name in people {
            keep(ids(&mut person, (name.trim(),)).map_err(&failed)?);
        }
        let mut label = conn
            .prepare_cached("SELECT photo_id FROM photo_labels WHERE kind = ?1 AND label = ?2")
            .map_err(&failed)?;
        for TagFilter { kind, value } in tags {
            let kind = match kind {
                TagKind::Keyword => "keyword",
                TagKind::Object => "object",
            };
            keep(ids(&mut label, (kind, value.trim().to_lowercase())).map_err(&failed)?);
        }
        Ok(all)
    })
}

fn ids(
    stmt: &mut rusqlite::CachedStatement<'_>,
    params: impl rusqlite::Params,
) -> rusqlite::Result<HashSet<u32>> {
    stmt.query_map(params, |r| r.get::<_, i64>(0))?
        .map(|id| id.map(|id| u32::try_from(id).ok()))
        .filter_map(Result::transpose)
        .collect()
}

/// Which folder the folder filter asks for.
#[derive(Clone, Copy)]
enum FolderWant {
    Any,
    /// A folder of the index.
    Is(u32),
    /// A folder no photo is in: nothing matches.
    Nowhere,
}

/// Decides which filters a photo fails.
pub(super) struct Matcher<'a> {
    star: Option<StarFilter>,
    shape: Option<ShapeFilter>,
    hidden: Option<HiddenFilter>,
    /// The first and last day, inclusive, as days from 1970-01-01.
    days: Option<(i64, i64)>,
    folder: FolderWant,
    has_location: bool,
    has_people: bool,
    content: Option<&'a HashSet<u32>>,
    low_quality: bool,
    near_duplicates: bool,
}

impl<'a> Matcher<'a> {
    /// The matcher for a [`validate`]d, normalised `filter` over `view`.
    pub(super) fn new(view: &IndexView<'_>, filter: &Filter, content: &'a Content) -> Self {
        let Filter {
            star,
            shape,
            hidden,
            from,
            to,
            folder,
            has_location,
            has_people,
            people: _,
            tags: _,
            low_quality,
            near_duplicates,
        } = filter;
        let from_day = from.as_deref().and_then(day_of);
        let to_day = to.as_deref().and_then(day_of);
        let days = (from_day.is_some() || to_day.is_some())
            .then(|| (from_day.unwrap_or(i64::MIN), to_day.unwrap_or(i64::MAX)));
        let folder = match folder {
            None => FolderWant::Any,
            Some(want) => (0..view.folder_count())
                .filter_map(|i| u32::try_from(i).ok())
                .find(|&i| view.folder(i) == want)
                .map_or(FolderWant::Nowhere, FolderWant::Is),
        };
        Self {
            star: *star,
            shape: *shape,
            hidden: *hidden,
            days,
            folder,
            has_location: has_location.unwrap_or(false),
            has_people: has_people.unwrap_or(false),
            content: content.as_ref(),
            low_quality: low_quality.unwrap_or(false),
            near_duplicates: near_duplicates.unwrap_or(false),
        }
    }

    /// The filters `p` fails, one bit each.
    fn fails(&self, p: &Photo) -> u16 {
        let mut fails = 0;
        let mut fail = |bit: u16, passes: bool| {
            if !passes {
                fails |= bit;
            }
        };
        if let Some(star) = self.star {
            fail(STAR, p.star() == (star == StarFilter::Yes));
        }
        if let Some(shape) = self.shape {
            fail(SHAPE, shape_of(p) == Some(shape));
        }
        match self.hidden {
            None => fail(HIDDEN, !p.hidden()),
            Some(HiddenFilter::Show) => {}
            Some(HiddenFilter::Only) => fail(HIDDEN, p.hidden()),
        }
        if let Some((from, to)) = self.days {
            fail(
                DATES,
                p.date()
                    .map(|d| d.div_euclid(86_400))
                    .is_some_and(|day| from <= day && day <= to),
            );
        }
        match self.folder {
            FolderWant::Any => {}
            FolderWant::Is(f) => fail(FOLDER, p.folder == f),
            FolderWant::Nowhere => fail(FOLDER, false),
        }
        if self.has_location {
            fail(LOCATION, p.has_location());
        }
        if self.has_people {
            fail(HAS_PEOPLE, p.people > 0);
        }
        if let Some(ids) = self.content {
            fail(CONTENT, ids.contains(&p.id));
        }
        if self.low_quality {
            fail(LOW_QUALITY, p.derived.quality_flags != 0);
        }
        if self.near_duplicates {
            fail(NEAR_DUPLICATES, p.derived.burst.is_some());
        }
        fails
    }
}

/// Portrait when taller than wide, landscape when wider than tall; None
/// for a square photo or unknown dimensions.
fn shape_of(p: &Photo) -> Option<ShapeFilter> {
    if p.width == 0 || p.height == 0 {
        return None;
    }
    match p.height.cmp(&p.width) {
        Ordering::Greater => Some(ShapeFilter::Portrait),
        Ordering::Less => Some(ShapeFilter::Landscape),
        Ordering::Equal => None,
    }
}

/// What one pass over the album index gives.
pub(super) struct Passed<'v> {
    /// The photos that pass, in id order.
    pub shown: Vec<&'v Photo>,
    /// Y: every photo in the scope, hidden ones only when the Hidden
    /// filter is `show` or `only`.
    pub total: usize,
    /// The facets, when asked for.
    pub facets: Option<Facets>,
}

/// Runs `matcher` over every photo of `view`, counting the facets in the
/// same pass when `want_facets`.
pub(super) fn pass<'v>(
    view: &'v IndexView<'_>,
    matcher: &Matcher<'_>,
    want_facets: bool,
) -> Passed<'v> {
    let photos = view.photos();
    let mut shown = Vec::with_capacity(photos.len());
    let mut total = 0;
    let mut f = Facets::default();
    let mut folders = if want_facets {
        vec![None::<u32>; view.folder_count()]
    } else {
        Vec::new()
    };
    for p in photos {
        if matcher.hidden.is_some() || !p.hidden() {
            total += 1;
        }
        let fails = matcher.fails(p);
        if fails == 0 {
            shown.push(p);
        }
        if !want_facets {
            continue;
        }
        if let Some(n) = folders.get_mut(p.folder as usize) {
            // Listed whatever it counts: the folder holds a photo.
            let n = n.get_or_insert(0);
            if fails & !FOLDER == 0 {
                *n += 1;
            }
        }
        // `counts(bit)`: the photo counts toward the group of `bit`.
        let counts = |bit: u16| fails & !bit == 0;
        if counts(STAR) {
            if p.star() {
                f.star.yes += 1;
            } else {
                f.star.no += 1;
            }
        }
        if counts(SHAPE) {
            match shape_of(p) {
                Some(ShapeFilter::Portrait) => f.shape.portrait += 1,
                Some(ShapeFilter::Landscape) => f.shape.landscape += 1,
                None => {}
            }
        }
        if counts(HIDDEN) && p.hidden() {
            f.hidden += 1;
        }
        if counts(LOCATION) && p.has_location() {
            f.has_location += 1;
        }
        if counts(HAS_PEOPLE) && p.people > 0 {
            f.has_people += 1;
        }
        if counts(LOW_QUALITY) && p.derived.quality_flags != 0 {
            f.low_quality += 1;
        }
        if counts(NEAR_DUPLICATES) && p.derived.burst.is_some() {
            f.near_duplicates += 1;
        }
    }
    let facets = want_facets.then(|| {
        f.folders = folder_facets(view, &folders);
        f
    });
    Passed {
        shown,
        total,
        facets,
    }
}

/// The folder list: the folders that hold a photo, the top folder first
/// and the rest in the natural order.
fn folder_facets(view: &IndexView<'_>, counts: &[Option<u32>]) -> Vec<FolderFacet> {
    let top = view.scope();
    let mut list: Vec<FolderFacet> = counts
        .iter()
        .enumerate()
        .filter_map(|(i, n)| {
            let count = (*n)?;
            let folder = view.folder(u32::try_from(i).ok()?).to_owned();
            Some(FolderFacet {
                label: folder_label(&folder, top),
                folder,
                count,
            })
        })
        .collect();
    list.sort_by(|a, b| {
        (a.folder != top)
            .cmp(&(b.folder != top))
            .then_with(|| natural_cmp(&a.folder, &b.folder))
    });
    list
}

/// "Top folder" for the scope's own folder, else the path.
fn folder_label(folder: &str, top: &str) -> String {
    if folder == top {
        text::query::TOP_FOLDER.to_owned()
    } else {
        folder.to_owned()
    }
}

/// One chip per filter that is on, in the order of the list of filters.
/// `filter` is normalised; `top` is the scope's folder.
pub(super) fn chips(filter: &Filter, top: &str) -> Vec<Chip> {
    let Filter {
        star,
        shape,
        hidden,
        from,
        to,
        folder,
        has_location,
        has_people,
        people,
        tags,
        low_quality,
        near_duplicates,
    } = filter;
    use text::query::chip as t;
    let mut chips = Vec::new();
    let mut chip = |id: String, text: String, remove: &dyn Fn(&mut Filter)| {
        let mut without = filter.clone();
        remove(&mut without);
        chips.push(Chip {
            id,
            text,
            remove: without.normalised(),
        });
    };
    if let Some(star) = star {
        let text = match star {
            StarFilter::Yes => t::STARRED,
            StarFilter::No => t::NOT_STARRED,
        };
        chip("star".into(), text.into(), &|f| f.star = None);
    }
    if let Some(shape) = shape {
        let text = match shape {
            ShapeFilter::Portrait => t::PORTRAIT,
            ShapeFilter::Landscape => t::LANDSCAPE,
        };
        chip("shape".into(), text.into(), &|f| f.shape = None);
    }
    if let Some(hidden) = hidden {
        let text = match hidden {
            HiddenFilter::Show => t::HIDDEN_SHOWN,
            HiddenFilter::Only => t::ONLY_HIDDEN,
        };
        chip("hidden".into(), text.into(), &|f| f.hidden = None);
    }
    if from.is_some() || to.is_some() {
        chip(
            "dates".into(),
            t::dates(from.as_deref(), to.as_deref()),
            &|f| {
                f.from = None;
                f.to = None;
            },
        );
    }
    if let Some(folder) = folder {
        chip("folder".into(), folder_label(folder, top), &|f| {
            f.folder = None
        });
    }
    if has_location.is_some() {
        chip("has_location".into(), t::HAS_LOCATION.into(), &|f| {
            f.has_location = None
        });
    }
    if has_people.is_some() {
        chip("has_people".into(), t::HAS_PEOPLE.into(), &|f| {
            f.has_people = None
        });
    }
    for (i, name) in people.iter().flatten().enumerate() {
        chip(format!("person:{name}"), name.clone(), &|f| {
            if let Some(p) = f.people.as_mut() {
                p.remove(i);
            }
        });
    }
    for (i, tag) in tags.iter().flatten().enumerate() {
        let kind = match tag.kind {
            TagKind::Keyword => "keyword",
            TagKind::Object => "object",
        };
        chip(
            format!("tag:{kind}:{}", tag.value),
            tag.value.clone(),
            &|f| {
                if let Some(t) = f.tags.as_mut() {
                    t.remove(i);
                }
            },
        );
    }
    if low_quality.is_some() {
        chip("low_quality".into(), t::LOW_QUALITY.into(), &|f| {
            f.low_quality = None
        });
    }
    if near_duplicates.is_some() {
        chip("near_duplicates".into(), t::NEAR_DUPLICATES.into(), &|f| {
            f.near_duplicates = None;
        });
    }
    chips
}
