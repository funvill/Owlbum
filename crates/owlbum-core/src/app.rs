//! The program as a whole: its version, what `app_startup` hands the page,
//! and which folder a launch, a drop or the command line opens
//! (docs/08-desktop-and-release.md, "Starting"; docs/04-browsing.md,
//! "Opening a folder").

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Event;
use crate::appdata::{Recent, Settings};

/// The program's version, for `app_version`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AppVersion {
    /// `major.minor.patch`, from `[workspace.package]` in the root
    /// `Cargo.toml`.
    pub version: String,
}

/// The program's version. Its one source is the workspace `Cargo.toml`.
pub fn version() -> AppVersion {
    AppVersion {
        version: env!("CARGO_PKG_VERSION").to_owned(),
    }
}

/// The reply of `app_startup`: everything the page needs to begin
/// (docs/08-desktop-and-release.md, "Commands and events this part adds").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Startup {
    pub version: String,
    /// `windows` or `linux`.
    pub platform: String,
    /// `x86_64`, for example.
    pub arch: String,
    /// How this copy was installed: `nsis`, `deb`, `appimage`, or `dev` for
    /// a development build.
    pub package: String,
    /// A development build.
    pub dev: bool,
    pub settings: Settings,
    /// The recent list, newest first, each checked to exist.
    pub recent: Vec<Recent>,
    /// A folder from the command line, or one that arrived before this
    /// call (a drop, a second launch). It is opened.
    pub launch_folder: Option<String>,
    /// The folder to reopen ([`reopen`]), when there is no launch folder.
    pub reopen: Option<String>,
    /// True while `updates.notice_seen` is false.
    pub show_update_notice: bool,
}

/// The folder to reopen at start (docs/08-desktop-and-release.md, "Which
/// album opens at start"): none when a folder was launched with, when
/// `reopen_last` is off, or when the newest recent folder was not found;
/// else the newest recent folder.
pub fn reopen(launch_folder: Option<&str>, reopen_last: bool, recent: &[Recent]) -> Option<String> {
    if launch_folder.is_some() || !reopen_last {
        return None;
    }
    recent.first().filter(|r| r.exists).map(|r| r.root.clone())
}

/// The package kind, for the start line, About and the updater.
pub fn package_kind(dev: bool, appimage: bool) -> &'static str {
    if dev {
        "dev"
    } else if cfg!(windows) {
        "nsis"
    } else if appimage {
        "appimage"
    } else {
        "deb"
    }
}

/// The `open_folder` event: a folder to open, from a drop, the File menu
/// or a second launch, arriving after `app_startup`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OpenFolder {
    pub path: String,
}

impl Event for OpenFolder {
    const NAME: &'static str = "open_folder";
}

/// What a drag over the window is doing (`folder_drop`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum DropPhase {
    /// Something is being dragged over the window: show the drop outline.
    Over,
    /// The drag left, or was dropped: hide it.
    Left,
    /// A drop held nothing that could be opened.
    Refused,
}

/// The `folder_drop` event. A drop that opens a folder sends `left` and
/// then `open_folder`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FolderDrop {
    pub phase: DropPhase,
}

impl Event for FolderDrop {
    const NAME: &'static str = "folder_drop";
}

/// The `app_closing` event (docs/08-desktop-and-release.md, "Closing and
/// quitting"): the window is about to close or the program to quit. The
/// page ends edit mode as Done, saves the session and answers with
/// `app_close_ready`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct AppClosing {}

impl Event for AppClosing {
    const NAME: &'static str = "app_closing";
}

/// A menu item that acts in the page (docs/08-desktop-and-release.md,
/// "Menus"). The page does the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum MenuItem {
    /// File > Close folder.
    CloseFolder,
    /// File > Rescan.
    Rescan,
    /// Help > Keyboard shortcuts.
    Shortcuts,
    /// Help's update item.
    Update,
    /// Help > What's new.
    WhatsNew,
    /// Help > About Owlbum.
    About,
}

