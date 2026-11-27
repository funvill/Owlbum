//! The edit model, rendering and saving (`edit_`;
//! docs/06-editing-and-file-actions.md, "Commands and events").

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::commands::file::{Desktop, desktop};
use crate::commands::off_main;
use crate::session::Session;
use crate::{area, command};
use owlbum_core::album::{AlbumChanged, ChangeReason};
use owlbum_core::appdata::AppData;
use owlbum_core::appdata::settings::EditTarget;
use owlbum_core::edit::save::{self, EditStatus, SaveEnv, SaveProgress, SavedAs, SavedEdits};
use owlbum_core::edit::store::{self, EditSet, Reverted, Turned};
use owlbum_core::edit::{Colour, EditInput, EditPlan, PhotoEdit, TurnOp, auto, plan};
use owlbum_core::files;
use owlbum_core::scan::{Folders, Request, Scanner};
use owlbum_core::thumbs::{Thumbs, lookup};
use owlbum_core::{AppError, ErrorCode, text};
use tauri::{AppHandle, Manager, Runtime};

area! {
    prefix: "edit",
    commands: [
        edit_plan,
        edit_transform,
        edit_auto,
        edit_set,
        edit_turn,
        edit_revert,
        edit_status,
        edit_save_to_file,
        edit_save_all,
        edit_save_as,
    ],
    events: [SaveProgress],
}

/// `<app data>/renders`, where edited photos are made before they are
/// moved into place.
pub fn renders_dir<R: Runtime>(app: &AppHandle<R>) -> PathBuf {
    app.try_state::<AppData>()
        .map_or_else(std::env::temp_dir, |d| d.dir().to_path_buf())
        .join("renders")
}

/// The `edit_target` setting; `album` when it cannot be read.
pub fn edit_target<R: Runtime>(app: &AppHandle<R>) -> EditTarget {
    app.try_state::<AppData>()
        .and_then(|d| d.settings().ok())
        .map_or(EditTarget::Album, |s| s.edit_target)
}

/// The bin as saving uses it: refuses on a drive with no bin, never
/// deletes for good.
pub fn bin_of(desktop: Arc<dyn Desktop>) -> impl Fn(&Path) -> Result<(), String> + Sync {
    move |p: &Path| match files::no_bin_reason(p) {
        Some(reason) => Err(reason),
        None => desktop.trash(p),
    }
}

/// A request for a scan of the one folder `rel_dir`.
fn folder_scan(rel_dir: &str) -> Request {
    let mut folders = Folders::new();
    folders.add(rel_dir, false);
    Request {
        folders: Some(folders),
        ..Request::default()
    }
}

/// Sends `album_changed` with reason `saved` for the photos saved.
fn saved<R: Runtime>(app: &AppHandle<R>, out: &SavedEdits) {
    if out.saved.is_empty() {
        return;
    }
    let _ = crate::events::emit(
        app,
        &AlbumChanged {
            revision: out.revision,
            reason: ChangeReason::Saved,
            rekeyed: out.saved.clone(),
        },
    );
}

type SaveJob = dyn FnOnce(
        &owlbum_core::album::Album,
        &SaveEnv<'_>,
        &dyn Fn(SaveProgress),
    ) -> Result<SavedEdits, AppError>
    + Send;

/// Runs a save into files with progress sent to the page, then asks for
/// the scan that makes the saved files' analysis and thumbnails.
async fn run_save<R: Runtime>(
    app: AppHandle<R>,
    job: Box<SaveJob>,
) -> Result<SavedEdits, AppError> {
    let scanner: Arc<Scanner> = app.state::<Session>().queries()?.0;
    let desktop = desktop(&app)?;
    let renders = renders_dir(&app);
    let emit_to = app.clone();
    let out = off_main(text::edit::SAVING, move || {
        let bin = bin_of(desktop);
        let env = SaveEnv::new(renders, &bin);
        let progress = |p: SaveProgress| {
            let _ = crate::events::emit(&emit_to, &p);
        };
        let out = job(scanner.album(), &env, &progress)?;
        if !out.saved.is_empty() {
            scanner.request(Request::default());
        }
        Ok(out)
    })
    .await?;
    saved(&app, &out);
    Ok(out)
}

