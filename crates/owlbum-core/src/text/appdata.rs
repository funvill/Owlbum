//! Sentences of the app data area: the app data folder, `app.sqlite`,
//! settings and the recent list (docs/02-data.md, "The app data folder").

/// What was being done, for an `internal` error while working out where the
/// app data folder is.
pub const FINDING_FOLDER: &str = "finding the folder for Owlbum's settings";

/// What was being done, for an `internal` error while creating the app data
/// folder.
pub const CREATING_FOLDER: &str = "creating the folder for Owlbum's settings";

/// What was being done, for an `internal` error while opening `app.sqlite`.
pub const OPENING_SETTINGS: &str = "opening Owlbum's settings";

/// What was being done, for an `internal` error while setting a damaged
/// `app.sqlite` aside.
pub const RESETTING_SETTINGS: &str = "resetting Owlbum's damaged settings";

/// What was being done, for an `internal` error while reading `app.sqlite`.
pub const READING_SETTINGS: &str = "reading Owlbum's settings";

/// What was being done, for an `internal` error while writing `app.sqlite`.
pub const SAVING_SETTINGS: &str = "saving Owlbum's settings";

/// Told once after a damaged `app.sqlite` was set aside and a new one made
/// (docs/08-desktop-and-release.md, "Startup sequence", step 4).
pub const SETTINGS_RESET: &str =
    "Owlbum's settings could not be read and were reset. Your photos and albums are not affected.";

/// `setting_out_of_range`: `app_settings_set` was given a value the setting
/// cannot hold.
pub fn setting_out_of_range(key: &str) -> String {
    format!("That value can't be used for the setting {key}.")
}

/// What was being done, for an `internal` error when the folder dialog
/// could not be shown.
pub const PICKING_FOLDER: &str = "showing the folder dialog";
