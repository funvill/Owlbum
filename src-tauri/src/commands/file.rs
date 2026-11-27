//! File actions: trash, reveal, copy, drag (`file_`), docs/06-editing-and-
//! file-actions.md, "File actions" and "Dragging photos out".
//!
//! Every command takes photo ids; Rust resolves them to paths, shows the
//! confirm, writes the clipboard and starts the drag, so the page never
//! names a path. The platform calls sit behind [`Desktop`], which `run`
//! fills with the real ones (`crate::desktop`) and shell tests with fakes.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use crate::commands::off_main;
use crate::session::Session;
use crate::{area, command};
use owlbum_core::album::{AlbumChanged, ChangeReason};
use owlbum_core::files::{
    self, Capabilities, CopiedFiles, CopiedImage, CopiedPaths, CopyAs, DragCache, Dragged, Target,
    Trashed,
};
use owlbum_core::media::MediaKind;
use owlbum_core::scan::Scanner;
use owlbum_core::thumbs::{Kind, Thumbs};
use owlbum_core::{AppError, ErrorCode, text};
use tauri::{AppHandle, Manager, Runtime};

area! {
    prefix: "file",
    commands: [
        file_copy_files,
        file_copy_image,
        file_copy_paths,
        file_copy_text,
        file_prepare_drag,
        file_reveal,
        file_start_drag,
        file_trash,
    ],
    events: [],
}

/// The platform calls of file actions. Each answers a reason for the user
/// when it fails.
pub trait Desktop: Send + Sync {
    /// The native warning box before moving to the bin, modal to the
    /// window. True when the user chose to move.
    fn confirm_trash(&self, message: &str, detail: &str) -> bool;
    /// Moves one file to the Recycle Bin or Trash; never deletes it.
    fn trash(&self, path: &Path) -> Result<(), String>;
    /// Opens the file manager with the file selected.
    fn reveal(&self, path: &Path) -> Result<(), String>;
    /// Puts text on the clipboard.
    fn copy_text(&self, text: &str) -> Result<(), String>;
    /// Puts a picture (8-bit RGBA) on the clipboard.
    fn copy_image(&self, width: u32, height: u32, rgba: Vec<u8>) -> Result<(), String>;
    /// Puts a list of files on the clipboard. Only called where
    /// [`Capabilities::current`] says `files`.
    fn copy_files(&self, paths: &[PathBuf]) -> Result<(), String>;
    /// Starts dragging `paths` out with `icon` (PNG bytes), offering copy
    /// only, never move. Only called where the platform offers drag-out.
    fn start_drag(&self, paths: Vec<PathBuf>, icon: Vec<u8>) -> Result<(), String>;
    /// The native Save dialog of Save as… (docs/06, "Save as…"), modal to
    /// the window. It asks before replacing a file. None when cancelled.
    fn save_dialog(&self, _dialog: &owlbum_core::edit::save::SaveAsDialog) -> Option<PathBuf> {
        None
    }
    /// The close question in `file` mode (docs/06, "Closing and
    /// quitting"). True for "Close anyway".
    fn confirm_close(&self, _message: &str, _detail: &str) -> bool {
        true
    }
}

/// The app's [`Desktop`], managed as state.
#[derive(Clone)]
pub struct DesktopShell(pub Arc<dyn Desktop>);

/// A drag prepared on pointer-down: the files and the icon.
#[derive(Clone)]
pub struct PreparedDrag {
    paths: Vec<PathBuf>,
    with_edits: bool,
    icon: Vec<u8>,
}

/// The one prepared drag, managed as state by `setup`.
#[derive(Default)]
pub struct DragPrep(pub DragCache<PreparedDrag>);

pub(crate) fn desktop<R: Runtime>(app: &AppHandle<R>) -> Result<Arc<dyn Desktop>, AppError> {
    app.try_state::<DesktopShell>()
        .map(|d| Arc::clone(&d.0))
        .ok_or_else(|| AppError::internal(text::files::COPYING))
}

fn scanner<R: Runtime>(app: &AppHandle<R>) -> Result<Arc<Scanner>, AppError> {
    Ok(app.state::<Session>().queries()?.0)
}

fn exists(p: &Path) -> bool {
    p.exists()
}

fn resolve(scanner: &Scanner, ids: &[i64], limit: usize) -> Result<files::Targets, AppError> {
    let open = scanner.open();
    files::resolve(open.album, open.overlay, ids, limit, &exists)
}