/// Sends `album_changed` with reason `edit` after a stored edit changed.
fn changed<R: Runtime>(app: &AppHandle<R>, revision: u64) {
    let _ = crate::events::emit(
        app,
        &AlbumChanged {
            revision,
            reason: ChangeReason::Edit,
            rekeyed: Vec::new(),
        },
    );
}

command! {
    /// The plan of a draft edit for photo `photo_id`. With `whole` the crop
    /// is ignored, so the plan shows the whole frame. Arithmetic only.
    /// Errors: `edit_invalid`, `photo_not_found`.
    pub async fn edit_plan<R: Runtime>(photo_id: i64, edit: EditInput, whole: bool; app: AppHandle<R>) -> EditPlan {
        let edit = edit.parse()?;
        let scanner = app.state::<Session>().queries()?.0;
        let (w, h) = off_main(text::edit::READING, move || {
            store::photo_size(scanner.album(), photo_id)
        })
        .await?;
        Ok(plan::edit_plan(w.max(1), h.max(1), &edit, whole))
    }
}

command! {
    /// `op` (`rotate_left`, `rotate_right` or `flip`) applied to `edit`.
    /// Pure; edit mode's buttons use it. Errors: `edit_invalid`.
    pub fn edit_transform(edit: EditInput, op: TurnOp) -> PhotoEdit {
        Ok(op.apply(&edit.parse()?))
    }
}

command! {
    /// Auto's light and colour for photo `photo_id`, from its medium
    /// thumbnail (made first when missing); saturation is always 0. Edit
    /// mode's Auto button keeps the rest of its draft. Errors:
    /// `photo_not_found`, `internal` when no thumbnail can be made.
    pub async fn edit_auto<R: Runtime>(photo_id: i64; app: AppHandle<R>) -> Colour {
        let scanner = app.state::<Session>().queries()?.0;
        let thumbs = app
            .try_state::<Thumbs>()
            .map(|t| t.inner().clone())
            .ok_or_else(|| AppError::internal(text::edit::AUTO))?;
        off_main(text::edit::AUTO, move || {
            let open = scanner.open();
            let photo = lookup::by_id(open.album, open.overlay, photo_id)?
                .ok_or_else(|| AppError::new(ErrorCode::PhotoNotFound, text::photo::NOT_FOUND))?;
            auto::of_photo(&thumbs, &photo)
        })
        .await
    }
}

command! {
    /// Stores `edit` as photo `photo_id`'s edit (null or identity stores
    /// none) and records one undo step labelled "edit". Nothing is written
    /// when it equals the stored edit. Sends `album_changed` with reason
    /// `edit` when it changed. Errors: `edit_invalid`, `edit_not_editable`,
    /// `photo_not_found`, `view_only`.
    pub async fn edit_set<R: Runtime>(photo_id: i64, edit: Option<EditInput>; app: AppHandle<R>) -> EditSet {
        let edit = edit.map(|e| e.parse()).transpose()?;
        let scanner = app.state::<Session>().queries()?.0;
        let set = off_main(text::album::WRITING_DATABASE, move || {
            store::set(scanner.album(), photo_id, edit)
        })
        .await?;
        if set.changed {
            changed(&app, set.revision);
        }
        Ok(set)
    }
}

command! {
    /// Turns or flips the stored edit of every photo in `photo_ids` in one
    /// transaction, as one undo step. Videos and animated pictures are
    /// skipped and counted. Sends `album_changed` with reason `edit` when
    /// any changed. Errors: `photo_not_found`, `view_only`.
    pub async fn edit_turn<R: Runtime>(photo_ids: Vec<i64>, op: TurnOp; app: AppHandle<R>) -> Turned {
        let scanner = app.state::<Session>().queries()?.0;
        let turned = off_main(text::album::WRITING_DATABASE, move || {
            store::turn(scanner.album(), photo_ids, op)
        })
        .await?;
        if !turned.changed.is_empty() {
            changed(&app, turned.revision);
        }
        Ok(turned)
    }
}

