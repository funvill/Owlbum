//! Typed info: description, notes, people, date and place
//! (`photo_set_info`, docs/04-browsing.md, "How typed fields save"), star
//! and hidden (docs/05-organising.md, "Star and hide"), and
//! the album's people for the People field's suggestions (`album_people`,
//! docs/05-organising.md, "People suggestions").
//!
//! The page checks what is typed before it sends it; Rust checks again
//! here, so a value that breaks a rule is never stored. Every write goes
//! through [`Album::write_photos`], so the album index and the full-text
//! `search` table follow it in the same transaction.

use std::cmp::Ordering;
use std::collections::HashMap;

use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Deserializer, Serialize};
use ts_rs::TS;

use crate::album::undo::Step;
use crate::album::{Album, search, sqlite_error};
use crate::appdata::clock::now_ms;
use crate::error::{AppError, AppResult, ErrorCode};
use crate::scan::natural_cmp_text;
use crate::scan::rows::{self, By};
use crate::text;
use crate::thumbs::lookup::servable;

/// The longest name a person can have, in characters.
pub const MAX_NAME_CHARS: usize = 100;

/// Reads a field that may be absent (outer `None`: leave it alone) or null
/// (inner `None`: clear it).
fn some<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// A typed place: coordinates and an optional name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct TypedLocation {
    pub lat: f64,
    pub lon: f64,
    pub name: Option<String>,
}

/// The fields `photo_set_info` changes. A field left out is left alone; a
/// null clears it. `star` and `hidden` cannot be null.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct InfoPatch {
    #[serde(
        default,
        deserialize_with = "some",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional = nullable)]
    pub description: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "some",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional = nullable)]
    pub notes: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "some",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional = nullable)]
    pub people: Option<Option<Vec<String>>>,
    /// `YYYY-MM-DDTHH:MM:SS`; the shorter forms the page accepts are
    /// accepted here too.
    #[serde(
        default,
        deserialize_with = "some",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional = nullable)]
    pub date_override: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "some",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional = nullable)]
    pub location: Option<Option<TypedLocation>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub star: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub hidden: Option<bool>,
}

/// `photo_set_info`'s answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct InfoChanged {
    /// The photos whose values changed.
    pub changed: Vec<i64>,
    /// The album's revision after the change; unchanged when nothing changed.
    pub revision: u64,
}

/// One photo's typed info as it was before a `photo_set_info` changed it:
/// what an undo step holds for that photo (docs/05-organising.md, "Undo").
#[derive(Debug, Clone, PartialEq)]
pub struct PriorInfo {
    pub id: i64,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub date_override: Option<String>,
    pub loc: Option<(f64, f64)>,
    pub loc_name: Option<String>,
    pub people: Vec<String>,
    pub star: bool,
    pub hidden: bool,
}

/// The undo step one `photo_set_info` call makes: its label and the photos
/// it changed, with their values before. Photos that already had the new
/// value are not in it.
#[derive(Debug, Clone, PartialEq)]
pub struct UndoStep {
    pub label: &'static str,
    /// The fields the call set: undo writes back only these.
    pub fields: InfoFields,
    pub before: Vec<PriorInfo>,
}

/// Which typed fields a `photo_set_info` call set.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InfoFields {
    pub description: bool,
    pub notes: bool,
    pub people: bool,
    pub date: bool,
    /// Coordinates and name together.
    pub place: bool,
    pub star: bool,
    pub hidden: bool,
}

impl InfoFields {
    /// The fields `patch` sets.
    pub fn of(patch: &InfoPatch) -> Self {
        Self {
            description: patch.description.is_some(),
            notes: patch.notes.is_some(),
            people: patch.people.is_some(),
            date: patch.date_override.is_some(),
            place: patch.location.is_some(),
            star: patch.star.is_some(),
            hidden: patch.hidden.is_some(),
        }
    }
}

/// One name of `album_people`, with the number of photos it is on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PersonCount {
    pub name: String,
    pub count: u32,
}

/// A text field's value: trimmed, and None when that leaves nothing.
fn clean_text(value: Option<String>) -> Option<String> {
    value.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}

/// A person's name: trimmed, inner runs of white space made one space, cut
/// to [`MAX_NAME_CHARS`]. None when nothing is left.
pub fn clean_name(name: &str) -> Option<String> {
    let joined = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let cut: String = joined.chars().take(MAX_NAME_CHARS).collect();
    let cut = cut.trim_end().to_owned();
    (!cut.is_empty()).then_some(cut)
}

