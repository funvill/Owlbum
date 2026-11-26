//! Messages for the album area: opening a folder and its database.

/// `not_a_folder`: the path given to open is not a folder (or does not
/// exist).
pub fn not_a_folder(path: impl std::fmt::Display) -> String {
    format!("{path} is not a folder.")
}

/// `folder_unreadable`: the folder exists but cannot be read.
pub fn folder_unreadable(path: impl std::fmt::Display) -> String {
    format!("Owlbum can't read the folder {path}.")
}

/// What was being done, for an `internal` error while opening the album
/// database ("Something went wrong while ...").
pub const OPENING_DATABASE: &str = "opening the album database";

/// What was being done, for an `internal` error while creating the album
/// database.
pub const CREATING_DATABASE: &str = "creating the album database";

/// What was being done, for an `internal` error while updating the album
/// database to this version of Owlbum.
pub const UPGRADING_DATABASE: &str = "updating the album database";

/// What was being done, for an `internal` error while writing to the album
/// database.
pub const WRITING_DATABASE: &str = "saving to the album database";

/// What was being done, for an `internal` error while reading the album
/// database.
pub const READING_DATABASE: &str = "reading the album database";
