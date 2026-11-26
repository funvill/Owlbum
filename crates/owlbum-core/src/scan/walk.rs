//! The walker (docs/03-media-and-scanning.md, "The walker").
//!
//! It lists the files of the scope (the album root, or the scoped folder),
//! always with subfolders, reading directory entries without following
//! links. What it leaves out is decided per entry by [`classify`]: dot
//! names, system folders and links by name and type; nested album roots by
//! one existence test per folder; online-only files by the attributes the
//! directory listing gives. The scan report ([`super::report`]) is made
//! from what a [`Walk`] counted.

use std::collections::HashSet;
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
    /// The format's label, as the unsupported table names it.
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
    /// Symbolic links (and, on Windows, junctions), skipped.
    pub links: u32,
    /// Supported files stored online only (cloud placeholders): not listed,
    /// never read, and their rows are not gone.
    pub online_only: HashSet<String>,
    /// Subfolders that are album roots of their own, skipped whole, in
    /// walker order.
    pub nested: Vec<String>,
    /// False when the walk was cancelled before it ended (and for a walk
    /// that never ran).
    pub complete: bool,
}

impl Walk {
    /// True when `rel_path` lies in a subfolder the walk could not read.
    pub fn is_under_unreadable(&self, rel_path: &str) -> bool {
        self.unreadable
            .iter()
            .any(|f| is_under(rel_path, &f.rel_path))
    }

    /// True when the file at `rel_path` was found stored online only.
    pub fn is_online_only(&self, rel_path: &str) -> bool {
        self.online_only.contains(rel_path)
    }

    /// The `skipped` count of `album_status`: links, online-only files and
    /// names that are not Unicode. System folders and dot names are left
    /// out silently; nested albums and unreadable folders are listed in the
    /// scan report instead.
    pub fn skipped(&self) -> u32 {
        let online = u32::try_from(self.online_only.len()).unwrap_or(u32::MAX);
        self.links
            .saturating_add(online)
            .saturating_add(self.unreadable_names)
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

/// Folders that hold deleted files or another program's thumbnails,
/// skipped wherever they are. Compared without case.
pub const SYSTEM_FOLDERS: [&str; 6] = [
    "$RECYCLE.BIN",
    "System Volume Information",
    "@eaDir",
    "#recycle",
    "#snapshot",
    "__MACOSX",
];

/// Windows file attributes that mark a file whose content is not on this
/// computer: reading it would download it (values from `windows-sys`).
const OFFLINE: u32 = 0x1000;
const RECALL_ON_OPEN: u32 = 0x0004_0000;
const RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;

/// True when a file with these Windows attributes is stored online only:
/// `RECALL_ON_DATA_ACCESS` (a cloud placeholder, such as a OneDrive file
/// after "Free up space"), `RECALL_ON_OPEN` or `OFFLINE`.
pub fn is_online_only(attributes: u32) -> bool {
    attributes & (OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS) != 0
}

/// True when the file `meta` describes is stored online only. The
/// attributes come from the directory listing, so nothing is opened. Never
/// on systems other than Windows.
fn online_only(meta: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        is_online_only(meta.file_attributes())
    }
    #[cfg(not(windows))]
    {
        let _ = meta;
        false
    }
}

/// True when `dir` holds an album database of its own.
fn is_album_root(dir: &Path) -> bool {
    std::fs::symlink_metadata(dir.join(".owlbum").join("owlbum.sqlite")).is_ok()
}

/// A directory entry's type, as the listing reports it (links are not
/// followed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Entry {
    /// A symbolic link or junction, to anything.
    Link,
    Folder,
    File,
    /// A device, socket, pipe and so on.
    Other,
}

impl From<FileType> for Entry {
    fn from(kind: FileType) -> Self {
        if kind.is_symlink() {
            Self::Link
        } else if kind.is_dir() {
            Self::Folder
        } else if kind.is_file() {
            Self::File
        } else {
            Self::Other
        }
    }
}

/// What the walker does with one directory entry.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Rule {
    /// Left out silently.
    Skip,
    /// A link: left out and counted.
    Link,
    /// A folder to walk into, unless it is an album root of its own.
    Descend,
    /// A supported file to list, unless it is stored online only.
    List,
    /// A recognised but unsupported file, counted.
    Unsupported(&'static UnsupportedFormat),
}