/// The key two names are compared by: Unicode lower case, so "Émile" and
/// "émile" are one person (SQLite's NOCASE folds only ASCII).
fn name_key(name: &str) -> String {
    name.to_lowercase()
}

/// The names of a list, cleaned, without repeats (ignoring case; the first
/// spelling in the list is kept).
fn clean_names(names: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    names
        .iter()
        .filter_map(|n| clean_name(n))
        .filter(|n| seen.insert(name_key(n)))
        .collect()
}

fn invalid_date() -> AppError {
    AppError::new(ErrorCode::InvalidDate, text::photo::INVALID_DATE)
}

fn invalid_location(message: &str) -> AppError {
    AppError::new(ErrorCode::InvalidLocation, message)
}

fn is_leap(y: u32) -> bool {
    (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
}

fn days_in(y: u32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(y) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Parses `n` ASCII digits.
fn digits(s: &str) -> Option<u32> {
    (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        .then(|| s.parse().ok())
        .flatten()
}

/// A typed date in one of the forms the date table accepts
/// (`YYYY-MM-DD HH:MM:SS`, `YYYY-MM-DD HH:MM`, `YYYY-MM-DD`, with a space
/// or a `T`), as `YYYY-MM-DDTHH:MM:SS`. None when it is not a real date
/// and time with a year from 1800 to 2200. No time zone is applied.
pub fn parse_date(typed: &str) -> Option<String> {
    let typed = typed.trim();
    let (date, time) = match typed.find([' ', 'T']) {
        Some(i) => (&typed[..i], typed[i + 1..].trim_start()),
        None => (typed, ""),
    };
    let d: Vec<&str> = date.split('-').collect();
    let [y, mo, da] = d.as_slice() else {
        return None;
    };
    if y.len() != 4 || mo.len() != 2 || da.len() != 2 {
        return None;
    }
    let (y, mo, da) = (digits(y)?, digits(mo)?, digits(da)?);
    let (h, mi, s) = if time.is_empty() {
        (12, 0, 0)
    } else {
        let t: Vec<&str> = time.split(':').collect();
        if t.iter().any(|p| p.len() != 2) {
            return None;
        }
        match t.as_slice() {
            [h, mi] => (digits(h)?, digits(mi)?, 0),
            [h, mi, s] => (digits(h)?, digits(mi)?, digits(s)?),
            _ => return None,
        }
    };
    let ok = (1800..=2200).contains(&y)
        && (1..=12).contains(&mo)
        && da >= 1
        && da <= days_in(y, mo)
        && h < 24
        && mi < 60
        && s < 60;
    ok.then(|| format!("{y:04}-{mo:02}-{da:02}T{h:02}:{mi:02}:{s:02}"))
}

/// Two coordinates are the same place when they agree to 5 decimals.
fn same_coordinate(a: f64, b: f64) -> bool {
    (a * 1e5).round() == (b * 1e5).round()
}

/// A photo's typed info as stored, for comparing before and after.
#[derive(Debug, Clone, PartialEq)]
struct Stored {
    description: Option<String>,
    notes: Option<String>,
    date_override: Option<String>,
    loc: Option<(f64, f64)>,
    loc_name: Option<String>,
    people: Vec<String>,
    star: bool,
    hidden: bool,
    captured_at: Option<String>,
    gps: Option<(f64, f64)>,
}

impl Stored {
    fn prior(self, id: i64) -> PriorInfo {
        PriorInfo {
            id,
            description: self.description,
            notes: self.notes,
            date_override: self.date_override,
            loc: self.loc,
            loc_name: self.loc_name,
            people: self.people,
            star: self.star,
            hidden: self.hidden,
        }
    }
}

fn failed() -> impl Fn(rusqlite::Error) -> AppError {
    sqlite_error(text::album::WRITING_DATABASE)
}

fn not_found() -> AppError {
    AppError::new(ErrorCode::PhotoNotFound, text::photo::NOT_FOUND)
}

fn stored(tx: &Transaction<'_>, id: i64) -> AppResult<Option<Stored>> {
    let row = tx
        .query_row(
            "SELECT description, notes, date_override, loc_lat, loc_lon, loc_name, captured_at,
                    gps_lat, gps_lon, star, hidden
               FROM photos WHERE id = ?1",
            [id],
            |r| {
                Ok(Stored {
                    description: r.get(0)?,
                    notes: r.get(1)?,
                    date_override: r.get(2)?,
                    loc: r.get::<_, Option<f64>>(3)?.zip(r.get::<_, Option<f64>>(4)?),
                    loc_name: r.get(5)?,
                    captured_at: r.get(6)?,
                    gps: r.get::<_, Option<f64>>(7)?.zip(r.get::<_, Option<f64>>(8)?),
                    star: r.get::<_, i64>(9)? != 0,
                    hidden: r.get::<_, i64>(10)? != 0,
                    people: Vec::new(),
                })
            },
        )
        .optional()
        .map_err(failed())?;
    let Some(mut s) = row else { return Ok(None) };
    let mut st = tx
        .prepare_cached(
            "SELECT p.name FROM photo_people pp JOIN people p ON p.id = pp.person_id
              WHERE pp.photo_id = ?1 ORDER BY p.name COLLATE NOCASE, p.name",
        )
        .map_err(failed())?;
    s.people = st
        .query_map([id], |r| r.get(0))
        .and_then(|rows| rows.collect::<Result<_, _>>())
        .map_err(failed())?;
    Ok(Some(s))
}

/// The patch's values, checked once for every photo it goes to. The date
/// and place still depend on each photo's camera values.
struct Checked {
    description: Option<Option<String>>,
    notes: Option<Option<String>>,
    people: Option<Vec<String>>,
    date: Option<Option<String>>,
    location: Option<Option<TypedLocation>>,
    star: Option<bool>,
    hidden: Option<bool>,
}

/// The undo label of a patch (docs/05-organising.md, "What can be undone").
/// The page sends one kind of change per call; for a patch of several the
/// first in this order names the step.
pub fn undo_label(patch: &InfoPatch) -> &'static str {
    if patch.star.is_some() {
        "star"
    } else if patch.hidden.is_some() {
        "hide"
    } else if patch.people.is_some() {
        "people"
    } else if patch.description.is_some() {
        "description"
    } else if patch.notes.is_some() {
        "notes"
    } else if patch.date_override.is_some() {
        "date"
    } else {
        "place"
    }
}

fn check(patch: InfoPatch) -> AppResult<Checked> {
    let date = match patch.date_override {
        Some(Some(typed)) => Some(Some(parse_date(&typed).ok_or_else(invalid_date)?)),
        Some(None) => Some(None),
        None => None,
    };
    let location = match patch.location {
        Some(Some(l)) => {
            if !l.lat.is_finite() || !(-90.0..=90.0).contains(&l.lat) {
                return Err(invalid_location(text::photo::LATITUDE_RANGE));
            }
            if !l.lon.is_finite() || !(-180.0..=180.0).contains(&l.lon) {
                return Err(invalid_location(text::photo::LONGITUDE_RANGE));
            }
            Some(Some(TypedLocation {
                name: clean_text(l.name),
                ..l
            }))
        }
        Some(None) => Some(None),
        None => None,
    };
    Ok(Checked {
        description: patch.description.map(clean_text),
        notes: patch.notes.map(clean_text),
        people: patch.people.map(|p| clean_names(p.unwrap_or_default())),
        date,
        location,
        star: patch.star,
        hidden: patch.hidden,
    })
}

/// The album's people, keyed by [`name_key`], with their stored spelling
/// and id.
fn known_people(tx: &Transaction<'_>) -> AppResult<HashMap<String, (i64, String)>> {
    let mut st = tx
        .prepare_cached("SELECT id, name FROM people")
        .map_err(failed())?;
    let rows = st
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .and_then(|rows| rows.collect::<Result<Vec<_>, _>>())
        .map_err(failed())?;
    Ok(rows
        .into_iter()
        .map(|(id, name)| (name_key(&name), (id, name)))
        .collect())
}

/// Writes photo `id`'s people: each name takes the album's spelling when
/// the album knows it (first spelling wins), else becomes a new person.
fn write_people(tx: &Transaction<'_>, id: i64, names: &[String]) -> AppResult<()> {
    let mut known = known_people(tx)?;
    tx.execute("DELETE FROM photo_people WHERE photo_id = ?1", [id])
        .map_err(failed())?;
    for name in names {
        let key = name_key(name);
        let person = match known.get(&key) {
            Some((pid, _)) => *pid,
            None => {
                tx.execute("INSERT INTO people (name) VALUES (?1)", [name])
                    .map_err(failed())?;
                let pid = tx.last_insert_rowid();
                known.insert(key, (pid, name.clone()));
                pid
            }
        };
        tx.execute(
            "INSERT OR IGNORE INTO photo_people (photo_id, person_id) VALUES (?1, ?2)",
            params![id, person],
        )
        .map_err(failed())?;
    }
    Ok(())
}

/// The people names as they would be stored: the album's spelling for
/// names it knows, in the stored order (by name, ignoring case).
fn spelled(known: &HashMap<String, (i64, String)>, names: &[String]) -> Vec<String> {
    let mut out: Vec<String> = names
        .iter()
        .map(|n| {
            known
                .get(&name_key(n))
                .map_or_else(|| n.clone(), |(_, s)| s.clone())
        })
        .collect();
    out.sort_by(|a, b| {
        a.to_ascii_lowercase()
            .cmp(&b.to_ascii_lowercase())
            .then(a.cmp(b))
    });
    out
}

/// `photo_set_info(ids, patch)`: changes typed info of every photo in
/// `ids` in one transaction and answers which photos changed.
///
/// `view_only` in a view-only album, `photo_not_found` when an id is not a
/// photo the album shows, `invalid_date` and `invalid_location` when a
/// value breaks the rules; each writes nothing. A date equal to the
/// camera's clears the override, and a place equal to the camera's GPS (to
/// 5 decimals) with no name clears the typed place, so no pointless
/// override is stored. A patch that changes nothing writes nothing and
/// keeps the revision; otherwise the revision is raised and the caller
/// sends `album_changed` with reason `info`.
pub fn set_info(album: &Album, ids: Vec<i64>, patch: InfoPatch) -> AppResult<InfoChanged> {
    let (changed, step) = set_info_step(album, ids, patch)?;
    if let Some(step) = step {
        album.undo().push(Step::Info(step));
    }
    Ok(changed)
}

/// [`set_info`], also answering the undo step the call makes: None when
/// nothing changed.
pub fn set_info_step(
    album: &Album,
    ids: Vec<i64>,
    patch: InfoPatch,
) -> AppResult<(InfoChanged, Option<UndoStep>)> {
    if album.is_view_only() {
        return Err(AppError::new(ErrorCode::ViewOnly, text::photo::VIEW_ONLY));
    }
    let label = undo_label(&patch);
    let fields = InfoFields::of(&patch);
    let checked = check(patch)?;
    // Whether each id is a photo the album shows, read before the write so
    // `servable` can be given the album.
    let shown: Vec<bool> = album
        .db()
        .read({
            let ids = ids.clone();
            move |conn| {
                ids.iter()
                    .map(|&id| rows::load_row(conn, By::Id(id)))
                    .collect::<AppResult<Vec<_>>>()
            }
        })?
        .into_iter()
        .map(|row| {
            row.is_some_and(|(_, rel_path, missing, facts)| {
                servable(album, &rel_path, missing, &facts)
            })
        })
        .collect();
    if shown.iter().any(|s| !s) {
        return Err(not_found());
    }

    let before = album.write_photos(move |tx, touched| {
        let mut before_all = Vec::new();
        for id in ids {
            let before = stored(tx, id)?.ok_or_else(not_found)?;
            let mut after = before.clone();
            if let Some(v) = &checked.description {
                after.description = v.clone();
            }
            if let Some(v) = &checked.notes {
                after.notes = v.clone();
            }
            if let Some(v) = &checked.date {
                after.date_override = match v {
                    Some(d) if Some(d) == before.captured_at.as_ref() => None,
                    other => other.clone(),
                };
            }
            if let Some(v) = checked.star {
                after.star = v;
            }
            if let Some(v) = checked.hidden {
                after.hidden = v;
            }
            if let Some(v) = &checked.location {
                (after.loc, after.loc_name) = match v {
                    Some(l)
                        if l.name.is_none()
                            && before.gps.is_some_and(|(glat, glon)| {
                                same_coordinate(glat, l.lat) && same_coordinate(glon, l.lon)
                            }) =>
                    {
                        (None, None)
                    }
                    Some(l) => (Some((l.lat, l.lon)), l.name.clone()),
                    None => (None, None),
                };
            }
            let people_changed = match &checked.people {
                Some(names) => {
                    let known = known_people(tx)?;
                    let next = spelled(&known, names);
                    let differs = next != before.people;
                    after.people = next;
                    differs
                }
                None => false,
            };
            if after == before {
                continue;
            }
            let flags_only = Stored {
                star: before.star,
                hidden: before.hidden,
                ..after.clone()
            } == before;
            tx.execute(
                "UPDATE photos SET description = ?2, notes = ?3, date_override = ?4,
                        loc_lat = ?5, loc_lon = ?6, loc_name = ?7, info_updated_at = ?8,
                        star = ?9, hidden = ?10
                  WHERE id = ?1",
                params![
                    id,
                    after.description,
                    after.notes,
                    after.date_override,
                    after.loc.map(|l| l.0),
                    after.loc.map(|l| l.1),
                    after.loc_name,
                    now_ms(),
                    after.star,
                    after.hidden,
                ],
            )
            .map_err(failed())?;
            if people_changed && let Some(names) = &checked.people {
                write_people(tx, id, names)?;
            }
            // Star and hidden are not in the search text.
            if !flags_only {
                search::reindex(tx, id)?;
            }
            touched.photo(id);
            before_all.push(before.prior(id));
        }
        if !before_all.is_empty() {
            // A name no photo uses any more is forgotten (docs/02-data.md).
            tx.execute(
                "DELETE FROM people WHERE id NOT IN (SELECT person_id FROM photo_people)",
                [],
            )
            .map_err(failed())?;
        }
        Ok(before_all)
    })?;
    let changed: Vec<i64> = before.iter().map(|p| p.id).collect();
    if changed.is_empty() {
        let revision = album.revision();
        return Ok((InfoChanged { changed, revision }, None));
    }
    let revision = album.next_revision();
    let step = UndoStep {
        label,
        fields,
        before,
    };
    Ok((InfoChanged { changed, revision }, Some(step)))
}

/// Writes back the values `step` holds, for the fields it set, in one
/// transaction, and answers the photos written. A photo whose row no
/// longer exists or is missing is left out. Records no undo step and does
/// not raise the revision; the caller does.
pub fn restore_info(album: &Album, step: &UndoStep) -> AppResult<Vec<i64>> {
    let fields = step.fields;
    let before = step.before.clone();
    album.write_photos(move |tx, touched| {
        let mut done = Vec::new();
        for prior in before {
            let id = prior.id;
            let missing: Option<bool> = tx
                .query_row("SELECT missing FROM photos WHERE id = ?1", [id], |r| {
                    Ok(r.get::<_, i64>(0)? != 0)
                })
                .optional()
                .map_err(failed())?;
            if missing != Some(false) {
                continue;
            }
            let Some(mut now) = stored(tx, id)? else {
                continue;
            };
            if fields.description {
                now.description = prior.description;
            }
            if fields.notes {
                now.notes = prior.notes;
            }
            if fields.date {
                now.date_override = prior.date_override;
            }
            if fields.place {
                (now.loc, now.loc_name) = (prior.loc, prior.loc_name);
            }
            if fields.star {
                now.star = prior.star;
            }
            if fields.hidden {
                now.hidden = prior.hidden;
            }
            tx.execute(
                "UPDATE photos SET description = ?2, notes = ?3, date_override = ?4,
                        loc_lat = ?5, loc_lon = ?6, loc_name = ?7, info_updated_at = ?8,
                        star = ?9, hidden = ?10
                  WHERE id = ?1",
                params![
                    id,
                    now.description,
                    now.notes,
                    now.date_override,
                    now.loc.map(|l| l.0),
                    now.loc.map(|l| l.1),
                    now.loc_name,
                    now_ms(),
                    now.star,
                    now.hidden,
                ],
            )
            .map_err(failed())?;
            if fields.people {
                write_people(tx, id, &prior.people)?;
            }
            if fields.description || fields.notes || fields.people || fields.place {
                search::reindex(tx, id)?;
            }
            touched.photo(id);
            done.push(id);
        }
        if fields.people && !done.is_empty() {
            tx.execute(
                "DELETE FROM people WHERE id NOT IN (SELECT person_id FROM photo_people)",
                [],
            )
            .map_err(failed())?;
        }
        Ok(done)
    })
}

/// `album_people()`: every name on at least one photo that is not
/// missing, with its count, most used first and ties in natural order.
/// Hidden photos count. The whole album, even in a scoped open.
pub fn album_people(album: &Album) -> AppResult<Vec<PersonCount>> {
    let mut people = album.db().read(|conn| {
        let failed = sqlite_error(text::photo::READING_DETAIL);
        let mut st = conn
            .prepare(
                "SELECT n.name, count(*) FROM people n
                   JOIN photo_people pp ON pp.person_id = n.id
                   JOIN photos p ON p.id = pp.photo_id
                  WHERE p.missing = 0
                  GROUP BY n.id",
            )
            .map_err(&failed)?;
        st.query_map([], |r| {
            Ok(PersonCount {
                name: r.get(0)?,
                count: r.get(1)?,
            })
        })
        .and_then(|rows| rows.collect::<Result<Vec<_>, _>>())
        .map_err(&failed)
    })?;
    people.sort_by(|a, b| match b.count.cmp(&a.count) {
        Ordering::Equal => natural_cmp_text(&a.name, &b.name),
        o => o,
    });
    Ok(people)
}

#[cfg(test)]
mod tests;
