//! The edit model, rendering and saving (`edit_`;
//! docs/06-editing-and-file-actions.md, "Commands and events").

use crate::commands::off_main;
use crate::session::Session;
use crate::{area, command};
use owlbum_core::album::{AlbumChanged, ChangeReason};
use owlbum_core::edit::store::{self, EditSet, Reverted, Turned};
use owlbum_core::edit::{Colour, EditInput, EditPlan, PhotoEdit, TurnOp, auto, plan};
use owlbum_core::thumbs::{Thumbs, lookup};
use owlbum_core::{AppError, ErrorCode, text};
use tauri::{AppHandle, Manager, Runtime};

area! {
    prefix: "edit",
    commands: [edit_plan, edit_transform, edit_auto, edit_set, edit_turn, edit_revert],
    events: [],
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
