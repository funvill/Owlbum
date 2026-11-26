//! Open, close, status, scan, undo, tidy, people list (`album_`).

use std::path::Path;

use crate::commands::off_main;
use crate::session::{Session, open_album};
use crate::{area, command};
use owlbum_core::album::undo::{self, Undone};
use owlbum_core::album::{Album, AlbumChanged, ChangeReason, OpenOptions};
use owlbum_core::appdata::AppData;
use owlbum_core::info::typed::{self, PersonCount};
use owlbum_core::scan::report::{ScanErrors, ScanReport, UnsupportedFiles};
use owlbum_core::scan::{AlbumOpened, AlbumStatus, Request, ScanProgress};
use owlbum_core::{AppError, text};
use tauri::{AppHandle, Manager, Runtime, State};

area! {
    prefix: "album",
    commands: [
        album_close,
        album_open,
        album_people,
        album_rescan,
        album_scan_cancel,
        album_scan_errors,
        album_scan_report,
        album_status,
        album_undo,
        album_unsupported,
    ],
    events: [AlbumChanged, ScanProgress],
}

command! {
    /// Opens the album for `path` (a file opens the folder that holds it)
    /// and starts its scan in the background. Any open album is closed
    /// first, even when this one then fails to open. The folder moves to
    /// the top of the recent list.
    ///
    /// Errors: `not_a_folder`, `folder_unreadable`, and `album_lock_stale`
    /// and `album_damaged` with their `details`, which the page answers by
    /// calling again with `options` (docs/04-browsing.md, "Opening a
    /// folder").
    pub async fn album_open<R: Runtime>(
        path: String,
        options: Option<OpenOptions>;
        app: AppHandle<R>
    ) -> AlbumOpened {
        off_main(text::album::OPENING_DATABASE, move || {
            let session = app.state::<Session>();
            let _one = session.one_at_a_time();
            session.close();
            let folder = owlbum_core::app::folder_of(Path::new(&path));
            let album = Album::open(&folder, &options.unwrap_or_default())?;
            let opened = album.location().opened().display().to_string();
            tracing::info!(
                target: "album",
                "open folder={} root={} view_only={:?}",
                opened,
                album.location().root.display(),
                album.safety().view_only,
            );
            open_album(&app, album);
            if let Some(data) = app.try_state::<AppData>()
                && let Err(e) = data.touch_recent(&opened, now_ms())
            {
                tracing::warn!(target: "album", "the recent list could not be updated: {}", e.message);
            }
            let status = match session.status() {
                Ok(Some(status)) => status,
                // Nothing is left open that the page does not know of.
                Ok(None) => {
                    session.close();
                    return Err(AppError::internal(text::album::OPENING_DATABASE));
                }
                Err(e) => {
                    session.close();
                    return Err(e);
                }
            };
            Ok(AlbumOpened {
                status,
                session: None,
            })
        })
        .await
    }
}

command! {
    /// Closes the open album: ends its scan and its watch and releases its
    /// lock. Nothing happens when none is open.
    pub async fn album_close<R: Runtime>(; app: AppHandle<R>) -> () {
        off_main(text::album::OPENING_DATABASE, move || {
            let session = app.state::<Session>();
            let _one = session.one_at_a_time();
            if session.is_open() {
                tracing::info!(target: "album", "close");
            }
            session.close();
            Ok(())
        })
        .await
    }
}

command! {
    /// The open album's status, or null when none is open
    /// (docs/04-browsing.md, "Shapes").
    pub async fn album_status<R: Runtime>(; app: AppHandle<R>) -> Option<AlbumStatus> {
        off_main(text::album::READING_DATABASE, move || app.state::<Session>().status()).await
    }
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
    pub async fn album_scan_errors<R: Runtime>(offset: u32, limit: u32; app: AppHandle<R>) -> ScanErrors {
        off_main(text::album::READING_DATABASE, move || {
            let scanner = app.state::<Session>().with(|open| Ok(std::sync::Arc::clone(open.scanner_arc())))?;
            scanner.scan_errors(offset, limit)
        })
        .await
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

/// Now, in milliseconds since the epoch.
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

command! {
    /// The names used in the album, with the number of photos each is on,
    /// most used first: the People field's suggestions
    /// (docs/05-organising.md, "People suggestions").
    pub async fn album_people(; session: State<'_, Session>) -> Vec<PersonCount> {
        let scanner = session.queries()?.0;
        off_main(text::photo::READING_DETAIL, move || typed::album_people(scanner.album())).await
    }
}

command! {
    /// Takes back the newest change to typed info (docs/05-organising.md,
    /// "What `album_undo` does"). Answers the outcome (`nothing`, `done`,
    /// `partial`, `gone`), the step's label and the photos put back, and
    /// sends `album_changed` with reason `info` when any were. Records no
    /// new step. `no_album` when no album is open.
    pub async fn album_undo<R: Runtime>(; app: AppHandle<R>) -> Undone {
        let scanner = app.state::<Session>().queries()?.0;
        let undone = off_main(text::album::WRITING_DATABASE, move || undo::undo(scanner.album())).await?;
        if !undone.ids.is_empty() {
            let _ = crate::events::emit(
                &app,
                &AlbumChanged {
                    revision: undone.revision,
                    reason: ChangeReason::Info,
                    rekeyed: Vec::new(),
                },
            );
        }
        Ok(undone)
    }
}
