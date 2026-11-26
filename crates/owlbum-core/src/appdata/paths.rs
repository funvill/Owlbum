//! Where the app data folder is (docs/02-data.md, "The app data folder").
//!
//! | Platform | Installed build          | Development build            |
//! |----------|--------------------------|------------------------------|
//! | Windows  | `%LOCALAPPDATA%\OwlbumData` | `%LOCALAPPDATA%\OwlbumData Dev` |
//! | Linux    | `$XDG_DATA_HOME/owlbum`, else `~/.local/share/owlbum` | `.../owlbum-dev` |
//!
//! `OWLBUM_DATA_DIR` replaces the location and switches off single-instance
//! handling (tests use it). Owlbum works the path out itself and never uses
//! Tauri's identifier-named folders.
//!
//! [`locate`] reads the environment only through the closure it is given, so
//! tests never touch the real folder or the process environment.

use std::ffi::OsString;
use std::path::PathBuf;

use crate::AppError;
use crate::error::AppResult;
use crate::text;

/// The variable that replaces the app data folder.
pub const DATA_DIR_VAR: &str = "OWLBUM_DATA_DIR";

/// The system Owlbum runs on, as far as the folder rules care.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// `%LOCALAPPDATA%`.
    Windows,
    /// The XDG base directories.
    Linux,
}

impl Platform {
    /// The system this program was built for.
    pub const CURRENT: Platform = if cfg!(windows) {
        Platform::Windows
    } else {
        Platform::Linux
    };

    /// The app data folder's own name.
    pub fn folder_name(self, dev: bool) -> &'static str {
        match (self, dev) {
            (Platform::Windows, false) => "OwlbumData",
            (Platform::Windows, true) => "OwlbumData Dev",
            (Platform::Linux, false) => "owlbum",
            (Platform::Linux, true) => "owlbum-dev",
        }
    }
}

/// Where the app data folder is, and how that was decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    /// The app data folder. It may not exist yet.
    pub dir: PathBuf,
    /// True when `OWLBUM_DATA_DIR` chose it. Single-instance handling is
    /// then off, so tests can run several copies, each with its own folder.
    pub overridden: bool,
}

