//! `album_changed`: rows were added, removed or changed
//! (docs/01-architecture.md, "Events").
//!
//! The scanner sends it with reason `scan`; the other reasons belong to the
//! parts that make those changes. Each event carries the album's new
//! revision ([`super::Album::next_revision`]).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Event;

/// Why the album changed. The page re-runs its query when the reason can
/// affect what it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ChangeReason {
    /// A scan added, changed or removed rows.
    Scan,
    /// The folder watcher saw files change.
    Watch,
    /// Typed info changed.
    Info,
    /// A photo's edit changed.
    Edit,
    /// An edit was saved into a file or as a copy.
    Saved,
    /// Photos were moved to the Recycle Bin or Trash.
    Trashed,
    /// Missing rows were removed.
    Tidy,
    /// Album-wide facts (bursts, quality) were recomputed.
    Facts,
    /// Tagging wrote results.
    Tags,
    /// The album's state changed (lock, view-only, conflict copy).
    Status,
}

/// The `album_changed` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AlbumChanged {
    /// The album's revision after the change.
    pub revision: u64,
    /// What changed it.
    pub reason: ChangeReason,
    /// Photos whose file content changed since the last event, so their
    /// thumbnails and tags are stale. Left out when there are none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<i64>>")]
    pub rekeyed: Vec<i64>,
}

impl Event for AlbumChanged {
    const NAME: &'static str = "album_changed";
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serialises_as_the_architecture_says() {
        let e = AlbumChanged {
            revision: 3,
            reason: ChangeReason::Scan,
            rekeyed: vec![7, 9],
        };
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            json!({ "revision": 3, "reason": "scan", "rekeyed": [7, 9] })
        );
        let quiet = AlbumChanged {
            rekeyed: vec![],
            ..e
        };
        assert_eq!(
            serde_json::to_value(&quiet).unwrap(),
            json!({ "revision": 3, "reason": "scan" })
        );
        assert_eq!(AlbumChanged::NAME, "album_changed");
    }

    #[test]
    fn reasons_have_their_documented_names() {
        let names: Vec<String> = [
            ChangeReason::Scan,
            ChangeReason::Watch,
            ChangeReason::Info,
            ChangeReason::Edit,
            ChangeReason::Saved,
            ChangeReason::Trashed,
            ChangeReason::Tidy,
            ChangeReason::Facts,
            ChangeReason::Tags,
            ChangeReason::Status,
        ]
        .iter()
        .map(|r| {
            serde_json::to_value(r)
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
        assert_eq!(
            names,
            [
                "scan", "watch", "info", "edit", "saved", "trashed", "tidy", "facts", "tags",
                "status"
            ]
        );
    }
}