command! {
    /// Moves the photos `photo_ids` to the Recycle Bin or Trash after the
    /// native confirm (docs/06, "Move to the Recycle Bin or Trash"). Only
    /// photos whose files exist are offered; none left answers an empty
    /// reply and shows nothing. Moved photos' rows are marked missing and
    /// `album_changed` is sent with reason `trashed`. Errors: `view_only`,
    /// `photo_not_found`, `no_album`, `trash_failed`.
    pub async fn file_trash<R: Runtime>(photo_ids: Vec<i64>; app: AppHandle<R>) -> Trashed {
        let scanner = scanner(&app)?;
        let desktop = desktop(&app)?;
        let out = off_main(text::files::TRASHING, move || {
            files::check_writable(scanner.album())?;
            let targets = resolve(&scanner, &photo_ids, usize::MAX)?;
            let album = scanner.album();
            let Some(first) = targets.photos.first() else {
                return Ok(Trashed { revision: album.revision(), ..Trashed::default() });
            };
            let n = targets.photos.len();
            let asked = desktop.confirm_trash(
                &text::files::trash_message(n, &first.name),
                &text::files::trash_detail(n),
            );
            if !asked {
                return Ok(Trashed { cancelled: true, revision: album.revision(), ..Trashed::default() });
            }
            let bin = |p: &Path| match files::no_bin_reason(p) {
                Some(reason) => Err(reason),
                None => desktop.trash(p),
            };
            files::trash(album, &targets.photos, &bin)
        })
        .await
        .map_err(|e| match e.code {
            ErrorCode::Internal => AppError::new(ErrorCode::TrashFailed, text::files::trash_failed(&e.message)),
            _ => e,
        })?;
        if !out.trashed.is_empty() {
            let _ = crate::events::emit(
                &app,
                &AlbumChanged {
                    revision: out.revision,
                    reason: ChangeReason::Trashed,
                    rekeyed: Vec::new(),
                },
            );
        }
        Ok(out)
    }
}

command! {
    /// Opens the file manager with the photo's file selected (docs/06,
    /// "Show in folder"). Errors: `photo_not_found`, `file_missing`,
    /// `internal`.
    pub async fn file_reveal<R: Runtime>(photo_id: i64; app: AppHandle<R>) -> () {
        let scanner = scanner(&app)?;
        let desktop = desktop(&app)?;
        off_main(text::files::REVEALING, move || {
            let open = scanner.open();
            let target = files::resolve_one(open.album, open.overlay, photo_id, &exists)?;
            desktop
                .reveal(&target.photo.path)
                .map_err(|r| AppError::new(ErrorCode::Internal, text::files::reveal_failed(&r)))
        })
        .await
    }
}

command! {
    /// Puts the absolute paths of the photos on the clipboard as text, one
    /// per line, in the order sent, at most 500 (docs/06, "Copy path").
    /// Errors: `photo_not_found`, `clipboard_failed`.
    pub async fn file_copy_paths<R: Runtime>(photo_ids: Vec<i64>; app: AppHandle<R>) -> CopiedPaths {
        let scanner = scanner(&app)?;
        let desktop = desktop(&app)?;
        off_main(text::files::COPYING, move || {
            let open = scanner.open();
            // A path is copied whether or not its file is there.
            let targets = files::resolve(open.album, open.overlay, &photo_ids, files::LIMIT, &|_| true)?;
            desktop
                .copy_text(&files::path_lines(&targets.photos))
                .map_err(|r| AppError::new(ErrorCode::ClipboardFailed, text::files::copy_path_failed(&r)))?;
            Ok(CopiedPaths { copied: targets.photos.len(), dropped: targets.dropped })
        })
        .await
    }
}

/// The most text `file_copy_text` takes, in bytes.
const MAX_COPY_TEXT: usize = 1 << 20;

command! {
    /// Puts `content` on the clipboard as plain text: the details panel's
    /// Copy button (docs/04-browsing.md, "The details panel"). At most
    /// 1 MiB. Errors: `internal` for more, `clipboard_failed`.
    pub async fn file_copy_text<R: Runtime>(content: String; app: AppHandle<R>) -> () {
        if content.len() > MAX_COPY_TEXT {
            return Err(AppError::internal(text::files::COPYING_TEXT));
        }
        let desktop = desktop(&app)?;
        off_main(text::files::COPYING_TEXT, move || {
            desktop
                .copy_text(&content)
                .map_err(|r| AppError::new(ErrorCode::ClipboardFailed, text::files::copy_text_failed(&r)))
        })
        .await
    }
}

command! {
    /// Puts the picture of one photo on the clipboard, upright, at most 50
    /// megapixels (docs/06, "Copy image (one photo)"). A video's file is
    /// copied instead. The stored edit is not drawn yet: `with_edits` says
    /// when the photo has one. Errors: `photo_not_found`, `file_missing`,
    /// `internal` (cannot be read), `clipboard_failed`.
    pub async fn file_copy_image<R: Runtime>(photo_id: i64; app: AppHandle<R>) -> CopiedImage {
        let scanner = scanner(&app)?;
        let desktop = desktop(&app)?;
        off_main(text::files::COPYING, move || {
            let open = scanner.open();
            let target = files::resolve_one(open.album, open.overlay, photo_id, &exists)?;
            let failed = |r: String| AppError::new(ErrorCode::ClipboardFailed, text::files::copy_image_failed(&r));
            if target.photo.kind == MediaKind::Video {
                copy_files_to(&*desktop, std::slice::from_ref(&target)).map_err(failed)?;
                return Ok(CopiedImage { scaled: false, file: true, with_edits: false });
            }
            let picture = files::picture(&target)?;
            desktop
                .copy_image(picture.width, picture.height, picture.rgba)
                .map_err(failed)?;
            Ok(CopiedImage { scaled: picture.scaled, file: false, with_edits: target.edited })
        })
        .await
    }
}