/// Works out the app data folder.
///
/// `dev` is true for a development build (one compiled with debug
/// assertions); it picks the `Dev` folder so a development build never
/// touches an installed copy's data. `env` reads one environment variable;
/// an empty value counts as unset.
///
/// # Errors
///
/// `internal` when the system gives no place for per-user data (no
/// `LOCALAPPDATA` on Windows; no `XDG_DATA_HOME` and no `HOME` on Linux).
pub fn locate(
    platform: Platform,
    dev: bool,
    env: impl Fn(&str) -> Option<OsString>,
) -> AppResult<Location> {
    let var = |name: &str| env(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(dir) = var(DATA_DIR_VAR) {
        return Ok(Location {
            dir,
            overridden: true,
        });
    }
    let base = match platform {
        Platform::Windows => var("LOCALAPPDATA").filter(|p| is_absolute(platform, p)),
        // The XDG spec says a relative XDG_DATA_HOME is invalid and is
        // ignored.
        Platform::Linux => var("XDG_DATA_HOME")
            .filter(|p| is_absolute(platform, p))
            .or_else(|| var("HOME").map(|home| home.join(".local").join("share"))),
    };
    let base = base.ok_or_else(|| AppError::internal(text::appdata::FINDING_FOLDER))?;
    Ok(Location {
        dir: base.join(platform.folder_name(dev)),
        overridden: false,
    })
}

/// Whether `p` is absolute on `platform`, whichever system the test runs on.
fn is_absolute(platform: Platform, p: &std::path::Path) -> bool {
    let s = p.to_string_lossy();
    match platform {
        Platform::Linux => s.starts_with('/'),
        Platform::Windows => {
            let b = s.as_bytes();
            (b.len() >= 3
                && b[0].is_ascii_alphabetic()
                && b[1] == b':'
                && matches!(b[2], b'\\' | b'/'))
                || s.starts_with(r"\\")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let map: HashMap<String, OsString> = vars
            .iter()
            .map(|(k, v)| ((*k).to_owned(), OsString::from(*v)))
            .collect();
        move |k| map.get(k).cloned()
    }

    #[test]
    fn windows_uses_local_app_data_owlbum_data() {
        let loc = locate(
            Platform::Windows,
            false,
            env(&[("LOCALAPPDATA", r"C:\Users\sam\AppData\Local")]),
        )
        .unwrap();
        assert_eq!(
            loc.dir,
            PathBuf::from(r"C:\Users\sam\AppData\Local").join("OwlbumData")
        );
        assert!(!loc.overridden);
    }

    #[test]
    fn a_windows_development_build_uses_its_own_folder() {
        let loc = locate(
            Platform::Windows,
            true,
            env(&[("LOCALAPPDATA", r"C:\Users\sam\AppData\Local")]),
        )
        .unwrap();
        assert_eq!(
            loc.dir,
            PathBuf::from(r"C:\Users\sam\AppData\Local").join("OwlbumData Dev")
        );
    }

    #[test]
    fn the_installed_and_development_folders_never_match() {
        for p in [Platform::Windows, Platform::Linux] {
            assert_ne!(p.folder_name(false), p.folder_name(true));
        }
        // Not the per-user install folder `%LOCALAPPDATA%\Owlbum`.
        assert_ne!(Platform::Windows.folder_name(false), "Owlbum");
        assert_ne!(Platform::Windows.folder_name(true), "Owlbum");
    }

    #[test]
    fn linux_uses_xdg_data_home() {
        let loc = locate(
            Platform::Linux,
            false,
            env(&[("XDG_DATA_HOME", "/data/sam"), ("HOME", "/home/sam")]),
        )
        .unwrap();
        assert_eq!(loc.dir, PathBuf::from("/data/sam").join("owlbum"));
    }

    #[test]
    fn linux_falls_back_to_local_share_in_home() {
        let loc = locate(Platform::Linux, false, env(&[("HOME", "/home/sam")])).unwrap();
        assert_eq!(
            loc.dir,
            PathBuf::from("/home/sam")
                .join(".local")
                .join("share")
                .join("owlbum")
        );
    }

    #[test]
    fn a_linux_development_build_uses_owlbum_dev() {
        let loc = locate(Platform::Linux, true, env(&[("HOME", "/home/sam")])).unwrap();
        assert_eq!(loc.dir.file_name().unwrap(), "owlbum-dev");
    }

    #[test]
    fn an_empty_or_relative_xdg_data_home_is_ignored() {
        for xdg in ["", "relative/data"] {
            let loc = locate(
                Platform::Linux,
                false,
                env(&[("XDG_DATA_HOME", xdg), ("HOME", "/home/sam")]),
            )
            .unwrap();
            assert!(loc.dir.starts_with("/home/sam"), "{xdg:?}: {:?}", loc.dir);
        }
    }

    #[test]
    fn owlbum_data_dir_wins_and_is_marked_overridden() {
        for (platform, dev) in [
            (Platform::Windows, false),
            (Platform::Windows, true),
            (Platform::Linux, false),
            (Platform::Linux, true),
        ] {
            let loc = locate(
                platform,
                dev,
                env(&[
                    ("OWLBUM_DATA_DIR", "/tmp/test-data"),
                    ("LOCALAPPDATA", r"C:\Local"),
                    ("HOME", "/home/sam"),
                ]),
            )
            .unwrap();
            assert_eq!(
                loc,
                Location {
                    dir: PathBuf::from("/tmp/test-data"),
                    overridden: true
                }
            );
        }
    }

    #[test]
    fn an_empty_owlbum_data_dir_counts_as_unset() {
        let loc = locate(
            Platform::Linux,
            false,
            env(&[("OWLBUM_DATA_DIR", ""), ("HOME", "/home/sam")]),
        )
        .unwrap();
        assert!(!loc.overridden);
    }

    #[test]
    fn no_place_for_user_data_is_an_internal_error() {
        let e = locate(Platform::Windows, false, env(&[])).unwrap_err();
        assert_eq!(e.code, crate::ErrorCode::Internal);
        assert!(locate(Platform::Linux, false, env(&[])).is_err());
        assert!(
            locate(
                Platform::Windows,
                false,
                env(&[("LOCALAPPDATA", "relative")])
            )
            .is_err()
        );
    }
}
