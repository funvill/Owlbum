//! Open, close, status, scan, undo, tidy, people list (`album_`).

use std::path::Path;

use crate::commands::off_main;
use crate::session::{Session, open_album};
use crate::{area, command};
use owlbum_core::album::reopen;
use owlbum_core::album::undo::{self, Undone};
use owlbum_core::album::{Album, AlbumChanged, ChangeReason, OpenOptions};
use owlbum_core::appdata::AppData;
use owlbum_core::info::typed::{self, PersonCount};
use owlbum_core::organise::tidy::{self, TidyFolder, TidyRemoved};
use owlbum_core::scan::report::{ScanErrors, ScanReport, UnsupportedFiles};
use owlbum_core::scan::{AlbumOpened, AlbumStatus, Request, ScanProgress};
use owlbum_core::{AppError, text};
use tauri::{AppHandle, Manager, Runtime, State};

area! {
    prefix: "album",
    commands: [
        album_close,
        album_copy_not_shown,
        album_open,
        album_people,
        album_rescan,
        album_scan_cancel,
        album_scan_errors,
        album_scan_report,
        album_status,
        album_tidy_list,
        album_tidy_remove,
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
            // Read before the album moves into the session; a session that
            // cannot be read opens the folder without it.
            let saved = app
                .try_state::<AppData>()
                .and_then(|data| restored_session(&data, &album, &opened));
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
                session: saved,
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

command! {
    /// Puts the whole Files not shown list on the clipboard as plain text:
    /// one line per file, its path relative to the album root, a tab and
    /// its reason; the unsupported files first, then the files that could
    /// not be read (docs/04-browsing.md, "Notices"). Answers how many
    /// lines were copied. Errors: `no_album`, `clipboard_failed`.
    pub async fn album_copy_not_shown<R: Runtime>(; app: AppHandle<R>) -> u32 {
        let desktop = crate::commands::file::desktop(&app)?;
        off_main(text::album::COPYING_LIST, move || {
            let scanner = app.state::<Session>().with(|open| Ok(std::sync::Arc::clone(open.scanner_arc())))?;
            let list = scanner.not_shown_text()?;
            desktop.copy_text(&list).map_err(|r| {
                AppError::new(owlbum_core::ErrorCode::ClipboardFailed, text::album::copy_list_failed(&r))
            })?;
            Ok(u32::try_from(list.lines().count()).unwrap_or(u32::MAX))
        })
        .await
    }
}

/// The folder's saved session in the page's form, or `None` when it has
/// none or it cannot be read (docs/04-browsing.md, "Restoring a session").
fn restored_session(data: &AppData, album: &Album, opened: &str) -> Option<serde_json::Value> {
    let read = data.session(opened).and_then(|stored| {
        stored
            .map(|s| album.db().read(|c| reopen::to_page(c, &s)))
            .transpose()
    });
    match read {
        Ok(session) => session,
        Err(e) => {
            tracing::warn!(target: "album", "the saved session could not be read: {}", e.message);
            None
        }
    }
}

/// Now, in milliseconds since the epoch.
pub(crate) fn now_ms() -> i64 {
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

command! {
    /// The missing rows in scope, grouped by folder, with what each would
    /// lose (docs/05-organising.md, "Tidy"). `scan_running` while a scan
    /// runs, `album_unreachable` when the album folder cannot be reached,
    /// `no_album` when no album is open.
    pub async fn album_tidy_list<R: Runtime>(; app: AppHandle<R>) -> Vec<TidyFolder> {
        let scanner = app.state::<Session>().queries()?.0;
        off_main(text::album::READING_DATABASE, move || {
            tidy::list(scanner.album(), scanner.is_running())
        })
        .await
    }
}

command! {
    /// Deletes the rows of `ids` that are still missing, in one
    /// transaction; a row whose file is found again is kept and un-marked
    /// (docs/05-organising.md, "Safety rules"). Never touches a photo file.
    /// Sends `album_changed` with reason `tidy` when anything was written,
    /// and starts a scan when a file was found again, so its facts are read
    /// afresh. `view_only`, `scan_running`, `album_unreachable`, `no_album`.
    pub async fn album_tidy_remove<R: Runtime>(ids: Vec<i64>; app: AppHandle<R>) -> TidyRemoved {
        let scanner = app.state::<Session>().queries()?.0;
        let worker = std::sync::Arc::clone(&scanner);
        let done = off_main(owlbum_core::text::album::tidy::REMOVING, move || {
            tidy::remove(worker.album(), ids, worker.is_running())
        })
        .await?;
        tracing::info!(target: "album", "tidy removed={} kept={}", done.removed, done.kept);
        if done.removed > 0 || done.kept > 0 {
            let _ = crate::events::emit(
                &app,
                &AlbumChanged {
                    revision: done.revision,
                    reason: ChangeReason::Tidy,
                    rekeyed: Vec::new(),
                },
            );
        }
        if done.kept > 0 {
            scanner.request(Request::default());
        }
        Ok(done)
    }
}
