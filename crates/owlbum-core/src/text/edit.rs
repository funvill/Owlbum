//! Messages for the edit area: the edit model, rotate and flip, rendering
//! (docs/06-editing-and-file-actions.md).

/// `edit_invalid`: the edit sent by the page did not parse.
pub const INVALID: &str = "That edit could not be understood.";

/// `edit_not_editable`: a video.
pub const VIDEO: &str = "Videos cannot be edited";

/// `edit_not_editable`: an animated picture.
pub const ANIMATED: &str = "Animated pictures cannot be edited";

/// `view_only`: edits cannot be changed in a view-only album.
pub const VIEW_ONLY: &str = "This album is view-only.";

/// `edit_render_failed`: the picture is over the pixel limit.
pub const TOO_LARGE: &str = "This photo is too large to edit.";

/// `edit_render_failed`: a JPEG side is limited to 65,535 pixels.
pub const TOO_WIDE_FOR_JPEG: &str = "the picture is too large for a JPEG";

/// What was being done, for an `internal` error from `edit_auto`.
pub const AUTO: &str = "working out Auto for this photo";

/// What was being done, for an `internal` error while reading edits.
pub const READING: &str = "reading this photo's edit";

/// `edit_render_failed`: the edited photo could not be made.
pub fn render_failed(reason: impl std::fmt::Display) -> String {
    format!("Couldn't make the edited photo: {reason}")
}

// Saving (docs/06-editing-and-file-actions.md, "Saving").

use super::files::bin;

/// What was being done, for an `internal` error while saving.
pub const SAVING: &str = "saving the edited photo";

/// Save, step 1: the photo is gone from the album.
pub const SAVE_GONE: &str = "That photo is not in the album any more.";

/// Save, step 1: the photo has no edit.
pub fn save_none(name: &str) -> String {
    format!("{name} has no edit to save.")
}

/// Save, step 1 and the Save item's tooltip: a copy-only format.
pub fn copy_only(label: &str) -> String {
    format!("{label} photos can only be saved as a copy.")
}

/// Save, step 1: another save holds the photo.
pub const SAVE_BUSY: &str = "That photo is already being saved.";

/// Save, step 4: the save record could not be written.
pub fn save_not_recorded(name: &str) -> String {
    format!("Couldn't save {name}: the album could not be written.")
}

/// Save, step 5: the bin refused the original.
pub fn original_not_moved(reason: &str) -> String {
    format!(
        "Couldn't move the original to the {}, so nothing was changed: {reason}",
        bin()
    )
}

/// Save, step 6: the original is in the bin, the render could not be
/// moved into place.
pub fn not_placed(name: &str, reason: &str) -> String {
    let bin = bin();
    format!(
        "The original of {name} is in the {bin}, but the edited photo could not be put in its place: {reason}. Restore the original from the {bin} and try again."
    )
}

/// Save, step 7: the saved file could not be read again.
pub fn saved_not_read(reason: &str) -> String {
    format!("The photo was saved, but Owlbum could not read it again: {reason}")
}

/// Save, step 7: the edit changed while the save ran.
pub fn later_change(name: &str) -> String {
    format!("A later change to {name} was not saved")
}

/// Recovery: the original is in the bin and the render is gone.
pub fn interrupted(name: &str) -> String {
    format!(
        "{name} was being saved when Owlbum stopped. Its original is in the {}; restore it to get the photo back.",
        bin()
    )
}

/// The Save dialog's title.
pub const SAVE_AS_TITLE: &str = "Save the edited photo as";

/// The Save dialog's one filter.
pub fn save_as_filter(label: &str) -> String {
    format!("{label} photo")
}

/// Save as: forcing the extension named an existing file.
pub const SAVE_AS_EXISTS: &str = "A file with that name already exists. Choose another name.";

/// Save as: the original's own path.
pub const SAVE_AS_SAME: &str = "Choose a different name: Save as keeps the original.";

/// Save as: the file being replaced could not go to the bin.
pub fn replaced_not_moved(reason: &str) -> String {
    format!(
        "Couldn't move the existing file to the {}, so nothing was changed: {reason}",
        bin()
    )
}

/// Save as: the render or the move failed.
pub fn copy_failed(reason: &str) -> String {
    format!("Couldn't save the copy: {reason}")
}

/// Save as: the copy inside the album could not be read.
pub fn copy_not_read(file: &str, reason: &str) -> String {
    format!("The copy was saved as {file}, but Owlbum could not read it: {reason}")
}

/// Save as: the typed info could not be copied.
pub fn copy_info_failed(file: &str, reason: &str) -> String {
    format!(
        "The copy was saved as {file}, but its star, people, date, place, description and notes could not be copied to it: {reason}"
    )
}

/// The close question, one photo or several.
pub fn close_message(n: u64) -> String {
    if n == 1 {
        "1 photo has an edit that is not saved into its file.".to_owned()
    } else {
        format!("{n} photos have edits that are not saved into their files.")
    }
}

/// The close question's detail.
pub const CLOSE_DETAIL: &str = "The edits are kept in the album and will be here next time. Until they are saved, other programs see these photos without them.";

/// The close question's buttons.
pub const CLOSE_ANYWAY: &str = "Close anyway";
/// The close question's safe button.
pub const GO_BACK: &str = "Go back";
