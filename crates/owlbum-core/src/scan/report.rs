//! The scan report: what the last walk left out, and the files not shown
//! (docs/03-media-and-scanning.md, "Commands": `album_scan_report`,
//! `album_scan_errors`, `album_unsupported`).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::memory::Overlay;
use super::order::natural_cmp;
use super::walk::{self, UnreadableFolder, UnsupportedFile, Walk};
use crate::album::{Album, sqlite_error};
use crate::error::AppResult;
use crate::{media, text};

/// The longest list the report carries; the counts say how many there
/// were in all.
pub const REPORT_LIST_MAX: usize = 100;

/// The most items one page of `album_scan_errors` or `album_unsupported`
/// holds. A larger `limit` is treated as this.
pub const PAGE_MAX: u32 = 500;

/// A subfolder that is an album of its own, skipped whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct NestedAlbum {
    /// Relative to the album root.
    pub rel_path: String,
    /// "{path} is its own album and is not shown here."
    pub message: String,
}

/// How many files of one unsupported format the walk found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UnsupportedCount {
    /// The format's label, such as "HEIC".
    pub label: String,
    pub count: u32,
}

/// The reply of `album_scan_report`: counts and short lists from the last
/// whole walk. All empty before one has ended.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ScanReport {
    /// Subfolders that could not be read: the first 100, in the order
    /// met.
    pub unreadable_folders: Vec<UnreadableFolder>,
    /// How many subfolders could not be read.
    pub unreadable_folder_count: u32,
    /// Nested album roots: the first 100, in walker order.
    pub nested_albums: Vec<NestedAlbum>,
    /// How many nested album roots were skipped.
    pub nested_album_count: u32,
    /// Links (and junctions) skipped.
    pub links: u32,
    /// Files stored online only, not read.
    pub online_only: u32,
    /// Names that are not valid Unicode, skipped.
    pub unreadable_names: u32,
    /// Unsupported files by label, in the order of the registry's table of
    /// recognised-but-unsupported formats.
    pub unsupported: Vec<UnsupportedCount>,
    /// Sentences about the scan as a whole, such as the missing-GStreamer
    /// notice (none until video lands).
    pub notices: Vec<String>,
}

/// One row with a `scan_error`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ScanErrorItem {
    pub id: i64,
    pub rel_path: String,
    /// `io`, `decode`, `unsupported` or `too_large`.
    pub code: String,
    /// The sentence.
    pub message: String,
}

/// The reply of `album_scan_errors`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ScanErrors {
    /// Error rows in scope, in all.
    pub total: u32,
    pub items: Vec<ScanErrorItem>,
}

/// The reply of `album_unsupported`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UnsupportedFiles {
    /// Unsupported files in the last walk, in all.
    pub total: u32,
    pub items: Vec<UnsupportedFile>,
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// One page of `items`: from `offset`, at most `limit` and at most
/// [`PAGE_MAX`].
fn page<T: Clone>(items: &[T], offset: u32, limit: u32) -> Vec<T> {
    let from = usize::try_from(offset).unwrap_or(usize::MAX);
    let n = usize::try_from(limit.min(PAGE_MAX)).unwrap_or(0);
    items.iter().skip(from).take(n).cloned().collect()
}

/// The report of `walk`, the last whole walk; empty when there is none.
pub fn report(walk: Option<&Walk>) -> ScanReport {
    let Some(walk) = walk else {
        return ScanReport::default();
    };
    let mut by_label: HashMap<&str, u32> = HashMap::new();
    for u in &walk.unsupported {
        *by_label.entry(u.label.as_str()).or_default() += 1;
    }
    let unsupported = media::UNSUPPORTED
        .iter()
        .filter_map(|u| {
            by_label.get(u.label).map(|&count| UnsupportedCount {
                label: u.label.to_owned(),
                count,
            })
        })
        .collect();
    ScanReport {
        unreadable_folders: walk
            .unreadable
            .iter()
            .take(REPORT_LIST_MAX)
            .cloned()
            .collect(),
        unreadable_folder_count: count(walk.unreadable.len()),
        nested_albums: walk
            .nested
            .iter()
            .take(REPORT_LIST_MAX)
            .map(|p| NestedAlbum {
                rel_path: p.clone(),
                message: text::scan::nested_album(p),
            })
            .collect(),
        nested_album_count: count(walk.nested.len()),
        links: walk.links,
        online_only: count(walk.online_only.len()),
        unreadable_names: walk.unreadable_names,
        unsupported,
        notices: Vec::new(),
    }
}

/// One page of the unsupported files of `walk`, the last whole walk.
pub fn unsupported(walk: Option<&Walk>, offset: u32, limit: u32) -> UnsupportedFiles {
    let files = walk.map_or(&[][..], |w| &w.unsupported);
    UnsupportedFiles {
        total: count(files.len()),
        items: page(files, offset, limit),
    }
}

