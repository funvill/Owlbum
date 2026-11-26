//! Open, close, status, scan, undo, tidy, people list (`album_`).

use crate::session::Session;
use crate::{area, command};
use owlbum_core::album::AlbumChanged;
use owlbum_core::scan::report::{ScanErrors, ScanReport, UnsupportedFiles};
use owlbum_core::scan::{Request, ScanProgress};
use tauri::State;

area! {
    prefix: "album",
    commands: [
        album_rescan,
        album_scan_cancel,
        album_scan_errors,
        album_scan_report,
        album_unsupported,
    ],
    events: [AlbumChanged, ScanProgress],
}

command! {
    /// Starts a whole-scope scan that retries every file with a scan error,
    /// or queues one if a scan is running (docs/03-media-and-scanning.md,
    /// "Manual Rescan"). `no_album` when no album is open.
    pub fn album_rescan(; session: State<'_, Session>) -> () {
        session.with(|open| {
            open.scanner().request(Request::manual());
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

command! {
    /// Counts and short lists from the last whole walk: unreadable folders,
    /// nested albums, skipped links, online-only files, unreadable names,
    /// unsupported files by label, and notices
    /// (docs/03-media-and-scanning.md, "Commands"). All empty before a
    /// walk has ended. `no_album` when no album is open.
    pub fn album_scan_report(; session: State<'_, Session>) -> ScanReport {
        session.with(|open| Ok(open.scanner().report()))
    }
}

command! {
    /// One page of the rows in scope with a scan error, in walker order.
    /// `limit` is at most 500; more is taken as 500. `no_album` when no
    /// album is open.
    pub fn album_scan_errors(offset: u32, limit: u32; session: State<'_, Session>) -> ScanErrors {
        session.with(|open| open.scanner().scan_errors(offset, limit))
    }
}

command! {
    /// One page of the files recognised but not supported yet, from the
    /// last whole walk, in walker order. `limit` is at most 500; more is
    /// taken as 500. `no_album` when no album is open.
    pub fn album_unsupported(offset: u32, limit: u32; session: State<'_, Session>) -> UnsupportedFiles {
        session.with(|open| Ok(open.scanner().unsupported(offset, limit)))
    }
}
