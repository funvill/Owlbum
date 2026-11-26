//! Messages for the album area: opening a folder and its database.

/// `not_a_folder`: the path given to open, or the folder a scan starts
/// from, is not a folder (or does not exist).
pub fn not_a_folder(path: impl std::fmt::Display) -> String {
    format!("Not a readable folder: {path}")
}

/// `folder_unreadable`: the folder exists but cannot be read. `err` is the
/// system's message.
pub fn folder_unreadable(path: impl std::fmt::Display, err: impl std::fmt::Display) -> String {
    format!("Can't read the folder {path}: {err}")
}

/// `no_album`: a command needs an open album and there is none.
pub const NO_ALBUM: &str = "No album is open.";

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

/// `album_lock_stale`: another computer's lock has an old heartbeat.
/// `last_seen` is already formatted for the user.
pub fn lock_stale(host: &str, last_seen: &str) -> String {
    format!("This album looks open on {host}, last seen {last_seen}.")
}

/// `album_damaged` when a backup can be restored. `date` is the backup's
/// date, `YYYY-MM-DD`.
pub fn damaged_with_backup(date: &str) -> String {
    format!(
        "This album's database is damaged. Owlbum can restore the backup from {date}. The damaged file is kept beside it."
    )
}

/// `album_damaged` when there is no backup to restore.
pub const DAMAGED_NO_BACKUP: &str = "This album's database is damaged and there is no backup. You can still look through the photos.";

/// The message of a `view_only` error: the first sentence of the view-only
/// banner in force (docs/04-browsing.md, "Album banners").
pub mod view_only {
    /// `read_only`.
    pub const READ_ONLY: &str = "View only: this folder can't be written to.";
    /// `read_only` with no database on disk (`in_memory`).
    pub const IN_MEMORY: &str = "View only: this folder can't be written to, so nothing Owlbum learns about these photos is kept.";
    /// `newer`.
    pub const NEWER: &str = "This album was last used by a newer Owlbum.";
    /// `damaged`.
    pub const DAMAGED: &str = "View only: this album's database is damaged.";

    /// `locked`: another computer has the album open.
    pub fn locked(host: &str) -> String {
        format!("View only: this album is open on {host}.")
    }

    /// `chosen`: the user picked Open view-only. `host` is the computer
    /// named by the old lock, when there was one.
    pub fn chosen(host: Option<&str>) -> String {
        match host {
            Some(host) => format!("View only: this album may be open on {host}."),
            None => "View only: you opened this album view-only.".to_owned(),
        }
    }
}

/// What was being done, for an `internal` error while taking or writing the
/// album's lock file.
pub const LOCKING: &str = "marking the album as open";

/// What was being done, for an `internal` error while checking the album
/// database for damage.
pub const CHECKING_DATABASE: &str = "checking the album database";

/// What was being done, for an `internal` error while restoring the album
/// database from its backup.
pub const RESTORING_BACKUP: &str = "restoring the album database from its backup";

/// What was being done, for an `internal` error while making the daily
/// backup.
pub const BACKING_UP: &str = "backing up the album database";
