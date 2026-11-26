//! Open, close, status, scan, undo, tidy, people list (`album_`).

use crate::session::Session;
use crate::{area, command};
use owlbum_core::album::AlbumChanged;
use owlbum_core::scan::{Request, ScanProgress};
use tauri::State;

area! {
    prefix: "album",
    commands: [album_rescan, album_scan_cancel],
    events: [AlbumChanged, ScanProgress],
}

command! {
    /// Starts a whole-scope scan that retries every file with a scan error,
    /// or queues one if a scan is running (docs/03-media-and-scanning.md,
    /// "Manual Rescan"). `no_album` when no album is open.
    pub fn album_rescan(; session: State<'_, Session>) -> () {
        session.with(|open| {
            open.scanner().request(Request { manual: true });
            Ok(())
        })
    }
}

command! {
    /// Cancels the running scan. Nothing happens, and no error, when none
    /// runs or no album is open.
    pub fn album_scan_cancel(; session: State<'_, Session>) -> () {
        let _ = session.with(|open| {
            open.scanner().cancel();
            Ok(())
        });
        Ok(())
    }
}
