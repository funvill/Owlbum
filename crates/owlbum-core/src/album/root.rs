//! Finding the album root (docs/02-data.md, "Finding the album root").

use std::path::{Component, Path, PathBuf};

use crate::error::AppResult;
use crate::text;
use crate::{AppError, ErrorCode};

/// The folder inside the album root that holds Owlbum's files.
pub const OWLBUM_DIR: &str = ".owlbum";
/// The album database's file name inside [`OWLBUM_DIR`].
pub const DATABASE_FILE: &str = "owlbum.sqlite";

/// Where an opened folder's album lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumLocation {
    /// The album root: the folder that holds (or will hold) `.owlbum/`.
    /// Absolute, as the user would recognise it (never a `\\?\` path).
    pub root: PathBuf,
    /// The opened folder relative to the root, with `/` separators; `""`
    /// when the root itself was opened. A non-empty scope is a scoped open.
    pub scope: String,
    /// True when the root already has `.owlbum/owlbum.sqlite`. False means
    /// the opened folder becomes a new album root.
    pub exists: bool,
}

impl AlbumLocation {
    /// `<root>/.owlbum`.
    pub fn owlbum_dir(&self) -> PathBuf {
        self.root.join(OWLBUM_DIR)
    }

    /// `<root>/.owlbum/owlbum.sqlite`.
    pub fn database_path(&self) -> PathBuf {
        database_path(&self.root)
    }

    /// True when the opened folder is below the root.
    pub fn is_scoped(&self) -> bool {
        !self.scope.is_empty()
    }

    /// The folder the user opened: the root, or the scope inside it.
    pub fn opened(&self) -> PathBuf {
        self.scope
            .split('/')
            .filter(|part| !part.is_empty())
            .fold(self.root.clone(), |path, part| path.join(part))
    }
}

/// `<root>/.owlbum/owlbum.sqlite`.
pub fn database_path(root: &Path) -> PathBuf {
    root.join(OWLBUM_DIR).join(DATABASE_FILE)
}

/// Finds the album root for `folder`.
///
/// Walks upward from `folder` to the drive root; the nearest folder that
/// contains `.owlbum/owlbum.sqlite` is the root (`folder` itself counts). A
/// `.owlbum` folder with no database in it does not count. When none is
/// found, `folder` is the root of a new album. Nothing is created.
///
/// Fails with `not_a_folder` when `folder` does not exist or is a file, and
/// with `folder_unreadable` when it cannot be read.
pub fn locate(folder: &Path) -> AppResult<AlbumLocation> {
    let shown = folder.display();
    match std::fs::metadata(folder) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => {
            return Err(AppError::new(
                ErrorCode::NotAFolder,
                text::album::not_a_folder(shown),
            ));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(AppError::new(
                ErrorCode::NotAFolder,
                text::album::not_a_folder(shown),
            ));
        }
        Err(e) => {
            return Err(AppError::new(
                ErrorCode::FolderUnreadable,
                text::album::folder_unreadable(shown, e),
            ));
        }
    }
    if let Err(e) = std::fs::read_dir(folder) {
        return Err(AppError::new(
            ErrorCode::FolderUnreadable,
            text::album::folder_unreadable(shown, e),
        ));
    }

    let folder = normalise(folder)?;
    for candidate in folder.ancestors() {
        if database_path(candidate).is_file() {
            return Ok(AlbumLocation {
                root: candidate.to_path_buf(),
                scope: scope_of(&folder, candidate),
                exists: true,
            });
        }
    }
    Ok(AlbumLocation {
        root: folder,
        scope: String::new(),
        exists: false,
    })
}

