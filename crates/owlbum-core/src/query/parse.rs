//! Query words in the search box (docs/05-organising.md, "Query words in
//! the search box").
//!
//! The page sends the box's text and shows what comes back; it has no table
//! of keys of its own. [`parse`] does the work over any [`Names`], so the
//! rules are tested without an album; [`parse_box`] is `query_parse` over
//! an open album.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::filter::day_of;
use super::search::words;
use super::shape::{Filter, HiddenFilter, ShapeFilter, StarFilter, TagFilter, TagKind};
use crate::album::{Album, sqlite_error};
use crate::error::AppResult;
use crate::text;
use crate::text::query::words as hint;

/// What `query_parse` answers.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct ParsedBox {
    /// True when at least one word became a filter.
    pub taken: bool,
    /// Single-value filters to set. Never holds people or tags.
    pub set: Filter,
    /// Tags to add to the filter's list; none already in it.
    pub add_tags: Vec<TagFilter>,
    /// People to add, in their stored spelling; none already in it.
    pub add_people: Vec<String>,
    /// What the box should now hold.
    pub rest: String,
    /// The search text to use; null when empty.
    pub search: Option<String>,
    /// The message for the hint line, or null.
    pub error: Option<String>,
}

/// The names a `folder:` or `person:` word is checked against. Asked only
/// when the box holds such a word.
pub trait Names {
    /// The scope's own folder, and every folder in the scope that holds a
    /// photo, each relative to the album root.
    fn folders(&mut self) -> (String, Vec<String>);
    /// Every person's stored name.
    fn people(&mut self) -> Vec<String>;
}

/// The known keys, in lower case.
const KEYS: [&str; 11] = [
    "star", "shape", "hidden", "from", "to", "folder", "has", "is", "tag", "object", "person",
];

/// The known key of `word` and its value with the quotes taken off, or
/// None when the word is plain text.
fn split(word: &str) -> Option<(&'static str, &str)> {
    let (key, value) = word.split_once(':')?;
    if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let key = KEYS.iter().find(|k| k.eq_ignore_ascii_case(key))?;
    let value = value
        .strip_prefix('"')
        .map_or(value, |v| v.strip_suffix('"').unwrap_or(v));
    Some((key, value))
}

/// The folder `value` names: matched without regard to case against each
/// folder's path from the album root, or from the scope's folder. `top` is
/// the scope's folder, when it holds a photo.
fn find_folder(names: &mut dyn Names, value: &str) -> Option<String> {
    let (top, folders) = names.folders();
    if value.eq_ignore_ascii_case("top") {
        return folders.contains(&top).then_some(top);
    }
    let want = value.to_lowercase();
    let below = |f: &str| -> Option<String> {
        if top.is_empty() {
            return None;
        }
        f.strip_prefix(top.as_str())?
            .strip_prefix('/')
            .map(str::to_lowercase)
    };
    folders
        .iter()
        .find(|f| f.to_lowercase() == want)
        .or_else(|| folders.iter().find(|f| below(f).as_deref() == Some(&want)))
        .cloned()
}

/// The parse in progress.
struct Parse<'a> {
    filter: &'a Filter,
    names: &'a mut dyn Names,
    out: ParsedBox,
}

