//! `album_status`: what the page shows about the open album as a whole,
//! in the top bar, the banners and the notices (docs/04-browsing.md,
//! "Shapes").

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::scanner::{ScanStatus, Scanner};
use super::watch::WatchState;
use crate::album::ViewOnly;
use crate::appdata::recent::display_name;
use crate::error::AppResult;

/// Files recognised but not supported yet, from the last whole walk.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UnsupportedSummary {
    /// How many, in all.
    pub count: u32,
    /// The distinct format labels found, in the order of the registry's
    /// table of recognised-but-unsupported formats.
    pub labels: Vec<String>,
}

/// The reply of `album_status`, and the `status` of `album_open`'s.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AlbumStatus {
    /// The album root, absolute.
    pub root: String,
    /// The opened folder relative to the root, `/`-separated; `""` when
    /// the root itself was opened.
    pub scope: String,
    /// The opened folder, absolute: the root, or the scope inside it. It is
    /// what the recent list holds, and what reopening it passes.
    pub folder: String,
    /// The opened folder's own name, for messages.
    pub name: String,
    /// Null when the album can be changed.
    pub view_only: Option<ViewOnly>,
    /// True when there is no database file and nothing is saved.
    pub in_memory: bool,
    /// The other computer, for `locked` and `chosen`.
    pub lock_host: Option<String>,
    /// A sync tool's copy of the database beside it, by file name.
    pub conflict_file: Option<String>,
    /// A scan is running or waiting.
    pub scanning: bool,
    /// Photos in the scope that are not missing and could be read, hidden
    /// ones included.
    pub photos: u32,
    pub unsupported: UnsupportedSummary,
    /// Rows in the scope with a scan error.
    pub unreadable: u32,
    /// The scan's counts. `watching` is false only once the folder is known
    /// not to be watched (a network share, a watch that failed or a folder
    /// that went away), not while the watch is being set up.
    pub scan: ScanStatus,
    pub revision: u64,
}

impl AlbumStatus {
    /// The status of `scanner`'s album, whose folder watch is in `watch`.
    pub fn of(scanner: &Scanner, watch: WatchState) -> AppResult<Self> {
        let album = scanner.album();
        let location = album.location();
        let safety = album.safety();
        let view_only = safety.view_only;
        let report = scanner.report();
        let opened = location.opened();
        Ok(Self {
            root: location.root.display().to_string(),
            scope: location.scope.clone(),
            folder: opened.display().to_string(),
            name: display_name(&opened.display().to_string()),
            view_only,
            in_memory: safety.in_memory,
            lock_host: safety.lock_host,
            conflict_file: safety.conflict_file,
            scanning: scanner.is_running(),
            photos: u32::try_from(album.index().len()).unwrap_or(u32::MAX),
            unsupported: UnsupportedSummary {
                count: report.unsupported.iter().map(|u| u.count).sum(),
                labels: report.unsupported.into_iter().map(|u| u.label).collect(),
            },
            unreadable: match scanner.scan_errors(0, 0) {
                Ok(errors) => errors.total,
                // A view-only album may be of a newer or damaged database
                // that cannot be read; it opens all the same.
                Err(_) if view_only.is_some() => 0,
                Err(e) => return Err(e),
            },
            scan: ScanStatus {
                watching: watch.may_watch(),
                ..scanner.status()
            },
            revision: album.revision(),
        })
    }
}

/// The reply of `album_open`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AlbumOpened {
    pub status: AlbumStatus,
    /// Where the person left off in this folder, when it has a saved
    /// session (docs/04-browsing.md, "Restoring a session"). Always null
    /// until reopening where I was is built (#37).
    pub session: Option<serde_json::Value>,
}
