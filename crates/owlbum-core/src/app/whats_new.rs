//! What's new after an update (docs/08-desktop-and-release.md, "What's
//! new"). The notes are the released sections of `CHANGELOG.md`, compiled
//! in: nothing is fetched.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The repository's changelog, compiled in.
pub const CHANGELOG: &str = include_str!("../../../../CHANGELOG.md");

/// Any non-empty value stops What's new from showing at start.
pub const OFF_VAR: &str = "OWLBUM_NO_WHATS_NEW";

/// One released version's notes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WhatsNewEntry {
    /// `major.minor.patch`.
    pub version: String,
    /// Its groups (Added, Changed, Fixed, Removed), in the file's order.
    pub sections: Vec<WhatsNewSection>,
}

/// One group of a version's notes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WhatsNewSection {
    /// `Added`, `Changed`, `Fixed` or `Removed`.
    pub heading: String,
    /// One line per item, wrapped lines joined.
    pub items: Vec<String>,
}

/// The released versions of a Keep a Changelog file, newest first as the
/// file has them. `Unreleased` and versions with no items are left out.
pub fn parse(changelog: &str) -> Vec<WhatsNewEntry> {
    let mut entries: Vec<WhatsNewEntry> = Vec::new();
    // Whether the lines being read belong to a released version.
    let mut released = false;
    for line in changelog.lines() {
        if let Some(head) = line.strip_prefix("## ") {
            released = false;
            let Some(version) = head
                .strip_prefix('[')
                .and_then(|h| h.split(']').next())
                .filter(|v| parts(v).is_some())
            else {
                continue;
            };
            entries.push(WhatsNewEntry {
                version: version.to_owned(),
                sections: Vec::new(),
            });
            released = true;
            continue;
        }
        let Some(entry) = entries.last_mut().filter(|_| released) else {
            continue;
        };
        if let Some(heading) = line.strip_prefix("### ") {
            entry.sections.push(WhatsNewSection {
                heading: heading.trim().to_owned(),
                items: Vec::new(),
            });
        } else if let Some(section) = entry.sections.last_mut() {
            let trimmed = line.trim();
            if let Some(item) = trimmed.strip_prefix("- ") {
                section.items.push(item.trim().to_owned());
            } else if !trimmed.is_empty()
                && line.starts_with(' ')
                && let Some(item) = section.items.last_mut()
            {
                item.push(' ');
                item.push_str(trimmed);
            }
        }
    }
    for entry in &mut entries {
        entry.sections.retain(|s| !s.items.is_empty());
    }
    entries.retain(|e| !e.sections.is_empty());
    entries
}

/// A version's three numbers, or `None` when it is not `a.b.c`.
fn parts(version: &str) -> Option<[u64; 3]> {
    let mut it = version.trim().split('.').map(|p| p.parse::<u64>().ok());
    let v = [it.next()??, it.next()??, it.next()??];
    it.next().is_none().then_some(v)
}

/// Compares two versions part by part as numbers (`0.1.10` is newer than
/// `0.1.9`). A version that does not parse sorts below every other.
pub fn compare(a: &str, b: &str) -> Ordering {
    parts(a).cmp(&parts(b))
}

/// What `app_startup` does about What's new.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AtStart {
    /// The entries to show, newest first. Empty shows nothing.
    pub show: Vec<WhatsNewEntry>,
    /// Record this as `whats_new.last_seen` at once. When entries are shown
    /// it is recorded only when their dialog closes
    /// (`app_whats_new_seen`), so a crash during start shows them again.
    pub record: Option<String>,
}

/// The rule of docs/08, "When it shows". `off` is a development build or
/// `OWLBUM_NO_WHATS_NEW` set: nothing is shown or recorded.
pub fn at_start(
    running: &str,
    last_seen: Option<&str>,
    off: bool,
    entries: &[WhatsNewEntry],
) -> AtStart {
    if off {
        return AtStart::default();
    }
    let Some(last_seen) = last_seen else {
        return AtStart {
            show: Vec::new(),
            record: Some(running.to_owned()),
        };
    };
    match compare(running, last_seen) {
        Ordering::Equal => AtStart::default(),
        Ordering::Less => AtStart {
            show: Vec::new(),
            record: Some(running.to_owned()),
        },
        Ordering::Greater => {
            let show: Vec<WhatsNewEntry> = entries
                .iter()
                .filter(|e| {
                    compare(&e.version, last_seen) == Ordering::Greater
                        && compare(&e.version, running) != Ordering::Greater
                })
                .cloned()
                .collect();
            let record = show.is_empty().then(|| running.to_owned());
            AtStart { show, record }
        }
    }
}