/// One page of the rows in scope with a `scan_error`, not missing, in
/// walker order. The rows a view-only album holds in `overlay` replace the
/// database's.
pub fn scan_errors(
    album: &Album,
    overlay: &Overlay,
    offset: u32,
    limit: u32,
) -> AppResult<ScanErrors> {
    let scope = &album.location().scope;
    let rows: Vec<(i64, String, bool, String)> = album.db().read(|conn| {
        let failed = sqlite_error(text::album::READING_DATABASE);
        let mut st = conn
            .prepare(
                "SELECT id, rel_path, missing, scan_error FROM photos
                 WHERE scan_error IS NOT NULL
                   AND (?1 = '' OR substr(rel_path, 1, length(?1) + 1) = ?1 || '/')",
            )
            .map_err(&failed)?;
        st.query_map([scope], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .map_err(&failed)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(&failed)
    })?;
    let mut by_id: HashMap<i64, (String, bool, Option<String>)> = rows
        .into_iter()
        .map(|(id, path, missing, error)| (id, (path, missing, Some(error))))
        .collect();
    for row in overlay.rows() {
        if walk::is_under(&row.rel_path, scope) {
            let error = row.facts.scan_error.clone();
            by_id.insert(row.id, (row.rel_path.clone(), row.missing, error));
        } else {
            by_id.remove(&row.id);
        }
    }
    let mut items: Vec<ScanErrorItem> = by_id
        .into_iter()
        .filter_map(|(id, (rel_path, missing, error))| {
            let error = error.filter(|_| !missing)?;
            let (code, message) = error.split_once(": ").unwrap_or((error.as_str(), ""));
            Some(ScanErrorItem {
                id,
                code: code.to_owned(),
                message: message.to_owned(),
                rel_path,
            })
        })
        .collect();
    items.sort_by(|a, b| natural_cmp(&a.rel_path, &b.rel_path).then(a.id.cmp(&b.id)));
    Ok(ScanErrors {
        total: count(items.len()),
        items: page(&items, offset, limit),
    })
}

/// Every item of a paged list: `page(offset, PAGE_MAX)` until `total` is
/// reached or a page comes back empty.
fn all_pages<T>(mut page: impl FnMut(u32) -> AppResult<(u32, Vec<T>)>) -> AppResult<Vec<T>> {
    let mut items = Vec::new();
    loop {
        let (total, more) = page(count(items.len()))?;
        let done = more.is_empty();
        items.extend(more);
        if done || items.len() >= usize::try_from(total).unwrap_or(usize::MAX) {
            return Ok(items);
        }
    }
}

/// The Files not shown list as plain text, for **Copy list**
/// (docs/04-browsing.md, "Notices"): one line per file, its path relative
/// to the album root and its reason separated by a tab, the unsupported
/// files first and then the files that could not be read, each in walker
/// order.
pub fn not_shown_text(unsupported: &[UnsupportedFile], errors: &[ScanErrorItem]) -> String {
    let lines = unsupported
        .iter()
        .map(|u| (u.rel_path.as_str(), u.reason.as_str()))
        .chain(
            errors
                .iter()
                .map(|e| (e.rel_path.as_str(), e.message.as_str())),
        );
    let mut out = String::new();
    for (path, reason) in lines {
        out.push_str(path);
        out.push('\t');
        out.push_str(reason);
        out.push('\n');
    }
    out
}

/// [`not_shown_text`] of `unsupported`, the last whole walk's unsupported
/// files, and every row in scope with a scan error.
pub fn not_shown(
    album: &Album,
    overlay: &Overlay,
    unsupported: &[UnsupportedFile],
) -> AppResult<String> {
    let errors = all_pages(|offset| {
        let p = scan_errors(album, overlay, offset, PAGE_MAX)?;
        Ok((p.total, p.items))
    })?;
    Ok(not_shown_text(unsupported, &errors))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_shown_text_lists_unsupported_then_unreadable_with_tabs() {
        let unsupported = [UnsupportedFile {
            rel_path: "2024/a.heic".into(),
            label: "HEIC".into(),
            reason: text::media::HEIC_UNSUPPORTED.into(),
        }];
        let errors = [ScanErrorItem {
            id: 4,
            rel_path: "b.jpg".into(),
            code: "decode".into(),
            message: "This file could not be read: bad bytes".into(),
        }];
        assert_eq!(
            not_shown_text(&unsupported, &errors),
            "2024/a.heic\tHEIC photos are not supported yet.\n\
             b.jpg\tThis file could not be read: bad bytes\n"
        );
        assert_eq!(not_shown_text(&[], &[]), "");
    }

    #[test]
    fn all_pages_reads_until_the_total() {
        let items: Vec<u32> = (0..1200).collect();
        let mut calls = 0;
        let all = all_pages(|offset| {
            calls += 1;
            Ok((count(items.len()), page(&items, offset, PAGE_MAX)))
        })
        .unwrap();
        assert_eq!(all, items);
        assert_eq!(calls, 3);
    }
}
