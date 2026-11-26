//! The pure part of folder watching: from one changed path to the folders
//! to rescan, and the debounce (docs/03-media-and-scanning.md, "What is
//! watched" and "Debounce"). No file system and no clock: the caller says
//! what the path is now and what time it is.

use std::time::{Duration, Instant};

use crate::media;
use crate::scan::folders::{Folders, MAX_FOLDERS};
use crate::scan::rows;
use crate::scan::walk::{SYSTEM_FOLDERS, is_under};

/// What happened to a path, as far as the rules care.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Created.
    Created,
    /// Renamed or moved, from or to this path.
    Renamed,
    /// Written, resized, or its attributes changed.
    Changed,
    /// Removed.
    Removed,
}

/// What is at a path now, looked at without following links.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stat {
    Folder,
    File,
    /// A symbolic link or junction: the walker never follows one.
    Link,
    /// Nothing: it was removed or renamed away.
    Gone,
}

/// What one changed path asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seen {
    /// Nothing: the path is one the walker leaves out, or not a picture.
    Nothing,
    /// Rescan this folder (relative to the album root), and the folders
    /// under it when `recursive`.
    Folder { rel_dir: String, recursive: bool },
    /// The watched folder itself is gone (an unplugged drive, a removed
    /// share): the watch cannot be trusted any more.
    ScopeGone,
}

/// The rules for one changed path. `rel_path` is relative to the album
/// root and lies in `scope` (or is it). `has_rows_under(folder)` answers
/// whether any row's path lies under that folder: a gone path is a folder
/// when it held rows.
pub fn seen(
    scope: &str,
    rel_path: &str,
    kind: Kind,
    stat: Stat,
    has_rows_under: impl FnOnce(&str) -> bool,
) -> Seen {
    if rel_path == scope {
        return match stat {
            Stat::Folder => Seen::Nothing,
            _ => Seen::ScopeGone,
        };
    }
    if !is_under(rel_path, scope) || left_out(&rel_path[scope.len()..]) {
        return Seen::Nothing;
    }
    let parent = rows::rel_dir(rel_path);
    let folder = |rel_dir: &str, recursive| Seen::Folder {
        rel_dir: rel_dir.to_owned(),
        recursive,
    };
    match stat {
        // A folder whose content changed reports each changed child too;
        // only a new folder needs its own walk.
        Stat::Folder => match kind {
            Kind::Created | Kind::Renamed => folder(rel_path, true),
            Kind::Changed | Kind::Removed => Seen::Nothing,
        },
        Stat::File if is_media(rows::file_name(rel_path)) => folder(parent, false),
        Stat::File | Stat::Link => Seen::Nothing,
        // Gone, so file or folder cannot be told by looking: a folder if
        // rows lie under it, else a file.
        Stat::Gone if has_rows_under(rel_path) => folder(parent, true),
        Stat::Gone if is_media(rows::file_name(rel_path)) => folder(parent, false),
        Stat::Gone => Seen::Nothing,
    }
}

/// True when a path below the scope passes through a name the walker
/// leaves out: a dot name (`.owlbum`, whose database journal changes on
/// every write) or a system folder.
fn left_out(below_scope: &str) -> bool {
    below_scope
        .split('/')
        .filter(|c| !c.is_empty())
        .any(|c| c.starts_with('.') || SYSTEM_FOLDERS.iter().any(|s| s.eq_ignore_ascii_case(c)))
}

/// A name the walker lists or counts: supported, or recognised but not
/// supported yet.
fn is_media(name: &str) -> bool {
    media::is_supported(name) || media::unsupported_of(name).is_some()
}

/// The watch's timing (docs/03, "Constants").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    /// The set is acted on after this long with no new event…
    pub quiet: Duration,
    /// …or this long after the first event, whichever comes first.
    pub most: Duration,
    /// More folders than this, and the whole scope is scanned.
    pub max_folders: usize,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            quiet: Duration::from_secs(2),
            most: Duration::from_secs(10),
            max_folders: MAX_FOLDERS,
        }
    }
}

/// The folders to rescan, collected until the events go quiet.
#[derive(Debug, Clone, Default)]
pub struct Debounce {
    timing: Timing,
    folders: Folders,
    whole: bool,
    first: Option<Instant>,
    last: Option<Instant>,
}

/// What a due set asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Due {
    /// These folders.
    Folders(Folders),
    /// The whole scope.
    Whole,
}

impl Debounce {
    pub fn new(timing: Timing) -> Self {
        Self {
            timing,
            ..Self::default()
        }
    }

    fn event(&mut self, now: Instant) {
        self.first.get_or_insert(now);
        self.last = Some(now);
    }