/// The running version's entry, for Help > What's new: empty when the
/// changelog has none.
pub fn for_version(running: &str, entries: &[WhatsNewEntry]) -> Vec<WhatsNewEntry> {
    entries
        .iter()
        .filter(|e| compare(&e.version, running) == Ordering::Equal)
        .cloned()
        .collect()
}

/// Whether `OWLBUM_NO_WHATS_NEW` turns What's new off: any non-empty value.
pub fn env_off(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "# Changelog\n\nIntro text.\n\n## [Unreleased]\n\n### Added\n\n- Not out yet.\n\n## [0.1.10] - 2026-12-01\n\n### Added\n\n- A long item that\n  wraps onto a second line.\n- Short one.\n\n### Fixed\n\n- A fix.\n\n## [0.1.9] - 2026-11-30\n\n### Changed\n\n- Nine.\n\n## [0.1.8] - 2026-11-29\n\n## [0.1.0] - 2026-11-26\n\n### Added\n\n- The first version.\n";

    fn versions(e: &[WhatsNewEntry]) -> Vec<&str> {
        e.iter().map(|e| e.version.as_str()).collect()
    }

    #[test]
    fn the_changelog_parses_into_released_versions_newest_first() {
        let e = parse(LOG);
        assert_eq!(
            versions(&e),
            ["0.1.10", "0.1.9", "0.1.0"],
            "no Unreleased, no empty 0.1.8"
        );
        assert_eq!(e[0].sections[0].heading, "Added");
        assert_eq!(
            e[0].sections[0].items,
            ["A long item that wraps onto a second line.", "Short one."]
        );
        assert_eq!(e[0].sections[1].items, ["A fix."]);
    }

    #[test]
    fn the_real_changelog_parses_newest_first_and_not_above_the_running_version() {
        let e = parse(CHANGELOG);
        assert!(!e.is_empty());
        for pair in e.windows(2) {
            assert_eq!(
                compare(&pair[0].version, &pair[1].version),
                Ordering::Greater
            );
        }
        let running = crate::app::version().version;
        assert_ne!(compare(&e[0].version, &running), Ordering::Greater);
        assert!(
            e.iter()
                .all(|e| e.sections.iter().all(|s| !s.items.is_empty()))
        );
    }

    #[test]
    fn versions_compare_as_numbers() {
        assert_eq!(compare("0.1.10", "0.1.9"), Ordering::Greater);
        assert_eq!(compare("1.0.0", "0.9.99"), Ordering::Greater);
        assert_eq!(compare("0.1.2", "0.1.2"), Ordering::Equal);
        assert_eq!(compare("junk", "0.0.0"), Ordering::Less);
    }

    #[test]
    fn a_first_install_shows_nothing_and_records_at_once() {
        let s = at_start("0.1.10", None, false, &parse(LOG));
        assert!(s.show.is_empty());
        assert_eq!(s.record.as_deref(), Some("0.1.10"));
    }

    #[test]
    fn an_update_shows_every_version_since_the_last_seen_and_records_later() {
        let s = at_start("0.1.10", Some("0.1.0"), false, &parse(LOG));
        assert_eq!(versions(&s.show), ["0.1.10", "0.1.9"]);
        assert_eq!(s.record, None, "recorded when the dialog closes");
    }

    #[test]
    fn an_update_with_no_notes_records_at_once() {
        let s = at_start("0.1.8", Some("0.1.0"), false, &parse(LOG));
        assert!(s.show.is_empty());
        assert_eq!(s.record.as_deref(), Some("0.1.8"));
    }

    #[test]
    fn the_same_version_does_nothing_and_a_downgrade_records() {
        assert_eq!(
            at_start("0.1.9", Some("0.1.9"), false, &parse(LOG)),
            AtStart::default()
        );
        let down = at_start("0.1.0", Some("0.1.9"), false, &parse(LOG));
        assert!(down.show.is_empty());
        assert_eq!(down.record.as_deref(), Some("0.1.0"));
    }

    #[test]
    fn turned_off_does_nothing() {
        assert_eq!(
            at_start("0.1.10", Some("0.1.0"), true, &parse(LOG)),
            AtStart::default()
        );
        assert_eq!(
            at_start("0.1.10", None, true, &parse(LOG)),
            AtStart::default()
        );
        assert!(env_off(Some("1".as_ref())));
        assert!(!env_off(Some("".as_ref())));
        assert!(!env_off(None));
    }

    #[test]
    fn help_shows_the_running_versions_entry_only() {
        assert_eq!(versions(&for_version("0.1.9", &parse(LOG))), ["0.1.9"]);
        assert!(for_version("0.1.8", &parse(LOG)).is_empty());
    }
}