/// Decides what to do with an entry, by its name and its type.
pub(crate) fn classify(name: &str, entry: Entry) -> Rule {
    // `.owlbum`, `.git`, `.Trash-1000`, dotfiles, and `._` AppleDouble
    // files (which also start with a dot; `is_supported` refuses them on
    // its own too, should this rule ever change).
    if name.starts_with('.') {
        return Rule::Skip;
    }
    match entry {
        // Never followed: links can loop, leave the album, or list one file
        // twice.
        Entry::Link => Rule::Link,
        Entry::Folder if SYSTEM_FOLDERS.iter().any(|s| s.eq_ignore_ascii_case(name)) => Rule::Skip,
        Entry::Folder => Rule::Descend,
        Entry::Other => Rule::Skip,
        Entry::File if media::is_supported(name) => Rule::List,
        Entry::File => media::unsupported_of(name).map_or(Rule::Skip, Rule::Unsupported),
    }
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
    found: impl FnMut(usize),
) -> AppResult<Walk> {
    check_scope(root, scope)?;
    let out = Walk {
        complete: true,
        ..Walk::default()
    };
    walk_from(
        root,
        scope,
        vec![(scope.to_owned(), true)],
        out,
        cancel,
        found,
    )
}

/// Walks only `folders` of `scope`: each a folder relative to `root` and
/// whether to walk the folders under it too (a targeted rescan,
/// docs/03-media-and-scanning.md, "Debounce"). Folders outside the scope are
/// ignored.
///
/// The walker's rules hold for the path down to each folder as they would
/// in a whole walk: a folder reached through a dot name, a system folder or
/// a link is not walked, and one inside a nested album root is not walked
/// (the nested root is listed). A folder that no longer exists is walked as
/// empty, so its rows are gone; a folder that cannot be read is listed as
/// unreadable, so its rows are kept.
///
/// Fails like [`walk`] when the scope's own folder cannot be listed, before
/// or after: a drive unplugged part-way must not make every folder look
/// empty.
pub fn walk_folders(
    root: &Path,
    scope: &str,
    folders: &[(String, bool)],
    cancel: &AtomicBool,
    found: impl FnMut(usize),
) -> AppResult<Walk> {
    check_scope(root, scope)?;
    let mut out = Walk {
        complete: true,
        ..Walk::default()
    };
    let mut starts = Vec::new();
    for (dir, recursive) in folders {
        if !is_under(dir, scope) {
            continue;
        }
        match reach(root, scope, dir) {
            Reach::Walk => starts.push((dir.clone(), *recursive)),
            Reach::Gone | Reach::Skip => {}
            Reach::Nested(n) => {
                if !out.nested.contains(&n) {
                    out.nested.push(n);
                }
            }
            Reach::Unreadable(f) => out.unreadable.push(f),
        }
    }
    let out = walk_from(root, scope, starts, out, cancel, found)?;
    check_scope(root, scope)?;
    Ok(out)
}

/// What stands between the scope and one folder of a targeted walk.
enum Reach {
    /// Nothing: walk it.
    Walk,
    /// It, or a folder on the way, no longer exists.
    Gone,
    /// A folder on the way is one the walker leaves out (a dot name, a
    /// system folder, a link).
    Skip,
    /// A folder on the way (or it) is an album root of its own.
    Nested(String),
    /// A folder on the way could not be looked at.
    Unreadable(UnreadableFolder),
}

/// Checks the path from `scope` down to `dir` by the walker's rules, one
/// component at a time, without following links.
fn reach(root: &Path, scope: &str, dir: &str) -> Reach {
    let below = dir.get(scope.len()..).unwrap_or("");
    let mut current = scope.to_owned();
    for name in below.split('/').filter(|c| !c.is_empty()) {
        if !current.is_empty() {
            current.push('/');
        }
        current.push_str(name);
        let abs = absolute(root, &current);
        let meta = match std::fs::symlink_metadata(&abs) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Reach::Gone,
            Err(e) => {
                return Reach::Unreadable(UnreadableFolder {
                    rel_path: current,
                    message: e.to_string(),
                });
            }
        };
        match classify(name, Entry::from(meta.file_type())) {
            Rule::Descend if is_album_root(&abs) => return Reach::Nested(current),
            Rule::Descend => {}
            // A file where the folder was: it holds nothing.
            Rule::List | Rule::Unsupported(_) => return Reach::Gone,
            Rule::Skip | Rule::Link => return Reach::Skip,
        }
    }
    Reach::Walk
}