command! {
    /// Removes the stored edit of the photos `photo_ids`, in one
    /// transaction, as one undo step labelled "revert to original". Sends
    /// `album_changed` with reason `edit` when any changed. Errors:
    /// `photo_not_found`, `view_only`.
    pub async fn edit_revert<R: Runtime>(photo_ids: Vec<i64>; app: AppHandle<R>) -> Reverted {
        let scanner = app.state::<Session>().queries()?.0;
        let reverted = off_main(text::album::WRITING_DATABASE, move || {
            store::revert(scanner.album(), photo_ids)
        })
        .await?;
        if !reverted.changed.is_empty() {
            changed(&app, reverted.revision);
        }
        Ok(reverted)
    }
}

command! {
    /// `{ edited, waiting, saving }`, counted from the database: photos with
    /// a stored edit, those waiting to be saved into their files (`edited`
    /// when `edit_target` is `file`, else 0), and those being saved now.
    /// Errors: `no_album`.
    pub async fn edit_status<R: Runtime>(; app: AppHandle<R>) -> EditStatus {
        let scanner = app.state::<Session>().queries()?.0;
        let target = edit_target(&app);
        off_main(text::edit::READING, move || save::status(scanner.album(), target)).await
    }
}

command! {
    /// Saves the edits of `photo_ids` into their files, one after another
    /// (docs/06, "Save into the file: the sequence"); each original goes to
    /// the Recycle Bin or Trash. Per-photo failures are in the reply.
    /// Sends `edit_save_progress` when there is more than one photo, and
    /// `album_changed` with reason `saved`. Errors: `view_only`, `no_album`.
    pub async fn edit_save_to_file<R: Runtime>(photo_ids: Vec<i64>; app: AppHandle<R>) -> SavedEdits {
        run_save(app, Box::new(move |album, env, progress| save::save_to_file(album, env, &photo_ids, progress))).await
    }
}

command! {
    /// Saves every photo with an edit into its file, in path order, in
    /// `file` mode only. Copy-only photos are skipped and counted. Stops
    /// after the current photo when the album closes or the program quits.
    /// Errors: `view_only`, `no_album`, `not_supported` in `album` mode.
    pub async fn edit_save_all<R: Runtime>(; app: AppHandle<R>) -> SavedEdits {
        if edit_target(&app) != EditTarget::File {
            return Err(AppError::new(ErrorCode::NotSupported, text::edit::SAVE_ALL_ALBUM_MODE));
        }
        run_save(app, Box::new(|album, env, progress| save::save_all(album, env, progress))).await
    }
}

command! {
    /// Save as…: opens the native Save dialog for photo `photo_id` and
    /// writes an edited copy where the user chose; the original and its
    /// edit stay as they are. Offered in both modes and in a view-only
    /// album. Errors: `edit_none`, `edit_not_editable`, `save_as_same_file`,
    /// `save_as_exists`, `edit_render_failed`, `edit_original_not_moved`.
    pub async fn edit_save_as<R: Runtime>(photo_id: i64; app: AppHandle<R>) -> SavedAs {
        let scanner = app.state::<Session>().queries()?.0;
        let desktop = desktop(&app)?;
        let renders = renders_dir(&app);
        let out = off_main(text::edit::SAVING, move || {
            let album = scanner.album();
            let dialog = save::save_as_dialog(album, photo_id)?;
            let Some(chosen) = desktop.save_dialog(&dialog) else {
                return Ok(SavedAs { cancelled: true, revision: album.revision(), ..SavedAs::default() });
            };
            let bin = bin_of(Arc::clone(&desktop));
            let env = SaveEnv::new(renders, &bin);
            let rescan = |rel_dir: &str| {
                scanner.scan_and_wait(folder_scan(rel_dir));
            };
            save::save_as(album, &env, photo_id, &chosen, &rescan)
        })
        .await?;
        if out.photo_id.is_some() {
            let _ = crate::events::emit(
                &app,
                &AlbumChanged { revision: out.revision, reason: ChangeReason::Saved, rekeyed: Vec::new() },
            );
        }
        Ok(out)
    }
}
