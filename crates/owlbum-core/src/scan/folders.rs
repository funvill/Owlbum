//! The folders a targeted scan walks (docs/03-media-and-scanning.md,
//! "Debounce"): the folder watcher's set of folders to rescan.
//!
//! Each folder is relative to the album root, `""` being the root, and is
//! either recursive (it and every folder under it) or not (only the files
//! directly in it).

use std::collections::BTreeMap;

use super::rows;
use super::walk::is_under;

/// More folders than this to rescan, and the whole scope is scanned
/// instead.
pub const MAX_FOLDERS: usize = 500;

/// A set of folders to scan, each recursive or not.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Folders {
    /// Folder → recursive.
    map: BTreeMap<String, bool>,
}

impl Folders {
    /// No folders.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `rel_dir`. Added twice, it is recursive if either was.
    pub fn add(&mut self, rel_dir: &str, recursive: bool) {
        let entry = self.map.entry(rel_dir.to_owned()).or_insert(false);
        *entry |= recursive;
    }

    /// Every folder of both sets.
    #[must_use]
    pub fn union(mut self, other: &Self) -> Self {
        for (dir, &recursive) in &other.map {
            self.add(dir, recursive);
        }
        self
    }

    /// How many folders were added (before [`Self::normalised`]).
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// True when no folder was added.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// The folders to walk, each once: a folder under a recursive one is
    /// left out, since that walk covers it. In path order, parents first.
    pub fn normalised(&self) -> Vec<(String, bool)> {
        let covered = |dir: &str| {
            self.map
                .iter()
                .any(|(d, &r)| r && d != dir && is_under(dir, d))
        };
        self.map
            .iter()
            .filter(|(dir, _)| !covered(dir))
            .map(|(dir, &r)| (dir.clone(), r))
            .collect()
    }

    /// True when the file at `rel_path` lies in the set: directly in a
    /// folder of it, or anywhere under a recursive one.
    pub fn holds(&self, rel_path: &str) -> bool {
        let dir = rows::rel_dir(rel_path);
        self.map
            .iter()
            .any(|(d, &recursive)| d == dir || (recursive && is_under(rel_path, d)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(items: &[(&str, bool)]) -> Folders {
        let mut f = Folders::new();
        for (d, r) in items {
            f.add(d, *r);
        }
        f
    }

    #[test]
    fn a_folder_added_twice_is_recursive_if_either_was() {
        let f = set(&[("a", false), ("a", true), ("b", true), ("b", false)]);
        assert_eq!(
            f.normalised(),
            [("a".to_owned(), true), ("b".to_owned(), true)]
        );
        assert_eq!(f.len(), 2);
    }

    #[test]
    fn folders_under_a_recursive_one_are_walked_by_it() {
        let f = set(&[
            ("a/b", false),
            ("a", true),
            ("a/b/c", true),
            ("ab", false),
            ("x/y", false),
            ("x", false),
        ]);
        assert_eq!(
            f.normalised(),
            [
                ("a".to_owned(), true),
                ("ab".to_owned(), false),
                ("x".to_owned(), false),
                ("x/y".to_owned(), false),
            ]
        );
    }

    #[test]
    fn the_root_recursive_covers_everything() {
        let f = set(&[("", true), ("a", false), ("b/c", true)]);
        assert_eq!(f.normalised(), [(String::new(), true)]);
    }

    #[test]
    fn holds_files_directly_in_a_folder_or_under_a_recursive_one() {
        let f = set(&[("a", false), ("b", true), ("", false)]);
        assert!(f.holds("a/x.jpg"));
        assert!(!f.holds("a/sub/x.jpg"));
        assert!(f.holds("b/x.jpg"));
        assert!(f.holds("b/sub/deep/x.jpg"));
        assert!(f.holds("top.jpg"));
        assert!(!f.holds("c/x.jpg"));
        assert!(!f.holds("ab/x.jpg"));
    }

    #[test]
    fn union_keeps_both() {
        let f = set(&[("a", false)]).union(&set(&[("a", true), ("b", false)]));
        assert_eq!(
            f.normalised(),
            [("a".to_owned(), true), ("b".to_owned(), false)]
        );
    }
}