/// Puts the files of `targets` on the clipboard where the platform can,
/// else their paths as text. Answers which.
fn copy_files_to(desktop: &dyn Desktop, targets: &[Target]) -> Result<CopyAs, String> {
    match Capabilities::current().copy_files {
        CopyAs::Files => {
            let paths: Vec<PathBuf> = targets.iter().map(|t| t.photo.path.clone()).collect();
            desktop.copy_files(&paths).map(|()| CopyAs::Files)
        }
        CopyAs::Paths => desktop
            .copy_text(&files::path_lines(targets))
            .map(|()| CopyAs::Paths),
    }
}

command! {
    /// Puts the original files of the photos on the clipboard, at most 500
    /// whose files exist, in the order sent (docs/06, "Copy several
    /// photos"). Where the platform cannot, their paths go as text and
    /// `as` is `paths`. Errors: `photo_not_found`, `clipboard_failed`.
    pub async fn file_copy_files<R: Runtime>(photo_ids: Vec<i64>; app: AppHandle<R>) -> CopiedFiles {
        let scanner = scanner(&app)?;
        let desktop = desktop(&app)?;
        off_main(text::files::COPYING, move || {
            let targets = resolve(&scanner, &photo_ids, files::LIMIT)?;
            if targets.photos.is_empty() {
                return Err(AppError::new(ErrorCode::PhotoNotFound, text::files::NOT_FOUND_MANY));
            }
            let copied_as = copy_files_to(&*desktop, &targets.photos)
                .map_err(|r| AppError::new(ErrorCode::ClipboardFailed, text::files::copy_files_failed(&r)))?;
            Ok(CopiedFiles {
                copied: targets.photos.len(),
                dropped: targets.dropped,
                copied_as,
                with_edits: targets.with_edits(),
            })
        })
        .await
    }
}

/// Resolves the photos of a drag and makes its icon from the first one's
/// small thumbnail.
fn prepare<R: Runtime>(
    app: &AppHandle<R>,
    scanner: &Scanner,
    ids: &[i64],
) -> Result<PreparedDrag, AppError> {
    let targets = resolve(scanner, ids, files::LIMIT)?;
    let small = match (targets.photos.first(), app.try_state::<Thumbs>()) {
        (Some(first), Some(thumbs)) => thumbs.thumbnail(&first.photo, Kind::Small),
        _ => None,
    };
    Ok(PreparedDrag {
        paths: targets
            .photos
            .iter()
            .map(|t| t.photo.path.clone())
            .collect(),
        with_edits: targets.with_edits(),
        icon: files::drag_icon(small.as_deref()),
    })
}

command! {
    /// Called on pointer-down on a tile or the preview: resolves the
    /// photos and loads the drag icon, kept for exactly these ids for 10
    /// seconds (docs/06, "Starting the drag"). Never fails; a failure shows
    /// when the drag starts.
    pub async fn file_prepare_drag<R: Runtime>(photo_ids: Vec<i64>; app: AppHandle<R>) -> () {
        if !Capabilities::current().drag_out {
            return Ok(());
        }
        let Ok(scanner) = scanner(&app) else {
            return Ok(());
        };
        let _ = off_main(text::files::DRAGGING, move || {
            if let Ok(prepared) = prepare(&app, &scanner, &photo_ids)
                && let Some(prep) = app.try_state::<DragPrep>()
            {
                prep.0.put(photo_ids, Instant::now(), prepared);
            }
            Ok(())
        })
        .await;
        Ok(())
    }
}

command! {
    /// Starts dragging the photos' original files out of the window, using
    /// the preparation for the same ids when it is fresh (docs/06,
    /// "Dragging photos out"). Errors: `not_supported` where the platform
    /// does not offer it, `photo_not_found`, `file_missing` when no file is
    /// left, `drag_failed`.
    pub async fn file_start_drag<R: Runtime>(photo_ids: Vec<i64>; app: AppHandle<R>) -> Dragged {
        if !Capabilities::current().drag_out {
            return Err(AppError::new(ErrorCode::NotSupported, text::files::NOT_SUPPORTED));
        }
        let scanner = scanner(&app)?;
        let desktop = desktop(&app)?;
        off_main(text::files::DRAGGING, move || {
            let held = app
                .try_state::<DragPrep>()
                .and_then(|p| p.0.take(&photo_ids, Instant::now()));
            let mut prepared = match held {
                Some(p) => p,
                None => prepare(&app, &scanner, &photo_ids)?,
            };
            // Checked once more just before the drag starts.
            prepared.paths.retain(|p| p.exists());
            if prepared.paths.is_empty() {
                return Err(AppError::new(ErrorCode::FileMissing, text::files::DRAG_GONE));
            }
            let dragged = prepared.paths.len();
            desktop
                .start_drag(prepared.paths, prepared.icon)
                .map_err(|r| AppError::new(ErrorCode::DragFailed, text::files::drag_failed(&r)))?;
            Ok(Dragged { dragged, with_edits: prepared.with_edits })
        })
        .await
    }
}