/// Makes `path` absolute and removes `.` and `..` by reading the path, not
/// the disk, so the result is the path the user would recognise (no `\\?\`
/// prefix and no resolved links).
fn normalise(path: &Path) -> AppResult<PathBuf> {
    let absolute = std::path::absolute(path).map_err(|e| {
        AppError::new(
            ErrorCode::FolderUnreadable,
            text::album::folder_unreadable(path.display(), e),
        )
    })?;
    let mut out = PathBuf::new();
    for part in absolute.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                // At the drive root `..` stays at the root, as the OS does.
                if out.parent().is_some() {
                    out.pop();
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    Ok(out)
}

/// `folder` relative to `root`, `/`-separated; `""` when they are the same.
fn scope_of(folder: &Path, root: &Path) -> String {
    folder
        .strip_prefix(root)
        .map(|rel| {
            rel.components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/")
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_album(root: &Path) {
        fs::create_dir_all(root.join(OWLBUM_DIR)).unwrap();
        fs::write(database_path(root), b"").unwrap();
    }

    #[test]
    fn a_folder_with_no_album_above_it_is_a_new_root() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("Photos");
        fs::create_dir(&f).unwrap();

        let loc = locate(&f).unwrap();
        assert_eq!(loc.root, normalise(&f).unwrap());
        assert_eq!(loc.scope, "");
        assert!(!loc.exists);
        assert!(!loc.is_scoped());
        assert!(!f.join(OWLBUM_DIR).exists(), "locate creates nothing");
    }

    #[test]
    fn the_opened_folder_itself_counts() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("Photos");
        make_album(tmp.path());
        fs::create_dir(&f).unwrap();
        make_album(&f);

        let loc = locate(&f).unwrap();
        assert_eq!(loc.root, normalise(&f).unwrap());
        assert_eq!(loc.scope, "");
        assert!(loc.exists);
    }

    #[test]
    fn a_subfolder_opens_scoped_with_slash_separators() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Photos");
        let f = root.join("2024").join("Trip");
        fs::create_dir_all(&f).unwrap();
        make_album(&root);

        let loc = locate(&f).unwrap();
        assert_eq!(loc.root, normalise(&root).unwrap());
        assert_eq!(loc.scope, "2024/Trip");
        assert!(loc.exists);
        assert!(loc.is_scoped());
        assert_eq!(
            loc.database_path(),
            normalise(&root)
                .unwrap()
                .join(".owlbum")
                .join("owlbum.sqlite")
        );
    }

    #[test]
    fn the_nearest_album_root_wins() {
        let tmp = tempfile::tempdir().unwrap();
        let outer = tmp.path().join("Photos");
        let inner = outer.join("2024");
        let f = inner.join("Trip");
        fs::create_dir_all(&f).unwrap();
        make_album(&outer);
        make_album(&inner);

        let loc = locate(&f).unwrap();
        assert_eq!(loc.root, normalise(&inner).unwrap());
        assert_eq!(loc.scope, "Trip");
    }

    #[test]
    fn an_owlbum_folder_without_a_database_does_not_count() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Photos");
        let f = root.join("Trip");
        fs::create_dir_all(&f).unwrap();
        fs::create_dir(root.join(OWLBUM_DIR)).unwrap();

        let loc = locate(&f).unwrap();
        assert_eq!(loc.root, normalise(&f).unwrap());
        assert!(!loc.exists);
    }

    #[test]
    fn a_database_path_that_is_a_folder_does_not_count() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Photos");
        fs::create_dir_all(database_path(&root)).unwrap();

        let loc = locate(&root).unwrap();
        assert!(!loc.exists);
    }

    #[test]
    fn dot_and_dot_dot_are_removed_from_the_path() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Photos");
        fs::create_dir_all(root.join("a").join("b")).unwrap();
        make_album(&root);

        let loc = locate(&root.join("a").join(".").join("b").join("..")).unwrap();
        assert_eq!(loc.root, normalise(&root).unwrap());
        assert_eq!(loc.scope, "a");
    }

    #[test]
    fn the_root_is_absolute_and_has_no_verbatim_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        let loc = locate(tmp.path()).unwrap();
        assert!(loc.root.is_absolute());
        assert!(!loc.root.to_string_lossy().starts_with(r"\\?\"));
    }

    #[test]
    fn a_missing_path_is_not_a_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let e = locate(&tmp.path().join("nope")).unwrap_err();
        assert_eq!(e.code, ErrorCode::NotAFolder);
        assert!(e.message.contains("nope"), "{}", e.message);
    }

    #[test]
    fn a_file_is_not_a_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("a.jpg");
        fs::write(&file, b"x").unwrap();
        assert_eq!(locate(&file).unwrap_err().code, ErrorCode::NotAFolder);
    }
}
