//! The timeline's groups (docs/04-browsing.md, "Timeline view").
//!
//! A photo's group is its written calendar date: the effective date is kept
//! as wall-clock seconds from 1970-01-01 with no time zone (see
//! [`crate::index::parse_date`]), so whole days of it are the written days
//! and nothing is ever converted through a zone.

use super::shape::{Group, GroupUnit, QueryTimelineUnit};
use crate::appdata::clock::civil_from_days;
use crate::index::IndexView;

/// The span of dated photos, in days from the first to the last, above
/// which `auto` picks Month.
pub const AUTO_DAYS: i64 = 90;

/// The key of the last group, for photos with no date.
pub const NO_DATE_KEY: &str = "none";

/// The groups of `tiles`, which are in date order (undated last), and the
/// unit used. A group's `count` counts its tiles and its `photos` the
/// photos they stand for: `weight(i)` is how many the tile at `i` does (a
/// closed stack's size on its face, else one), so a stack counts in the
/// group of its face.
pub fn groups(
    view: &IndexView<'_>,
    tiles: &[u32],
    unit: QueryTimelineUnit,
    weight: impl Fn(usize) -> u32,
) -> (Vec<Group>, GroupUnit) {
    let days: Vec<Option<i64>> = tiles
        .iter()
        .map(|&id| {
            view.get(id)
                .and_then(|p| p.date())
                .map(|d| d.div_euclid(86_400))
        })
        .collect();
    let unit = match unit {
        QueryTimelineUnit::Day => GroupUnit::Day,
        QueryTimelineUnit::Month => GroupUnit::Month,
        QueryTimelineUnit::Auto => {
            let first = days.iter().flatten().next();
            let last = days.iter().flatten().next_back();
            match (first, last) {
                (Some(a), Some(b)) if b - a > AUTO_DAYS => GroupUnit::Month,
                _ => GroupUnit::Day,
            }
        }
    };

    let mut out: Vec<Group> = Vec::new();
    // The day of the group being built, so the calendar is worked out once
    // per day, not once per tile; `None` once the undated group started.
    let mut last_day: Option<Option<i64>> = None;
    for (i, day) in days.iter().enumerate() {
        if last_day != Some(*day) {
            last_day = Some(*day);
            let key = key_of(*day, unit);
            if out.last().is_none_or(|g| g.key != key) {
                out.push(Group {
                    key,
                    start: count(i),
                    count: 0,
                    photos: 0,
                });
            }
        }
        if let Some(g) = out.last_mut() {
            g.count += 1;
            g.photos += weight(i);
        }
    }
    (out, unit)
}

/// `2026-07-14`, `2026-07` or `none`.
fn key_of(day: Option<i64>, unit: GroupUnit) -> String {
    let Some(day) = day else {
        return NO_DATE_KEY.to_owned();
    };
    let (y, m, d) = civil_from_days(day);
    match unit {
        GroupUnit::Day => format!("{y:04}-{m:02}-{d:02}"),
        GroupUnit::Month => format!("{y:04}-{m:02}"),
    }
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_the_written_date() {
        // 2026-07-14 is day 20,648 from 1970-01-01.
        assert_eq!(key_of(Some(20_648), GroupUnit::Day), "2026-07-14");
        assert_eq!(key_of(Some(20_648), GroupUnit::Month), "2026-07");
        assert_eq!(key_of(Some(0), GroupUnit::Day), "1970-01-01");
        assert_eq!(key_of(Some(-1), GroupUnit::Day), "1969-12-31");
        assert_eq!(key_of(None, GroupUnit::Day), "none");
    }
}