/// The `app_menu` event: a menu item that acts in the page was chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AppMenu {
    pub item: MenuItem,
}

impl Event for AppMenu {
    const NAME: &'static str = "app_menu";
}

/// The folder a path opens: a folder opens itself, a file the folder that
/// holds it. A path that is neither (gone, unreadable) is kept, so that
/// opening it shows why it cannot be opened.
pub fn folder_of(path: &Path) -> PathBuf {
    match std::fs::metadata(path) {
        Ok(m) if m.is_file() => path
            .parent()
            .map_or_else(|| path.to_path_buf(), Path::to_path_buf),
        _ => path.to_path_buf(),
    }
}

/// The folder a drop of `paths` opens: the first folder among them, else
/// the folder that holds the first file; none when nothing can be opened.
pub fn dropped_folder(paths: &[PathBuf]) -> Option<PathBuf> {
    paths.iter().find(|p| p.is_dir()).cloned().or_else(|| {
        paths
            .iter()
            .find(|p| p.is_file())
            .and_then(|p| p.parent().map(Path::to_path_buf))
    })
}

/// The folder named on a command line (docs/08-desktop-and-release.md,
/// "Command line"): the first argument after the program's own name that
/// does not begin with `-`, resolved against `cwd`, a file's folder for a
/// file.
pub fn launch_folder<I, S>(args: I, cwd: &Path) -> Option<PathBuf>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let arg = args
        .into_iter()
        .skip(1)
        .find(|a| !a.as_ref().to_string_lossy().starts_with('-'))?;
    Some(folder_of(&cwd.join(arg.as_ref())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_the_workspace_version() {
        assert_eq!(version().version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn version_has_three_numeric_parts() {
        let v = version().version;
        let parts: Vec<&str> = v.split('.').collect();
        assert_eq!(parts.len(), 3, "{v}");
        assert!(parts.iter().all(|p| p.parse::<u32>().is_ok()), "{v}");
    }

    fn recent(root: &str, exists: bool) -> Recent {
        Recent {
            root: root.into(),
            name: root.into(),
            opened_at: 0,
            exists,
        }
    }

    #[test]
    fn reopen_is_the_newest_recent_folder_when_it_exists() {
        let list = [recent("/a", true), recent("/b", true)];
        assert_eq!(reopen(None, true, &list).as_deref(), Some("/a"));
        assert_eq!(reopen(None, false, &list), None, "reopen_last off");
        assert_eq!(
            reopen(Some("/c"), true, &list),
            None,
            "the launch folder wins"
        );
        assert_eq!(reopen(None, true, &[]), None);
        let gone = [recent("/a", false), recent("/b", true)];
        assert_eq!(reopen(None, true, &gone), None, "the newest is gone");
    }

    #[test]
    fn a_file_opens_its_folder_and_a_drop_opens_its_first_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("Trip");
        std::fs::create_dir(&dir).unwrap();
        let file = dir.join("a.jpg");
        std::fs::write(&file, b"x").unwrap();
        let other = tmp.path().join("Other");
        std::fs::create_dir(&other).unwrap();
        let gone = tmp.path().join("gone");
        assert_eq!(folder_of(&file), dir);
        assert_eq!(folder_of(&dir), dir);
        assert_eq!(folder_of(&gone), gone);
        assert_eq!(dropped_folder(&[file.clone(), other.clone()]), Some(other));
        assert_eq!(
            dropped_folder(std::slice::from_ref(&file)),
            Some(dir.clone())
        );
        assert_eq!(dropped_folder(std::slice::from_ref(&gone)), None);
        assert_eq!(dropped_folder(&[]), None);
        assert_eq!(
            launch_folder(["owlbum", "--flag", "Trip/a.jpg"], tmp.path()),
            Some(dir.clone())
        );
        assert_eq!(launch_folder(["owlbum", "-x"], tmp.path()), None);
        assert_eq!(launch_folder(["owlbum", "gone"], tmp.path()), Some(gone));
    }
}
