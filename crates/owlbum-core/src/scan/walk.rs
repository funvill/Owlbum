//! The walker (docs/03-media-and-scanning.md, "The walker").
//!
//! It lists the files of the scope (the album root, or the scoped folder),
//! always with subfolders, reading directory entries without following
//! links. What it leaves out is decided per entry by [`classify`]; roadmap
//! #9 adds the rules for system folders, nested album roots, counted links
//! and online-only files there, and the scan report commands over
//! [`Walk`].

use std::fs::{FileType, Metadata};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::order::natural_cmp;
use crate::error::AppResult;
use crate::media::{self, UnsupportedFormat};
use crate::text;
use crate::{AppError, ErrorCode};

/// A supported file the walk found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    /// Relative to the album root, `/` separators, as the system reports
    /// it.
    pub rel_path: String,
    /// Size in bytes, from the directory entry.
    pub size: u64,
    /// Modified time in ms since the epoch, rounded to the nearest ms.
    pub mtime_ms: i64,
}

/// A file Owlbum recognises but cannot open yet. It gets no row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UnsupportedFile {
    /// Relative to the album root.
    pub rel_path: String,
    /// The format's label, such as `HEIC`.
    pub label: String,
    /// The sentence shown beside the file.
    pub reason: String,
}

/// A subfolder the walk could not read. Its rows are never marked missing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UnreadableFolder {
    /// Relative to the album root.
    pub rel_path: String,
    /// The system's message.
    pub message: String,
}

/// What a walk found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Walk {
    /// Supported files, in walker order ([`natural_cmp`]).
    pub files: Vec<Listed>,
    /// Recognised but unsupported files, in walker order.
    pub unsupported: Vec<UnsupportedFile>,
    /// Subfolders that could not be read, in the order met.
    pub unreadable: Vec<UnreadableFolder>,
    /// Names that are not valid Unicode, skipped.
    pub unreadable_names: u32,
    /// False when the walk was cancelled before it ended.
    pub complete: bool,
}

impl Walk {
    /// True when `rel_path` lies in a subfolder the walk could not read.
    pub fn is_under_unreadable(&self, rel_path: &str) -> bool {
        self.unreadable
            .iter()
            .any(|f| is_under(rel_path, &f.rel_path))
    }
}

/// True when `rel_path` is `folder` itself or lies under it. `folder` `""`
/// is the album root, which holds everything. Compares whole components,
/// so `2024-old/a.jpg` is not under `2024`.
pub fn is_under(rel_path: &str, folder: &str) -> bool {
    folder.is_empty()
        || rel_path == folder
        || (rel_path.starts_with(folder) && rel_path.as_bytes().get(folder.len()) == Some(&b'/'))
}

/// What the walker does with one directory entry.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Rule {
    /// Left out silently.
    Skip,
    /// A folder to walk into.
    Descend,
    /// A supported file to list.
    List,
    /// A recognised but unsupported file, counted.
    Unsupported(&'static UnsupportedFormat),
}

/// Decides what to do with an entry, by its name and its type as the
/// directory reports it (links are not followed). Roadmap #9 extends it.
pub(crate) fn classify(name: &str, kind: FileType) -> Rule {
    // `.owlbum`, `.git`, `.Trash-1000`, dotfiles, and `._` AppleDouble
    // files (which also start with a dot; `is_supported` refuses them on
    // its own too, should this rule ever change).
    if name.starts_with('.') {
        return Rule::Skip;
    }
    // Links are never followed: they can loop, leave the album, or list one
    // file twice. Counting them for the scan report is #9.
    if kind.is_symlink() {
        return Rule::Skip;
    }
    if kind.is_dir() {
        return Rule::Descend;
    }
    if !kind.is_file() {
        return Rule::Skip;
    }
    if media::is_supported(name) {
        return Rule::List;
    }
    if let Some(u) = media::unsupported_of(name) {
        return Rule::Unsupported(u);
    }
    Rule::Skip
}

/// The modified time of `meta` in ms since the epoch, rounded to the
/// nearest millisecond. 0 when the system cannot tell.
pub fn mtime_ms(meta: &Metadata) -> i64 {
    meta.modified().map_or(0, system_time_ms)
}

fn system_time_ms(t: SystemTime) -> i64 {
    let round = |d: std::time::Duration| {
        i64::try_from((d.as_nanos() + 500_000) / 1_000_000).unwrap_or(i64::MAX)
    };
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => round(d),
        Err(e) => -round(e.duration()),
    }
}

/// `root` joined with a `/`-separated relative path.
pub fn absolute(root: &Path, rel_path: &str) -> PathBuf {
    let mut p = root.to_path_buf();
    p.extend(rel_path.split('/').filter(|c| !c.is_empty()));
    p
}

