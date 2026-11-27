//! `AppError`: the one error type every command returns.
//!
//! It serialises as `{ "code": "<stable_code>", "message": "<sentence for the
//! user>", "details"?: { ... } }` (docs/01-architecture.md, "Errors"). The page
//! shows `message` and branches only on `code`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A stable error code. The page branches on these.
///
/// Adding a code is a code change here, never a string typed at a call site.
/// Keep the variants in alphabetical order, one per line, so that parallel
/// additions rarely touch the same lines. Codes used by one part only are
/// listed in that part's document; codes used by several parts are listed in
/// docs/01-architecture.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// `album_open`: the album database failed its damage check
    /// (docs/04-browsing.md, "Opening a folder").
    AlbumDamaged,
    /// `album_open`: another computer's lock on the album has an old
    /// heartbeat; the user chooses what to do.
    AlbumLockStale,
    /// The album folder cannot be reached (an unplugged drive).
    AlbumUnreachable,
    /// The query could not be understood.
    BadQuery,
    /// The clipboard could not be written.
    ClipboardFailed,
    /// The drag could not start.
    DragFailed,
    /// `edit_plan`, `edit_set`, `edit_transform`: the edit sent by the page
    /// did not parse (docs/06-editing-and-file-actions.md).
    EditInvalid,
    /// `edit_set`: the photo is a video or an animated picture.
    EditNotEditable,
    /// The edited photo could not be made.
    EditRenderFailed,
    /// Save into the file: the format can only be saved as a copy.
    EditCopyOnly,
    /// Save into the file: the photo has no edit to save.
    EditNone,
    /// Save into the file: the photo is already being saved.
    EditBusy,
    /// Save into the file: the original could not go to the bin; nothing
    /// was changed.
    EditOriginalNotMoved,
    /// Save into the file: the original is in the bin and the edited photo
    /// could not be put in its place.
    EditNotPlaced,
    /// Save into the file: the file was saved and could not be read again.
    EditSavedNotRead,
    /// A photo's file is not there.
    FileMissing,
    /// The path given to `album_open` cannot be read.
    FolderUnreadable,
    /// Something failed that the user cannot act on; the message says what.
    Internal,
    /// `photo_set_info`: a typed date that is not a real date and time.
    InvalidDate,
    /// `photo_set_info`: a typed place out of range.
    InvalidLocation,
    /// No album is open.
    NoAlbum,
    /// The platform does not offer this action.
    NotSupported,
    /// The path given to `album_open` is not a folder.
    NotAFolder,
    /// The id is not a photo of the open album.
    PhotoNotFound,
    /// The token was dropped; run the query again.
    QueryExpired,
    /// Save as: the chosen name, after its extension was forced, names a
    /// file that already exists.
    SaveAsExists,
    /// Save as: the chosen file is the original.
    SaveAsSameFile,
    /// The action must wait for the scan.
    ScanRunning,
    /// `app_settings_set` was given a value outside the setting's range.
    SettingOutOfRange,
    /// Tagging (docs/07-tagging.md, "Commands and events").
    NoSuchModel,
    DownloadBusy,
    DownloadFailed,
    ModelNotReady,
    NoDescribeModel,
    NotDescribedYet,
    ModelLoadFailed,
    EmptyDescription,
    TaggingTimeout,
    TaggingLost,
    TaggingStopped,
    /// `file_trash` could not run at all; per-file failures are in its
    /// reply.
    TrashFailed,
    /// The album cannot be changed right now.
    ViewOnly,
}

/// The error every command returns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, thiserror::Error)]
#[error("{message}")]
pub struct AppError {
    /// What went wrong, for the page to branch on.
    pub code: ErrorCode,
    /// A sentence for the user. Written in the `text` module, never inline.
    pub message: String,
    /// Structured facts a dialog needs, for example the other computer's name
    /// for a stale lock.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub details: Option<serde_json::Value>,
}

impl AppError {
    /// An error with a code and a message for the user.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    /// Adds structured facts for the page.
    #[must_use]
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    /// An `internal` error. `what` names what failed, for the log and the
    /// message.
    pub fn internal(what: impl std::fmt::Display) -> Self {
        Self::new(ErrorCode::Internal, crate::text::errors::internal(what))
    }
}

/// The result type of every fallible core function and command.
pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serialises_code_and_message_without_details() {
        let e = AppError::new(ErrorCode::NoAlbum, "No album is open.");
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            json!({ "code": "no_album", "message": "No album is open." })
        );
    }

    #[test]
    fn serialises_details_when_present() {
        let e = AppError::new(ErrorCode::ViewOnly, "Locked.")
            .with_details(json!({ "computer": "LAPTOP", "seen": 1 }));
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            json!({
                "code": "view_only",
                "message": "Locked.",
                "details": { "computer": "LAPTOP", "seen": 1 }
            })
        );
    }

    #[test]
    fn shared_codes_have_their_documented_names() {
        let names: Vec<String> = [
            ErrorCode::AlbumUnreachable,
            ErrorCode::BadQuery,
            ErrorCode::FolderUnreadable,
            ErrorCode::Internal,
            ErrorCode::NoAlbum,
            ErrorCode::NotAFolder,
            ErrorCode::PhotoNotFound,
            ErrorCode::QueryExpired,
            ErrorCode::ScanRunning,
            ErrorCode::SettingOutOfRange,
            ErrorCode::ViewOnly,
        ]
        .iter()
        .map(|c| {
            serde_json::to_value(c)
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
        assert_eq!(
            names,
            [
                "album_unreachable",
                "bad_query",
                "folder_unreadable",
                "internal",
                "no_album",
                "not_a_folder",
                "photo_not_found",
                "query_expired",
                "scan_running",
                "setting_out_of_range",
                "view_only",
            ]
        );
    }

    #[test]
    fn round_trips_through_json() {
        let e = AppError::new(ErrorCode::PhotoNotFound, "Gone.").with_details(json!({ "id": 7 }));
        let back: AppError = serde_json::from_str(&serde_json::to_string(&e).unwrap()).unwrap();
        assert_eq!(back, e);
    }

    #[test]
    fn internal_errors_say_what_failed() {
        let e = AppError::internal("reading the clock");
        assert_eq!(e.code, ErrorCode::Internal);
        assert!(e.message.contains("reading the clock"), "{}", e.message);
    }
}