    /// One changed path's folder, seen at `now`.
    pub fn add(&mut self, rel_dir: &str, recursive: bool, now: Instant) {
        self.event(now);
        if !self.whole {
            self.folders.add(rel_dir, recursive);
            if self.folders.len() > self.timing.max_folders {
                self.whole();
            }
        }
    }

    /// The watcher lost events (an overflow, "rescan needed"): the whole
    /// scope, seen at `now`.
    pub fn add_whole(&mut self, now: Instant) {
        self.event(now);
        self.whole();
    }

    fn whole(&mut self) {
        self.whole = true;
        self.folders = Folders::new();
    }

    /// True when nothing waits.
    pub fn is_empty(&self) -> bool {
        self.first.is_none()
    }

    /// When the waiting set is due: 2 s after the last event or 10 s after
    /// the first, whichever comes first. `None` when nothing waits.
    pub fn due_at(&self) -> Option<Instant> {
        let (first, last) = (self.first?, self.last?);
        Some((last + self.timing.quiet).min(first + self.timing.most))
    }

    /// The waiting set, emptied, when it is due at `now`.
    pub fn take_due(&mut self, now: Instant) -> Option<Due> {
        if self.due_at()? > now {
            return None;
        }
        self.take()
    }

    /// The waiting set, emptied, due or not.
    pub fn take(&mut self) -> Option<Due> {
        self.first?;
        let due = if self.whole {
            Due::Whole
        } else {
            Due::Folders(std::mem::take(&mut self.folders))
        };
        *self = Self::new(self.timing);
        Some(due)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(rel_dir: &str, recursive: bool) -> Seen {
        Seen::Folder {
            rel_dir: rel_dir.to_owned(),
            recursive,
        }
    }

    fn no_rows(_: &str) -> bool {
        false
    }

    #[test]
    fn a_file_created_changed_removed_or_renamed_rescans_its_folder() {
        for kind in [Kind::Created, Kind::Changed, Kind::Renamed] {
            assert_eq!(
                seen("", "2024/a.jpg", kind, Stat::File, no_rows),
                folder("2024", false),
                "{kind:?}"
            );
        }
        for kind in [Kind::Removed, Kind::Renamed] {
            assert_eq!(
                seen("", "2024/a.jpg", kind, Stat::Gone, no_rows),
                folder("2024", false),
                "{kind:?}"
            );
        }
        assert_eq!(
            seen("", "top.png", Kind::Created, Stat::File, no_rows),
            folder("", false)
        );
    }

    #[test]
    fn a_rename_across_folders_rescans_both_parents() {
        // The two halves of a rename arrive as two paths.
        let from = seen("", "a/x.jpg", Kind::Renamed, Stat::Gone, no_rows);
        let to = seen("", "b/x.jpg", Kind::Renamed, Stat::File, no_rows);
        assert_eq!((from, to), (folder("a", false), folder("b", false)));
    }

    #[test]
    fn a_folder_created_or_renamed_into_place_is_rescanned_whole() {
        assert_eq!(
            seen("", "2024/trip", Kind::Created, Stat::Folder, no_rows),
            folder("2024/trip", true)
        );
        assert_eq!(
            seen("", "2024/trip", Kind::Renamed, Stat::Folder, no_rows),
            folder("2024/trip", true)
        );
        // A folder whose content changed: its children report themselves.
        assert_eq!(
            seen("", "2024/trip", Kind::Changed, Stat::Folder, no_rows),
            Seen::Nothing
        );
    }

    #[test]
    fn a_gone_path_with_rows_under_it_was_a_folder() {
        let rows = |f: &str| f == "2024/trip";
        assert_eq!(
            seen("", "2024/trip", Kind::Removed, Stat::Gone, rows),
            folder("2024", true)
        );
        assert_eq!(
            seen("", "2024/trip", Kind::Renamed, Stat::Gone, rows),
            folder("2024", true)
        );
        // Named like a picture but held rows: still a folder.
        let rows = |f: &str| f == "odd.jpg";
        assert_eq!(
            seen("", "odd.jpg", Kind::Removed, Stat::Gone, rows),
            folder("", true)
        );
    }

    #[test]
    fn a_gone_path_with_no_rows_and_no_picture_name_is_nothing() {
        assert_eq!(
            seen("", "2024/empty", Kind::Removed, Stat::Gone, no_rows),
            Seen::Nothing
        );
    }

    #[test]
    fn dot_names_and_system_folders_are_dropped() {
        for path in [
            ".owlbum/owlbum.sqlite-journal",
            ".owlbum/owlbum.sqlite",
            "2024/.hidden/a.jpg",
            "2024/._a.jpg",
            ".git/x.jpg",
            "$RECYCLE.BIN/S-1-5/a.jpg",
            "2024/@eaDir/a.jpg/SYNOPHOTO_THUMB_M.jpg",
            "__macosx/a.jpg",
        ] {
            for stat in [Stat::File, Stat::Gone, Stat::Folder] {
                assert_eq!(
                    seen("", path, Kind::Created, stat, |_| true),
                    Seen::Nothing,
                    "{path} {stat:?}"
                );
            }
        }
    }

    #[test]
    fn files_that_are_not_pictures_and_links_are_dropped() {
        assert_eq!(
            seen("", "notes.txt", Kind::Changed, Stat::File, no_rows),
            Seen::Nothing
        );
        assert_eq!(
            seen("", "a.jpg", Kind::Created, Stat::Link, no_rows),
            Seen::Nothing
        );
        assert_eq!(
            seen("", "linked", Kind::Created, Stat::Link, no_rows),
            Seen::Nothing
        );
    }

    #[test]
    fn a_recognised_but_unsupported_file_rescans_its_folder() {
        assert_eq!(
            seen("", "2024/a.heic", Kind::Created, Stat::File, no_rows),
            folder("2024", false)
        );
    }

    #[test]
    fn paths_are_judged_below_the_scope() {
        // A scoped open of a folder whose own name starts with a dot.
        assert_eq!(
            seen(
                ".x/2024",
                ".x/2024/a.jpg",
                Kind::Created,
                Stat::File,
                no_rows
            ),
            folder(".x/2024", false)
        );
        assert_eq!(
            seen("2024", "2025/a.jpg", Kind::Created, Stat::File, no_rows),
            Seen::Nothing
        );
    }

    #[test]
    fn the_scope_itself_gone_ends_the_watch() {
        for kind in [Kind::Removed, Kind::Renamed] {
            assert_eq!(seen("", "", kind, Stat::Gone, no_rows), Seen::ScopeGone);
            assert_eq!(
                seen("2024", "2024", kind, Stat::Gone, no_rows),
                Seen::ScopeGone
            );
        }
        assert_eq!(
            seen("", "", Kind::Changed, Stat::Folder, no_rows),
            Seen::Nothing
        );
    }

    // ---- Debounce, with a fake clock ----

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn the_set_is_due_two_seconds_after_the_last_event() {
        let t0 = Instant::now();
        let mut d = Debounce::new(Timing::default());
        assert!(d.is_empty());
        assert_eq!(d.due_at(), None);
        d.add("a", false, t0);
        d.add("b", false, t0 + ms(1500));
        assert_eq!(d.due_at(), Some(t0 + ms(3500)));
        assert_eq!(d.take_due(t0 + ms(3499)), None);
        let Some(Due::Folders(f)) = d.take_due(t0 + ms(3500)) else {
            panic!("due");
        };
        assert_eq!(
            f.normalised(),
            [("a".to_owned(), false), ("b".to_owned(), false)]
        );
        assert!(d.is_empty());
        assert_eq!(d.take_due(t0 + ms(9000)), None);
    }

    #[test]
    fn a_steady_stream_of_events_is_acted_on_ten_seconds_after_the_first() {
        let t0 = Instant::now();
        let mut d = Debounce::new(Timing::default());
        for i in 0..=20u64 {
            d.add("a", false, t0 + ms(i * 500));
            if i * 500 < 10_000 {
                assert_eq!(d.take_due(t0 + ms(i * 500)), None, "{i}");
            }
        }
        assert_eq!(d.due_at(), Some(t0 + ms(10_000)));
        assert!(d.take_due(t0 + ms(10_000)).is_some());
        // The next event starts a new set.
        d.add("a", false, t0 + ms(10_100));
        assert_eq!(d.due_at(), Some(t0 + ms(12_100)));
    }

    #[test]
    fn more_than_500_folders_is_the_whole_scope() {
        let t0 = Instant::now();
        let mut d = Debounce::new(Timing::default());
        for i in 0..500 {
            d.add(&format!("f{i}"), false, t0);
        }
        d.add("f0", true, t0);
        let mut copy = d.clone();
        assert!(matches!(copy.take(), Some(Due::Folders(f)) if f.len() == 500));
        d.add("f500", false, t0);
        assert_eq!(d.take(), Some(Due::Whole));
    }

    #[test]
    fn a_lost_event_stream_is_the_whole_scope() {
        let t0 = Instant::now();
        let mut d = Debounce::new(Timing::default());
        d.add("a", false, t0);
        d.add_whole(t0 + ms(10));
        d.add("b", false, t0 + ms(20));
        assert_eq!(d.due_at(), Some(t0 + ms(2020)));
        assert_eq!(d.take_due(t0 + ms(2020)), Some(Due::Whole));
    }
}