impl Parse<'_> {
    /// Applies one word with a known key. Err holds the hint when the value
    /// is bad.
    fn apply(&mut self, key: &str, value: &str) -> Result<(), String> {
        let fixed = |options: &[&str], hint: &str| -> Result<usize, String> {
            options
                .iter()
                .position(|o| o.eq_ignore_ascii_case(value))
                .ok_or_else(|| hint.to_owned())
        };
        let set = &mut self.out.set;
        match key {
            "star" => {
                set.star =
                    Some([StarFilter::Yes, StarFilter::No][fixed(&["yes", "no"], hint::STAR)?]);
            }
            "shape" => {
                set.shape = Some(
                    [ShapeFilter::Portrait, ShapeFilter::Landscape]
                        [fixed(&["portrait", "landscape"], hint::SHAPE)?],
                );
            }
            "hidden" => {
                set.hidden = Some(
                    [HiddenFilter::Show, HiddenFilter::Only]
                        [fixed(&["show", "only"], hint::HIDDEN)?],
                );
            }
            "from" | "to" => {
                let is_from = key == "from";
                if day_of(value).is_none() {
                    return Err(if is_from { hint::FROM } else { hint::TO }.to_owned());
                }
                let value = value.to_owned();
                let other = if is_from {
                    set.to.clone().or_else(|| self.filter.to.clone())
                } else {
                    set.from.clone().or_else(|| self.filter.from.clone())
                };
                let out_of_order =
                    other.is_some_and(|o| if is_from { value > o } else { value < o });
                if out_of_order {
                    return Err(hint::FROM_AFTER_TO.to_owned());
                }
                if is_from {
                    set.from = Some(value);
                } else {
                    set.to = Some(value);
                }
            }
            "folder" => {
                let folder =
                    find_folder(self.names, value).ok_or_else(|| hint::no_folder(value))?;
                self.out.set.folder = Some(folder);
            }
            "has" => match fixed(&["location", "people"], hint::HAS)? {
                0 => set.has_location = Some(true),
                _ => set.has_people = Some(true),
            },
            "is" => match fixed(&["lowquality", "duplicate"], hint::IS)? {
                0 => set.low_quality = Some(true),
                _ => set.near_duplicates = Some(true),
            },
            "tag" | "object" => {
                let (kind, bad) = if key == "tag" {
                    (TagKind::Keyword, hint::TAG)
                } else {
                    (TagKind::Object, hint::OBJECT)
                };
                let value = value.trim();
                if value.is_empty() {
                    return Err(bad.to_owned());
                }
                let lower = value.to_lowercase();
                let had = self
                    .filter
                    .tags
                    .iter()
                    .flatten()
                    .chain(&self.out.add_tags)
                    .any(|t| t.kind == kind && t.value.trim().to_lowercase() == lower);
                if !had {
                    self.out.add_tags.push(TagFilter {
                        kind,
                        value: value.to_owned(),
                    });
                }
            }
            _ => {
                // person
                let value = value.trim();
                let lower = value.to_lowercase();
                let name = (!value.is_empty())
                    .then(|| {
                        self.names
                            .people()
                            .into_iter()
                            .find(|n| n.to_lowercase() == lower)
                    })
                    .flatten()
                    .ok_or_else(|| hint::no_person(value))?;
                let had = self
                    .filter
                    .people
                    .iter()
                    .flatten()
                    .chain(&self.out.add_people)
                    .any(|n| n.to_lowercase() == lower);
                if !had {
                    self.out.add_people.push(name);
                }
            }
        }
        Ok(())
    }
}

/// Parses the box's `text`. With `last` false (typing) the unfinished last
/// word, the text after the last space outside quotes, is left alone; with
/// `last` true (Enter) it is parsed too. `filter` is the filter the page
/// holds, for the order of `from` and `to` and for tags and people already
/// in it.
pub fn parse(text: &str, last: bool, filter: &Filter, names: &mut dyn Names) -> ParsedBox {
    let all = words(text);
    let quoted = text.chars().filter(|&c| c == '"').count() % 2 == 1;
    let unfinished =
        (!last && !all.is_empty() && (quoted || !text.ends_with(' '))).then(|| all[all.len() - 1]);
    let finished = &all[..all.len() - usize::from(unfinished.is_some())];

    let mut p = Parse {
        filter,
        names,
        out: ParsedBox::default(),
    };
    let mut kept: Vec<&str> = Vec::new();
    for &word in finished {
        let Some((key, value)) = split(word) else {
            kept.push(word);
            continue;
        };
        match p.apply(key, value) {
            Ok(()) => p.out.taken = true,
            Err(why) => {
                kept.push(word);
                p.out.error.get_or_insert_with(|| hint::error(word, &why));
            }
        }
    }
    let mut out = p.out;

    let mut search: Vec<&str> = kept.clone();
    search.extend(unfinished.filter(|w| split(w).is_none()));
    let search = search.join(" ");
    out.search = (!search.trim().is_empty()).then(|| search.trim().to_owned());

    let mut rest = kept.join(" ");
    match unfinished {
        Some(w) => {
            if !rest.is_empty() {
                rest.push(' ');
            }
            rest.push_str(w);
        }
        // Typing goes on after a space, so the box keeps one.
        None if !last && !rest.is_empty() => rest.push(' '),
        None => {}
    }
    out.rest = rest.trim_start().to_owned();
    if last {
        out.rest = out.rest.trim_end().to_owned();
    }
    out
}