/// Walks `scope` (relative to `root`; `""` for the root itself).
///
/// `cancel` is checked once per folder; a cancelled walk answers what it
/// had with `complete: false`. `found` is called with the number of
/// supported files listed so far.
///
/// Fails with `not_a_folder` or `folder_unreadable` when the scope's own
/// folder cannot be listed; nothing is listed then.
pub fn walk(
    root: &Path,
    scope: &str,
    cancel: &AtomicBool,
    mut found: impl FnMut(usize),
) -> AppResult<Walk> {
    let start = absolute(root, scope);
    let shown = start.display().to_string();
    match std::fs::metadata(&start) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => {
            return Err(AppError::new(
                ErrorCode::NotAFolder,
                text::album::not_a_folder(&shown),
            ));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(AppError::new(
                ErrorCode::NotAFolder,
                text::album::not_a_folder(&shown),
            ));
        }
        Err(e) => {
            return Err(AppError::new(
                ErrorCode::FolderUnreadable,
                text::album::folder_unreadable(&shown, &e),
            ));
        }
    }

    let mut out = Walk {
        complete: true,
        ..Walk::default()
    };
    let mut folders = vec![scope.to_owned()];
    let mut first = true;
    while let Some(rel_dir) = folders.pop() {
        if cancel.load(Ordering::Relaxed) {
            out.complete = false;
            break;
        }
        let dir = absolute(root, &rel_dir);
        let entries = match read_folder(&dir) {
            Ok(entries) => entries,
            Err(e) if first => {
                return Err(AppError::new(
                    ErrorCode::FolderUnreadable,
                    text::album::folder_unreadable(&shown, &e),
                ));
            }
            Err(e) => {
                out.unreadable.push(UnreadableFolder {
                    rel_path: rel_dir,
                    message: e.to_string(),
                });
                continue;
            }
        };
        first = false;
        for entry in entries {
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                out.unreadable_names += 1;
                continue;
            };
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let rel_path = if rel_dir.is_empty() {
                name.clone()
            } else {
                format!("{rel_dir}/{name}")
            };
            match classify(&name, kind) {
                Rule::Skip => {}
                Rule::Descend => folders.push(rel_path),
                Rule::List => {
                    // A file that vanished between listing and stat is
                    // simply not there.
                    let Ok(meta) = entry.metadata() else {
                        continue;
                    };
                    out.files.push(Listed {
                        rel_path,
                        size: meta.len(),
                        mtime_ms: mtime_ms(&meta),
                    });
                    found(out.files.len());
                }
                Rule::Unsupported(u) => out.unsupported.push(UnsupportedFile {
                    rel_path,
                    label: u.label.to_owned(),
                    reason: u.reason.to_owned(),
                }),
            }
        }
    }
    out.files
        .sort_by(|a, b| natural_cmp(&a.rel_path, &b.rel_path));
    out.unsupported
        .sort_by(|a, b| natural_cmp(&a.rel_path, &b.rel_path));
    Ok(out)
}

