//! Messages and native dialog wording for file actions: the Recycle Bin or
//! Trash, show in folder, copying and dragging photos out
//! (docs/06-editing-and-file-actions.md, "File actions").

/// `photo_not_found` for a command on one photo.
pub const NOT_FOUND_ONE: &str = "That photo is not in the album any more.";

/// `photo_not_found` for a command on several photos.
pub const NOT_FOUND_MANY: &str = "Those photos are not in the album any more.";

/// `file_missing`: the photo's file is gone.
pub const FILE_MISSING: &str = "That photo's file is not there any more.";

/// `file_missing` for a drag: none of the files is left.
pub const DRAG_GONE: &str = "Couldn't drag the photos: the files are not there any more.";

/// `drag_failed`: the system refused the drag.
pub fn drag_failed(reason: &str) -> String {
    format!("Couldn't drag the photos: {reason}")
}

/// `trash_failed`: the command could not run at all.
pub fn trash_failed(reason: &str) -> String {
    format!("Couldn't delete: {reason}")
}

/// `internal`: show in folder failed.
pub fn reveal_failed(reason: &str) -> String {
    format!("Couldn't show the photo: {reason}")
}

/// `clipboard_failed` for copy path.
pub fn copy_path_failed(reason: &str) -> String {
    format!("Couldn't copy the path: {reason}")
}

/// `clipboard_failed` for copy image.
pub fn copy_image_failed(reason: &str) -> String {
    format!("Couldn't copy the photo: {reason}")
}

/// `clipboard_failed` for copy files.
pub fn copy_files_failed(reason: &str) -> String {
    format!("Couldn't copy the photos: {reason}")
}

/// `view_only`: the album's files cannot be changed.
pub const VIEW_ONLY: &str = "This album is view-only.";

/// `not_supported`: the platform does not offer this action.
pub const NOT_SUPPORTED: &str = "This is not supported on this platform yet.";

/// `internal`: the photo could not be decoded for copying.
pub const READ_FAILED: &str = "Couldn't read that photo.";

/// What was being done, for an `internal` error.
pub const TRASHING: &str = "moving photos to the bin";
/// What was being done, for an `internal` error.
pub const COPYING: &str = "copying photos";
/// What was being done, for an `internal` error.
pub const DRAGGING: &str = "dragging photos";
/// What was being done, for an `internal` error.
pub const REVEALING: &str = "showing the photo in its folder";

/// The bin's name on this platform.
pub const fn bin() -> &'static str {
    if cfg!(windows) {
        "Recycle Bin"
    } else {
        "Trash"
    }
}

/// A per-file failure: the drive has no bin, so the file is left where it
/// is rather than deleted for good.
pub fn no_bin() -> String {
    format!("This drive has no {}.", bin())
}

/// The trash confirm's message.
pub fn trash_message(count: usize, first_name: &str) -> String {
    if count == 1 {
        format!("Move \"{first_name}\" to the {}?", bin())
    } else {
        format!("Move {count} photos to the {}?", bin())
    }
}

/// The trash confirm's detail line.
pub fn trash_detail(count: usize) -> String {
    let them = if count == 1 { "it" } else { "them" };
    format!("You can restore {them} from the {}.", bin())
}

/// The trash confirm's accepting button.
pub fn trash_button() -> String {
    format!("Move to {}", bin())
}

/// The trash confirm's refusing button.
pub const CANCEL: &str = "Cancel";