/// The folders and people of an open album, read the first time a word
/// asks.
struct AlbumNames<'a> {
    album: &'a Album,
    failed: Option<crate::error::AppError>,
}

impl Names for AlbumNames<'_> {
    fn folders(&mut self) -> (String, Vec<String>) {
        self.album.index().read(|v| {
            let mut used = HashSet::new();
            for p in v.photos() {
                used.insert(p.folder);
            }
            let mut folders: Vec<String> =
                used.into_iter().map(|f| v.folder(f).to_owned()).collect();
            folders.sort_unstable();
            (v.scope().to_owned(), folders)
        })
    }

    fn people(&mut self) -> Vec<String> {
        let read = self.album.db().read(|conn| {
            let failed = sqlite_error(text::query::READING_ROWS);
            let mut stmt = conn
                .prepare_cached("SELECT name FROM people ORDER BY name")
                .map_err(&failed)?;
            let names = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(&failed)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(&failed)?;
            Ok(names)
        });
        read.unwrap_or_else(|e| {
            self.failed = Some(e);
            Vec::new()
        })
    }
}

/// `query_parse` over an open album.
pub fn parse_box(album: &Album, text: &str, last: bool, filter: &Filter) -> AppResult<ParsedBox> {
    let filter = filter.normalised();
    let mut names = AlbumNames {
        album,
        failed: None,
    };
    let parsed = parse(text, last, &filter, &mut names);
    match names.failed {
        Some(e) => Err(e),
        None => Ok(parsed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An album whose top folder is the root, with `day 2` and `Trip/Day`,
    /// and the people Ann and Bo.
    struct Fake {
        asked: usize,
    }

    impl Names for Fake {
        fn folders(&mut self) -> (String, Vec<String>) {
            self.asked += 1;
            (
                String::new(),
                vec![String::new(), "Day 2".into(), "Trip/Day".into()],
            )
        }
        fn people(&mut self) -> Vec<String> {
            self.asked += 1;
            vec!["Ann".into(), "Bo".into()]
        }
    }

    fn run(text: &str, last: bool) -> ParsedBox {
        parse(text, last, &Filter::default(), &mut Fake { asked: 0 })
    }

    fn tag(kind: TagKind, value: &str) -> TagFilter {
        TagFilter {
            kind,
            value: value.to_owned(),
        }
    }

    #[test]
    fn the_examples_of_the_doc() {
        let r = run("star:yes lake shape:Portrait", true);
        assert!(r.taken);
        assert_eq!(r.set.star, Some(StarFilter::Yes));
        assert_eq!(r.set.shape, Some(ShapeFilter::Portrait));
        assert_eq!(
            (r.rest.as_str(), r.search.as_deref(), r.error),
            ("lake", Some("lake"), None)
        );

        let r = run("HAS:People ", false);
        assert_eq!(r.set.has_people, Some(true));
        assert_eq!((r.rest.as_str(), r.search), ("", None));

        let r = run(r#"folder:"day 2" "#, false);
        assert_eq!(
            r.set.folder.as_deref(),
            Some("Day 2"),
            "the folder's own spelling"
        );

        let r = run("tag:Lake object:boat ", false);
        assert_eq!(
            r.add_tags,
            [tag(TagKind::Keyword, "Lake"), tag(TagKind::Object, "boat")]
        );

        let r = run("person:ann ", false);
        assert_eq!(r.add_people, ["Ann"]);

        let r = run("from:2024-06-01 to:2024-06-30 ", false);
        assert_eq!(r.set.from.as_deref(), Some("2024-06-01"));
        assert_eq!(r.set.to.as_deref(), Some("2024-06-30"));

        let r = run("star:maybe lake", true);
        assert!(!r.taken);
        assert_eq!(
            r.error.as_deref(),
            Some(r#""star:maybe" isn't a filter: try star:yes or star:no"#)
        );
        assert_eq!(r.search.as_deref(), Some("star:maybe lake"));

        let r = run("folder:nowhere ", false);
        assert!(!r.taken);
        assert_eq!(
            r.error.as_deref(),
            Some(r#""folder:nowhere" isn't a filter: no folder named nowhere"#)
        );

        let r = run("star:yes shape:por", false);
        assert_eq!(r.set.star, Some(StarFilter::Yes));
        assert_eq!(r.set.shape, None);
        assert_eq!(
            (r.rest.as_str(), r.search, r.error),
            ("shape:por", None, None)
        );
    }

    #[test]
    fn the_parsing_rules() {
        // Unknown keys are plain text and stay, with no error.
        let r = run("http://x a:b ", false);
        assert!(!r.taken && r.error.is_none());
        assert_eq!(r.search.as_deref(), Some("http://x a:b"));
        // The later single-value word wins.
        assert_eq!(
            run("star:yes star:no ", false).set.star,
            Some(StarFilter::No)
        );
        // The same tag or person twice is added once.
        assert_eq!(run("tag:lake tag:LAKE ", false).add_tags.len(), 1);
        assert_eq!(run("person:ann person:Ann ", false).add_people, ["Ann"]);
        // The first bad word sets the error.
        let r = run("star:x shape:y ", false);
        assert!(r.error.unwrap().contains("star:x"));
        // A space inside an open quote does not finish the word.
        let r = run(r#"folder:"Day "#, false);
        assert!(!r.taken && r.error.is_none());
        assert_eq!(r.rest, r#"folder:"Day "#);
        assert_eq!(
            run(r#"folder:"Day 2" "#, false).set.folder.as_deref(),
            Some("Day 2")
        );
        // Typing goes on after a taken word.
        assert_eq!(run("lake star:yes ", false).rest, "lake ");
        // An unfinished word with a known key is no search text yet.
        assert_eq!(run("lake star:ye", false).search.as_deref(), Some("lake"));
        assert_eq!(
            run("lake star:ye", true).search.as_deref(),
            Some("lake star:ye")
        );
    }

    #[test]
    fn values_are_checked() {
        for (text, want) in [
            ("from:2024-02-30", "try from:YYYY-MM-DD"),
            ("to:2024-6-1", "try to:YYYY-MM-DD"),
            ("has:gps", "try has:location or has:people"),
            ("is:ugly", "try is:lowquality or is:duplicate"),
            ("hidden:no", "try hidden:show or hidden:only"),
            (r#"tag:"  ""#, "try tag:<word>"),
            ("object:", "try object:<word>"),
            ("person:zed", "no person named zed"),
            ("from:2024-07-01 to:2024-06-01", "from must not be after to"),
        ] {
            let r = run(text, true);
            assert!(
                r.error.as_deref().is_some_and(|e| e.ends_with(want)),
                "{text}: {:?}",
                r.error
            );
        }
        let r = run(
            "is:LowQuality is:duplicate has:location hidden:Only ",
            false,
        );
        assert_eq!(r.set.low_quality, Some(true));
        assert_eq!(r.set.near_duplicates, Some(true));
        assert_eq!(r.set.has_location, Some(true));
        assert_eq!(r.set.hidden, Some(HiddenFilter::Only));
        // `from` is checked against the filter the page holds too.
        let held = Filter {
            to: Some("2024-01-01".into()),
            ..Filter::default()
        };
        let r = parse("from:2024-02-01 ", false, &held, &mut Fake { asked: 0 });
        assert!(!r.taken && r.error.unwrap().ends_with("from must not be after to"));
        // Tags and people already in the filter are not added again.
        let held = Filter {
            tags: Some(vec![tag(TagKind::Keyword, "Lake")]),
            people: Some(vec!["Ann".into()]),
            ..Filter::default()
        };
        let r = parse("tag:lake person:ANN ", false, &held, &mut Fake { asked: 0 });
        assert!(r.taken && r.add_tags.is_empty() && r.add_people.is_empty());
    }

    #[test]
    fn folder_top_and_paths() {
        assert_eq!(run("folder:TOP ", false).set.folder.as_deref(), Some(""));
        assert_eq!(
            run("folder:trip/day ", false).set.folder.as_deref(),
            Some("Trip/Day")
        );
        assert!(
            run("folder:Trip ", false).error.is_some(),
            "a folder that holds no photo"
        );
    }

    #[test]
    fn names_are_read_only_when_a_word_needs_them() {
        let mut names = Fake { asked: 0 };
        parse(
            "star:yes lake tag:x ",
            false,
            &Filter::default(),
            &mut names,
        );
        assert_eq!(names.asked, 0);
    }
}
