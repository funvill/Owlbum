//! The program as a whole: its version, what `app_startup` hands the page,
//! and which folder a launch, a drop or the command line opens
//! (docs/08-desktop-and-release.md, "Starting"; docs/04-browsing.md,
//! "Opening a folder").

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Event;
use crate::appdata::{Recent, Settings};

pub mod assoc;
pub mod fatal;
pub mod update;
pub mod whats_new;

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
    /// The picture the launch named, when it named a file rather than a
    /// folder (Owlbum opening a picture from the file manager): it is
    /// selected and shown once its folder is open.
    pub launch_photo: Option<String>,
    /// The folder to reopen ([`reopen`]), when there is no launch folder.
    pub reopen: Option<String>,
    /// True while `updates.notice_seen` is false.
    pub show_update_notice: bool,
    /// The notes of the versions since `whats_new.last_seen`, newest
    /// first, to show now; empty shows nothing (docs/08, "What's new").
    pub whats_new: Vec<whats_new::WhatsNewEntry>,
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
    /// The picture to select and show once the folder is open: a second
    /// launch that named a file. Null otherwise.
    pub photo: Option<String>,
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

/// The project's page on GitHub. The About box links to it
/// (docs/08-desktop-and-release.md, "About box").
pub const REPOSITORY_URL: &str = "https://github.com/funvill/Owlbum";

/// Where feedback goes: the project's issues page (docs/08, "Feedback").
/// The owner asked for the plain issues page (issue 61), so nothing about
/// the computer is put in the address.
pub const FEEDBACK_URL: &str = "https://github.com/funvill/Owlbum/issues";

/// The `LICENSE` file, compiled in: the About box's Licence opens a copy
/// when the program's resources lack one.
pub const LICENCE: &str = include_str!("../../../LICENSE");

/// The copyright line of the `LICENSE` file, compiled in.
pub fn copyright() -> &'static str {
    LICENCE
        .lines()
        .find(|l| l.starts_with("Copyright"))
        .unwrap_or("")
}

/// The reply of `app_about`: the About box's facts
/// (docs/08-desktop-and-release.md, "About box"). The page holds the words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct About {
    pub version: String,
    /// `windows` or `linux`.
    pub platform: String,
    /// `x86_64`, for example.
    pub arch: String,
    /// `nsis`, `deb`, `appimage` or `dev`.
    pub package: String,
    /// The app data folder.
    pub data_dir: String,
    /// The copyright line of the licence.
    pub copyright: String,
    /// The project's address, shown as the link's text. The link itself is
    /// followed by `app_open_repository`: the page never opens an address.
    pub repository: String,
    /// Whether `THIRD-PARTY-NOTICES.txt` is among the program's resources;
    /// the About box shows its button only then.
    pub has_notices: bool,
}

/// The About box's facts for a data folder and package kind.
pub fn about(data_dir: &Path, package: &str, has_notices: bool) -> About {
    About {
        version: version().version,
        platform: std::env::consts::OS.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
        package: package.to_owned(),
        data_dir: data_dir.display().to_string(),
        copyright: copyright().to_owned(),
        repository: REPOSITORY_URL.to_owned(),
        has_notices,
    }
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

/// What a command line asks to open: a folder, and the picture in it when
/// it named a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub folder: PathBuf,
    /// The file named, when the argument was a file.
    pub photo: Option<PathBuf>,
}

/// What a command line opens (docs/08-desktop-and-release.md, "Command
/// line"): the first argument after the program's own name that does not
/// begin with `-`, resolved against `cwd`. A folder opens itself; a file
/// opens the folder that holds it, with the file as the photo to show.
pub fn launch<I, S>(args: I, cwd: &Path) -> Option<Launch>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let arg = args
        .into_iter()
        .skip(1)
        .find(|a| !a.as_ref().to_string_lossy().starts_with('-'))?;
    let path = cwd.join(arg.as_ref());
    let folder = folder_of(&path);
    let photo = (folder != path).then_some(path);
    Some(Launch { folder, photo })
}

/// The folder named on a command line: [`launch`]'s folder.
pub fn launch_folder<I, S>(args: I, cwd: &Path) -> Option<PathBuf>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    launch(args, cwd).map(|l| l.folder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_the_workspace_version() {
        assert_eq!(version().version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn about_names_the_version_the_repository_and_the_licence() {
        let a = about(Path::new("/data"), "dev", false);
        assert_eq!(a.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(a.repository, "https://github.com/funvill/Owlbum");
        assert!(a.copyright.starts_with("Copyright (c) "), "{}", a.copyright);
        assert_eq!(a.data_dir, Path::new("/data").display().to_string());
        assert_eq!(a.package, "dev");
        assert_eq!(FEEDBACK_URL, "https://github.com/funvill/Owlbum/issues");
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
        assert_eq!(
            launch_folder(["owlbum", "gone"], tmp.path()),
            Some(gone.clone())
        );
    }

    #[test]
    fn a_file_on_the_command_line_is_the_photo_to_show() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("Trip");
        std::fs::create_dir(&dir).unwrap();
        let file = dir.join("a.jpg");
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(
            launch(["owlbum", "Trip/a.jpg"], tmp.path()),
            Some(Launch {
                folder: dir.clone(),
                photo: Some(tmp.path().join("Trip/a.jpg")),
            })
        );
        assert_eq!(
            launch(["owlbum", file.to_str().unwrap()], Path::new("/elsewhere")),
            Some(Launch {
                folder: dir.clone(),
                photo: Some(file.clone()),
            }),
            "an absolute path, as the file manager gives it"
        );
        assert_eq!(
            launch(["owlbum", "Trip"], tmp.path()),
            Some(Launch {
                folder: dir,
                photo: None
            })
        );
        assert_eq!(launch(["owlbum"], tmp.path()), None);
    }
}
