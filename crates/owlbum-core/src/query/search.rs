//! Text search (docs/05-organising.md, "Text search").
//!
//! The search text is never passed to FTS5 as written. [`fts_query`] turns
//! it into terms that are each one quoted phrase with a prefix star, joined
//! with `AND`, so no operator, bracket or stray quote can change its
//! meaning or be a syntax error. [`matching`] runs it over the `search`
//! table and, for rows held only in memory, over their file names.

use std::collections::HashSet;

use super::support;
use crate::album::{Album, sqlite_error};
use crate::error::AppResult;
use crate::info::country::fold;
use crate::scan::memory::Overlay;
use crate::text;

/// Splits `text` on spaces that are outside double quotes; each `"`
/// switches quoting on or off. The quotes stay in the words.
pub(super) fn words(text: &str) -> Vec<&str> {
    let mut words = Vec::new();
    let mut quoted = false;
    let mut start = None;
    for (i, c) in text.char_indices() {
        if c == '"' {
            quoted = !quoted;
        }
        if c == ' ' && !quoted {
            if let Some(s) = start.take() {
                words.push(&text[s..i]);
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        words.push(&text[s..]);
    }
    words
}

/// The words of one term as the tokeniser splits it: runs of letters and
/// digits, in order. Quotes are only delimiters.
fn term_words(term: &str) -> Vec<&str> {
    term.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect()
}

/// The terms of the search text, each the list of its words. Empty when
/// the text is not a filter (under the minimum length).
fn terms(search: &str) -> Vec<Vec<&str>> {
    if !support::is_search(Some(search)) {
        return Vec::new();
    }
    words(search)
        .into_iter()
        .map(term_words)
        .filter(|t| !t.is_empty())
        .collect()
}

/// The FTS5 query for `search`: `ann "day 2" IMG_12` gives
/// `"ann"* AND "day 2"* AND "img 12"*`. None when the text is not a filter.
pub fn fts_query(search: &str) -> Option<String> {
    let terms = terms(search);
    if terms.is_empty() {
        return None;
    }
    Some(
        terms
            .iter()
            .map(|t| format!("\"{}\"*", t.join(" ").replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" AND "),
    )
}

/// True when the words of `name` hold every term: each term's words appear
/// one after another, the last of them as a prefix. Case and the accents of
/// Latin letters are ignored.
fn name_matches(name: &str, terms: &[Vec<String>]) -> bool {
    let name = fold(name);
    let words = term_words(&name);
    terms.iter().all(|term| {
        let Some((last, whole)) = term.split_last() else {
            return true;
        };
        (0..words.len()).any(|i| {
            let n = whole.len();
            i + n < words.len()
                && whole.iter().zip(&words[i..i + n]).all(|(t, w)| t == w)
                && words[i + n].starts_with(last.as_str())
        })
    })
}

/// The ids of the photos that match `search`: those the `search` table
/// finds, and the rows held only in memory whose file name matches. None
/// when the text is not a filter.
pub(super) fn matching(
    album: &Album,
    overlay: &Overlay,
    search: Option<&str>,
) -> AppResult<Option<HashSet<u32>>> {
    let Some(search) = search else {
        return Ok(None);
    };
    let Some(fts) = fts_query(search) else {
        return Ok(None);
    };
    let mut ids: HashSet<u32> = album.db().read(|conn| {
        let failed = sqlite_error(text::query::READING_ROWS);
        let mut stmt = conn
            .prepare_cached("SELECT rowid FROM search WHERE search MATCH ?1")
            .map_err(&failed)?;
        let rows = stmt
            .query_map([&fts], |r| r.get::<_, i64>(0))
            .map_err(&failed)?;
        let mut ids = HashSet::new();
        for id in rows {
            if let Ok(id) = u32::try_from(id.map_err(&failed)?) {
                ids.insert(id);
            }
        }
        Ok(ids)
    })?;
    if !overlay.is_empty() {
        let terms: Vec<Vec<String>> = terms(search)
            .iter()
            .map(|t| t.iter().map(|w| fold(w)).collect())
            .collect();
        for row in overlay.rows().filter(|r| !r.missing) {
            if name_matches(row.file_name(), &terms)
                && let Ok(id) = u32::try_from(row.id)
            {
                ids.insert(id);
            }
        }
    }
    Ok(Some(ids))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fts_query_quotes_every_term_and_adds_a_prefix_star() {
        assert_eq!(
            fts_query(r#"ann "day 2" IMG_12"#).as_deref(),
            Some(r#""ann"* AND "day 2"* AND "IMG 12"*"#)
        );
        assert_eq!(fts_query("star:maybe").as_deref(), Some(r#""star maybe"*"#));
        assert_eq!(
            fts_query(r#"a OR (b) NOT c* ^d"#).as_deref(),
            Some(r#""a"* AND "OR"* AND "b"* AND "NOT"* AND "c"* AND "d"*"#)
        );
        assert_eq!(
            fts_query(r#"say "hi"#).as_deref(),
            Some(r#""say"* AND "hi"*"#)
        );
    }

    #[test]
    fn text_under_the_minimum_length_is_no_query() {
        for t in ["", " ", "a", " a ", "*", "\"\"", "a:"] {
            assert_eq!(fts_query(t), None, "{t:?}");
        }
        assert!(fts_query("ab").is_some());
        assert!(
            fts_query("a b").is_some(),
            "a one-letter term inside longer text is kept"
        );
    }

    #[test]
    fn words_split_on_spaces_outside_quotes() {
        assert_eq!(
            words(r#"  lake  folder:"day 2"  x "#),
            ["lake", r#"folder:"day 2""#, "x"]
        );
        assert_eq!(words(r#"folder:"Day "#), [r#"folder:"Day "#]);
    }

    #[test]
    fn names_match_by_word_prefix_ignoring_case_and_accents() {
        let t = |s: &str| -> Vec<Vec<String>> {
            terms(s)
                .iter()
                .map(|t| t.iter().map(|w| fold(w)).collect())
                .collect()
        };
        assert!(name_matches("José at the Lake.jpg", &t("jose lak")));
        assert!(name_matches("IMG_1234.jpg", &t("img_12")));
        assert!(!name_matches("IMG_1234.jpg", &t("mg")));
        assert!(!name_matches("lake.jpg", &t("ake")));
        assert!(
            !name_matches("IMG_1234.jpg", &t(r#""1234 img""#)),
            "phrase words in order"
        );
        assert!(
            name_matches("IMG_1234.jpg", &t("1234 img")),
            "terms in any order"
        );
    }
}