/// Fails with `not_a_folder` or `folder_unreadable` unless the scope's own
/// folder is a folder that can be looked at.
fn check_scope(root: &Path, scope: &str) -> AppResult<()> {
    let start = absolute(root, scope);
    let shown = start.display().to_string();
    match std::fs::metadata(&start) {
        Ok(m) if m.is_dir() => Ok(()),
        Ok(_) => Err(AppError::new(
            ErrorCode::NotAFolder,
            text::album::not_a_folder(&shown),
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(AppError::new(
            ErrorCode::NotAFolder,
            text::album::not_a_folder(&shown),
        )),
        Err(e) => Err(AppError::new(
            ErrorCode::FolderUnreadable,
            text::album::folder_unreadable(&shown, &e),
        )),
    }
}

/// Walks from each of `starts` (a folder, and whether to go into its
/// subfolders), adding to `out`. The scope's own folder failing to list
/// fails the walk; any other start folder that no longer exists is empty.
fn walk_from(
    root: &Path,
    scope: &str,
    starts: Vec<(String, bool)>,
    mut out: Walk,
    cancel: &AtomicBool,
    mut found: impl FnMut(usize),
) -> AppResult<Walk> {
    let starts_at: HashSet<String> = starts.iter().map(|(d, _)| d.clone()).collect();
    let mut folders = starts;
    while let Some((rel_dir, recursive)) = folders.pop() {
        if cancel.load(Ordering::Relaxed) {
            out.complete = false;
            break;
        }
        let dir = absolute(root, &rel_dir);
        let is_scope = rel_dir == scope;
        // The walk's own folder is never nested.
        if !is_scope && is_album_root(&dir) {
            out.nested.push(rel_dir);
            continue;
        }
        let entries = match read_folder(&dir) {
            Ok(entries) => entries,
            Err(e) if is_scope => {
                let shown = dir.display().to_string();
                return Err(AppError::new(
                    ErrorCode::FolderUnreadable,
                    text::album::folder_unreadable(&shown, &e),
                ));
            }
            // A start folder of a targeted walk that has gone since: empty.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && starts_at.contains(&rel_dir) => {
                continue;
            }
            Err(e) => {
                out.unreadable.push(UnreadableFolder {
                    rel_path: rel_dir,
                    message: e.to_string(),
                });
                continue;
            }
        };
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
            match classify(&name, Entry::from(kind)) {
                Rule::Skip => {}
                Rule::Link => out.links += 1,
                Rule::Descend => {
                    if recursive {
                        folders.push((rel_path, true));
                    }
                }
                Rule::List => {
                    // On Windows this comes from the directory listing and
                    // opens nothing. A file that vanished between listing
                    // and stat is simply not there.
                    let Ok(meta) = entry.metadata() else {
                        continue;
                    };
                    if online_only(&meta) {
                        out.online_only.insert(rel_path);
                        continue;
                    }
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
    out.nested.sort_by(|a, b| natural_cmp(a, b));
    Ok(out)
}

// Every entry of one folder. A failure part-way counts as the folder
/// being unreadable.
fn read_folder(dir: &Path) -> std::io::Result<Vec<std::fs::DirEntry>> {
    std::fs::read_dir(dir)?.collect()
}

#[cfg(test)]
pub(crate) mod tests {
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
        let label = |name: &str| media::unsupported_of(name).unwrap().label;
        assert_eq!(labels, [label("photo.avif"), label("photo.heic")]);
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
    fn system_folders_are_skipped_at_any_depth_without_case() {
        let tmp = tempfile::tempdir().unwrap();
        for rel in [
            "$RECYCLE.BIN/a.jpg",
            "x/System Volume Information/b.jpg",
            "x/system volume information/c.jpg",
            "@EADIR/d.jpg",
            "y/z/@eaDir/e.jpg",
            "#recycle/f.jpg",
            "#Snapshot/g.jpg",
            "__macosx/h.jpg",
            // Only those exact names, and only folders.
            "my @eaDir/ok1.jpg",
            "__MACOSX-old/ok2.jpg",
            "x/ok3.jpg",
        ] {
            touch(tmp.path(), rel);
        }
        let w = run(tmp.path(), "");
        assert_eq!(
            listed(&w),
            ["__MACOSX-old/ok2.jpg", "my @eaDir/ok1.jpg", "x/ok3.jpg"]
        );
        // Left out silently: not links, not unreadable, not nested.
        assert_eq!(w.links, 0);
        assert!(w.unreadable.is_empty());
        assert!(w.nested.is_empty());
    }

    #[test]
    fn a_file_with_a_system_folder_name_is_not_a_folder() {
        for name in SYSTEM_FOLDERS {
            assert!(matches!(classify(name, Entry::Folder), Rule::Skip));
            assert!(matches!(
                classify(&name.to_uppercase(), Entry::Folder),
                Rule::Skip
            ));
        }
        assert!(matches!(classify("#recycle", Entry::File), Rule::Skip));
        assert!(matches!(classify("@eaDir.jpg", Entry::File), Rule::List));
    }

    #[test]
    fn links_are_not_followed_and_are_counted() {
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "real/a.jpg");
        let made = symlink_dir(&tmp.path().join("real"), &tmp.path().join("link"));
        if !made {
            eprintln!("skipped: this system will not make a link here");
            return;
        }
        let w = run(tmp.path(), "");
        assert_eq!(listed(&w), ["real/a.jpg"]);
        assert_eq!(w.links, 1);
    }

    #[test]
    fn a_link_to_a_file_is_skipped_and_counted() {
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "a.jpg");
        if !symlink_file(&tmp.path().join("a.jpg"), &tmp.path().join("b.jpg")) {
            eprintln!("skipped: this system will not make a file link here");
            return;
        }
        let w = run(tmp.path(), "");
        assert_eq!(listed(&w), ["a.jpg"]);
        assert_eq!(w.links, 1);
    }

    /// A junction is the link most Windows users can make; the standard
    /// library must report it as a link, or the walker would follow it.
    #[cfg(windows)]
    #[test]
    fn a_junction_is_a_link_to_the_standard_library_and_is_counted() {
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "real/a.jpg");
        let link = tmp.path().join("junction");
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(tmp.path().join("real"))
            .output()
            .is_ok_and(|o| o.status.success());
        assert!(made, "mklink /J needs no special rights");
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        let entry = fs::read_dir(tmp.path())
            .unwrap()
            .map(Result::unwrap)
            .find(|e| e.file_name() == "junction")
            .unwrap();
        assert!(entry.file_type().unwrap().is_symlink());
        let w = run(tmp.path(), "");
        assert_eq!(listed(&w), ["real/a.jpg"]);
        assert_eq!(w.links, 1);
    }

    #[test]
    fn a_nested_album_root_is_skipped_whole_and_recorded() {
        let tmp = tempfile::tempdir().unwrap();
        for rel in [
            "inner/.owlbum/owlbum.sqlite",
            "inner/a.jpg",
            "inner/sub/b.jpg",
            "deep/x/.owlbum/owlbum.sqlite",
            "deep/x/d.jpg",
            "deep/y.jpg",
            // A `.owlbum` folder without the database is not an album.
            "half/.owlbum/other.txt",
            "half/e.jpg",
            "c.jpg",
        ] {
            touch(tmp.path(), rel);
        }
        let w = run(tmp.path(), "");
        assert_eq!(listed(&w), ["c.jpg", "deep/y.jpg", "half/e.jpg"]);
        assert_eq!(w.nested, ["deep/x", "inner"]);
        assert!(w.complete);
    }

    #[test]
    fn the_walks_own_folder_is_never_nested() {
        let tmp = tempfile::tempdir().unwrap();
        for rel in [
            ".owlbum/owlbum.sqlite",
            "a.jpg",
            "inner/.owlbum/owlbum.sqlite",
            "inner/b.jpg",
        ] {
            touch(tmp.path(), rel);
        }
        let w = run(tmp.path(), "");
        assert_eq!(listed(&w), ["a.jpg"]);
        assert_eq!(w.nested, ["inner"]);
        // Scoped into the nested root itself, it is the walk's own folder.
        let w = run(tmp.path(), "inner");
        assert_eq!(listed(&w), ["inner/b.jpg"]);
        assert!(w.nested.is_empty());
    }

    #[test]
    fn online_only_is_told_by_attributes() {
        // The values from `windows-sys` (`Win32::Storage::FileSystem`).
        const READONLY: u32 = 0x1;
        const ARCHIVE: u32 = 0x20;
        const REPARSE_POINT: u32 = 0x400;
        const SPARSE_FILE: u32 = 0x200;
        const OFFLINE: u32 = 0x1000;
        const RECALL_ON_OPEN: u32 = 0x0004_0000;
        const PINNED: u32 = 0x0008_0000;
        const UNPINNED: u32 = 0x0010_0000;
        const RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
        // A OneDrive file with "Free up space": a cloud placeholder.
        assert!(is_online_only(
            ARCHIVE | REPARSE_POINT | SPARSE_FILE | UNPINNED | RECALL_ON_DATA_ACCESS
        ));
        assert!(is_online_only(RECALL_ON_DATA_ACCESS));
        assert!(is_online_only(OFFLINE));
        assert!(is_online_only(ARCHIVE | OFFLINE));
        assert!(is_online_only(RECALL_ON_OPEN));
        // A cloud file kept on this device, pinned or not: walked and read.
        assert!(!is_online_only(ARCHIVE | REPARSE_POINT | PINNED));
        assert!(!is_online_only(ARCHIVE | REPARSE_POINT));
        assert!(!is_online_only(ARCHIVE | READONLY | SPARSE_FILE));
        assert!(!is_online_only(0));
    }

    #[test]
    fn walk_knows_its_online_only_paths() {
        let w = Walk {
            online_only: ["a/b.jpg".to_owned()].into_iter().collect(),
            ..Walk::default()
        };
        assert!(w.is_online_only("a/b.jpg"));
        assert!(!w.is_online_only("a/c.jpg"));
        assert!(!w.is_online_only("a"));
    }

    /// The real attribute on a real NTFS file: `attrib +O` sets OFFLINE,
    /// which any user can set (RECALL_ON_DATA_ACCESS only a cloud driver
    /// can, so it is pinned by the attribute test above).
    #[cfg(windows)]
    #[test]
    fn an_offline_file_is_counted_online_only_and_not_listed() {
        let tmp = tempfile::tempdir().unwrap();
        for rel in [
            "a.jpg",
            "cloud/b.jpg",
            "cloud/c.heic",
            "cloud/notes.txt",
            "cloud/d.jpg",
        ] {
            touch(tmp.path(), rel);
        }
        for rel in ["cloud", "cloud/b.jpg", "cloud/c.heic", "cloud/notes.txt"] {
            set_offline(&absolute(tmp.path(), rel));
        }
        let w = run(tmp.path(), "");
        // An offline folder is still walked; only files are left alone.
        assert_eq!(listed(&w), ["a.jpg", "cloud/d.jpg"]);
        assert!(w.is_online_only("cloud/b.jpg"));
        assert_eq!(w.online_only.len(), 1, "{:?}", w.online_only);
        // A file Owlbum would not list anyway is not counted as online.
        assert_eq!(w.unsupported.len(), 1);
        assert_eq!(w.unsupported[0].rel_path, "cloud/c.heic");
        for rel in ["cloud", "cloud/b.jpg", "cloud/c.heic", "cloud/notes.txt"] {
            clear_offline(&absolute(tmp.path(), rel));
        }
    }

    #[cfg(windows)]
    pub(crate) fn set_offline(path: &Path) {
        attrib("+O", path);
    }

    #[cfg(windows)]
    pub(crate) fn clear_offline(path: &Path) {
        attrib("-O", path);
    }

    #[cfg(windows)]
    fn attrib(flag: &str, path: &Path) {
        let out = std::process::Command::new("attrib")
            .arg(flag)
            .arg(path)
            .output()
            .unwrap();
        assert!(out.status.success(), "attrib {flag} {}", path.display());
    }

    #[test]
    fn a_name_that_is_not_unicode_is_counted() {
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "ok.jpg");
        if !make_non_unicode_file(tmp.path()) {
            eprintln!("skipped: this file system will not store such a name");
            return;
        }
        let w = run(tmp.path(), "");
        assert_eq!(listed(&w), ["ok.jpg"]);
        assert_eq!(w.unreadable_names, 1);
    }

    #[cfg(unix)]
    fn make_non_unicode_file(dir: &Path) -> bool {
        use std::os::unix::ffi::OsStrExt;
        let name = std::ffi::OsStr::from_bytes(b"bad\xff.jpg");
        fs::write(dir.join(name), b"x").is_ok()
    }

    #[cfg(windows)]
    fn make_non_unicode_file(dir: &Path) -> bool {
        use std::os::windows::ffi::OsStringExt;
        // An unpaired surrogate: valid to NTFS, not valid Unicode.
        let wide: Vec<u16> = [0xD800].into_iter().chain(".jpg".encode_utf16()).collect();
        let name = std::ffi::OsString::from_wide(&wide);
        fs::write(dir.join(name), b"x").is_ok()
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

    #[cfg(unix)]
    fn symlink_file(target: &Path, link: &Path) -> bool {
        std::os::unix::fs::symlink(target, link).is_ok()
    }

    #[cfg(windows)]
    fn symlink_file(target: &Path, link: &Path) -> bool {
        // Needs developer mode or elevation.
        std::os::windows::fs::symlink_file(target, link).is_ok()
    }

    // ---- Targeted walks (the folder watcher's rescans) ----

    fn run_folders(root: &Path, scope: &str, folders: &[(&str, bool)]) -> AppResult<Walk> {
        let folders: Vec<(String, bool)> =
            folders.iter().map(|(d, r)| ((*d).to_owned(), *r)).collect();
        walk_folders(root, scope, &folders, &AtomicBool::new(false), |_| {})
    }

    #[test]
    fn a_targeted_walk_lists_only_its_folders() {
        let tmp = tempfile::tempdir().unwrap();
        for rel in [
            "top.jpg",
            "a/1.jpg",
            "a/sub/2.jpg",
            "b/3.jpg",
            "b/deep/er/4.jpg",
            "c/5.jpg",
        ] {
            touch(tmp.path(), rel);
        }
        let w = run_folders(tmp.path(), "", &[("a", false), ("b", true)]).unwrap();
        assert_eq!(listed(&w), ["a/1.jpg", "b/3.jpg", "b/deep/er/4.jpg"]);
        assert!(w.complete);
        let w = run_folders(tmp.path(), "", &[("", false)]).unwrap();
        assert_eq!(listed(&w), ["top.jpg"]);
    }

    #[test]
    fn a_targeted_walk_keeps_the_walkers_rules_on_the_way_down() {
        let tmp = tempfile::tempdir().unwrap();
        for rel in [
            ".hidden/a.jpg",
            "$RECYCLE.BIN/x/b.jpg",
            "@eaDir/c.jpg",
            "nested/.owlbum/owlbum.sqlite",
            "nested/d.jpg",
            "nested/inner/e.jpg",
            "ok/f.jpg",
        ] {
            touch(tmp.path(), rel);
        }
        let w = run_folders(
            tmp.path(),
            "",
            &[
                (".hidden", true),
                ("$RECYCLE.BIN/x", false),
                ("@eaDir", false),
                ("nested", false),
                ("nested/inner", true),
                ("ok", false),
            ],
        )
        .unwrap();
        assert_eq!(listed(&w), ["ok/f.jpg"]);
        assert_eq!(w.nested, ["nested"]);
    }

    #[test]
    fn a_targeted_walk_does_not_follow_a_link_to_its_folder() {
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "real/a.jpg");
        if !symlink_dir(&tmp.path().join("real"), &tmp.path().join("link")) {
            eprintln!("skipped: cannot make a link here");
            return;
        }
        let w = run_folders(tmp.path(), "", &[("link", true), ("link/x", false)]).unwrap();
        assert!(w.files.is_empty(), "{:?}", listed(&w));
    }

    #[test]
    fn a_targeted_folder_that_is_gone_is_empty_not_unreadable() {
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "a/1.jpg");
        let w = run_folders(
            tmp.path(),
            "",
            &[("gone", true), ("gone/deeper", false), ("a", false)],
        )
        .unwrap();
        assert_eq!(listed(&w), ["a/1.jpg"]);
        assert!(w.unreadable.is_empty());
        assert!(w.complete);
    }

    #[test]
    fn a_targeted_walk_fails_when_the_scope_is_gone() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("album");
        touch(&root, "a/1.jpg");
        let gone = tmp.path().join("unplugged");
        let err = run_folders(&gone, "", &[("a", false)]).unwrap_err();
        assert_eq!(err.code, ErrorCode::NotAFolder);
        // A scoped walk whose scope went away fails the same way.
        let err = run_folders(&root, "b", &[("b/c", false)]).unwrap_err();
        assert_eq!(err.code, ErrorCode::NotAFolder);
    }

    #[test]
    fn a_targeted_walk_ignores_folders_outside_its_scope() {
        let tmp = tempfile::tempdir().unwrap();
        for rel in ["s/a.jpg", "t/b.jpg"] {
            touch(tmp.path(), rel);
        }
        let w = run_folders(tmp.path(), "s", &[("t", false), ("s", false)]).unwrap();
        assert_eq!(listed(&w), ["s/a.jpg"]);
    }
}