/// Every entry of one folder. A failure part-way counts as the folder
/// being unreadable.
fn read_folder(dir: &Path) -> std::io::Result<Vec<std::fs::DirEntry>> {
    std::fs::read_dir(dir)?.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn touch(root: &Path, rel: &str) {
        let p = absolute(root, rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"x").unwrap();
    }

    fn listed(w: &Walk) -> Vec<&str> {
        w.files.iter().map(|f| f.rel_path.as_str()).collect()
    }

    fn run(root: &Path, scope: &str) -> Walk {
        walk(root, scope, &AtomicBool::new(false), |_| {}).unwrap()
    }

    #[test]
    fn lists_supported_files_in_walker_order_with_subfolders() {
        let tmp = tempfile::tempdir().unwrap();
        for rel in [
            "b/IMG_10.jpg",
            "b/IMG_2.jpg",
            "a.png",
            "b/c/d.gif",
            "notes.txt",
        ] {
            touch(tmp.path(), rel);
        }
        let w = run(tmp.path(), "");
        assert_eq!(
            listed(&w),
            ["a.png", "b/c/d.gif", "b/IMG_2.jpg", "b/IMG_10.jpg"]
        );
        assert!(w.complete);
        assert_eq!(w.files[0].size, 1);
        assert!(
            w.files[0].mtime_ms > 1_500_000_000_000,
            "{}",
            w.files[0].mtime_ms
        );
    }

    #[test]
    fn dot_names_and_appledouble_files_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        for rel in [
            ".owlbum/x.jpg",
            ".git/y.jpg",
            ".hidden.jpg",
            "._IMG_1.jpg",
            "a/._b.jpg",
            "a/.Trash-1000/c.jpg",
            "a/ok.jpg",
        ] {
            touch(tmp.path(), rel);
        }
        assert_eq!(listed(&run(tmp.path(), "")), ["a/ok.jpg"]);
    }

    #[test]
    fn unsupported_files_are_counted_not_listed() {
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "p/photo.heic");
        touch(tmp.path(), "p/photo.avif");
        touch(tmp.path(), "p/photo.jpg");
        let w = run(tmp.path(), "");
        assert_eq!(listed(&w), ["p/photo.jpg"]);
        let labels: Vec<_> = w.unsupported.iter().map(|u| u.label.as_str()).collect();
        assert_eq!(labels, ["AVIF", "HEIC"]);
        assert_eq!(
            w.unsupported[1].reason,
            "HEIC photos are not supported yet."
        );
    }

    #[test]
    fn a_scope_walks_only_its_folder_with_root_relative_paths() {
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "2024/a.jpg");
        touch(tmp.path(), "2024/x/b.jpg");
        touch(tmp.path(), "2024-old/c.jpg");
        touch(tmp.path(), "d.jpg");
        assert_eq!(
            listed(&run(tmp.path(), "2024")),
            ["2024/a.jpg", "2024/x/b.jpg"]
        );
    }

    #[test]
    fn the_found_count_grows_as_files_are_listed() {
        let tmp = tempfile::tempdir().unwrap();
        for i in 0..5 {
            touch(tmp.path(), &format!("{i}.jpg"));
        }
        let mut seen = Vec::new();
        walk(tmp.path(), "", &AtomicBool::new(false), |n| seen.push(n)).unwrap();
        assert_eq!(seen, [1, 2, 3, 4, 5]);
    }

    #[test]
    fn a_cancelled_walk_is_incomplete() {
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "a.jpg");
        let w = walk(tmp.path(), "", &AtomicBool::new(true), |_| {}).unwrap();
        assert!(!w.complete);
        assert!(w.files.is_empty());
    }

    #[test]
    fn a_missing_root_is_not_a_folder_and_a_file_root_too() {
        let tmp = tempfile::tempdir().unwrap();
        let e = walk(
            &tmp.path().join("gone"),
            "",
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap_err();
        assert_eq!(e.code, ErrorCode::NotAFolder);
        assert!(
            e.message.starts_with("Not a readable folder: "),
            "{}",
            e.message
        );
        touch(tmp.path(), "f.jpg");
        let e = walk(tmp.path(), "f.jpg", &AtomicBool::new(false), |_| {}).unwrap_err();
        assert_eq!(e.code, ErrorCode::NotAFolder);
    }

    #[test]
    fn under_compares_whole_components() {
        assert!(is_under("2024/a.jpg", "2024"));
        assert!(is_under("2024/x/a.jpg", "2024"));
        assert!(is_under("2024", "2024"));
        assert!(!is_under("2024-old/a.jpg", "2024"));
        assert!(!is_under("2024a.jpg", "2024"));
        assert!(is_under("anything", ""));
    }

    #[test]
    fn mtime_rounds_to_the_nearest_millisecond() {
        use std::time::Duration;
        let at = |ns: u64| system_time_ms(UNIX_EPOCH + Duration::from_nanos(ns));
        assert_eq!(at(1_499_999), 1);
        assert_eq!(at(1_500_000), 2);
        assert_eq!(system_time_ms(UNIX_EPOCH - Duration::from_millis(3)), -3);
    }

    #[cfg(unix)]
    #[test]
    fn an_unreadable_subfolder_is_reported_and_the_rest_listed() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "locked/a.jpg");
        touch(tmp.path(), "open/b.jpg");
        let locked = tmp.path().join("locked");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        if fs::read_dir(&locked).is_ok() {
            // Running as root: permissions do not stop reading.
            fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
            eprintln!("skipped: permissions do not apply to this user");
            return;
        }
        let w = run(tmp.path(), "");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(listed(&w), ["open/b.jpg"]);
        assert_eq!(w.unreadable.len(), 1);
        assert_eq!(w.unreadable[0].rel_path, "locked");
        assert!(w.is_under_unreadable("locked/a.jpg"));
    }

    #[cfg(unix)]
    #[test]
    fn an_unreadable_root_fails() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("r");
        touch(&root, "a.jpg");
        fs::set_permissions(&root, fs::Permissions::from_mode(0o000)).unwrap();
        let result = walk(&root, "", &AtomicBool::new(false), |_| {});
        fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
        if let Err(e) = result {
            assert_eq!(e.code, ErrorCode::FolderUnreadable);
            assert!(
                e.message.starts_with("Can't read the folder "),
                "{}",
                e.message
            );
        } else {
            eprintln!("skipped: permissions do not apply to this user");
        }
    }

    #[test]
    fn links_are_not_followed() {
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "real/a.jpg");
        let made = symlink_dir(&tmp.path().join("real"), &tmp.path().join("link"));
        if !made {
            eprintln!("skipped: this system will not make a link here");
            return;
        }
        assert_eq!(listed(&run(tmp.path(), "")), ["real/a.jpg"]);
    }

    #[cfg(unix)]
    fn symlink_dir(target: &Path, link: &Path) -> bool {
        std::os::unix::fs::symlink(target, link).is_ok()
    }

    #[cfg(windows)]
    fn symlink_dir(target: &Path, link: &Path) -> bool {
        // Symbolic links need developer mode or elevation on Windows; a
        // junction does not.
        std::os::windows::fs::symlink_dir(target, link).is_ok()
            || std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(link)
                .arg(target)
                .output()
                .is_ok_and(|o| o.status.success())
    }
}
