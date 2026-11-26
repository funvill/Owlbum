//! Where cached pictures live under app data, and how they are written and
//! read back (docs/02-data.md, "The app data folder"; docs/03, "Writing").
//!
//! Every path made here is inside `thumbs/` or `full/`. Only those paths
//! are ever deleted by the cache: a file that fails the end-marker check is
//! one of Owlbum's own copies, never a user's photo.

use std::io;
use std::path::{Path, PathBuf};

use super::Kind;

/// `<app data>/thumbs`.
pub fn thumbs_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("thumbs")
}

/// `<app data>/full`.
pub fn full_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("full")
}

/// True for a `thumb_key`: 40 lower-case hex digits. Anything else (from a
/// URL) never reaches the file system.
pub fn is_thumb_key(key: &str) -> bool {
    key.len() == 40 && key.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// The file that holds `key`'s picture of `kind`:
/// `thumbs/<aa>/<key>_s.jpg`, `thumbs/<aa>/<key>_m.jpg` or
/// `full/<key>.jpg`.
pub fn path(data_dir: &Path, key: &str, kind: Kind) -> PathBuf {
    match kind {
        Kind::Small | Kind::Medium => thumbs_dir(data_dir)
            .join(key.get(..2).unwrap_or("00"))
            .join(format!("{key}_{}.jpg", kind.as_str())),
        Kind::Full => full_dir(data_dir).join(format!("{key}.jpg")),
    }
}

/// True when `bytes` is a whole JPEG: not empty, starting with the start
/// marker and ending with the end marker (`FF D9`). A file cut short by a
/// power loss fails.
pub fn is_whole_jpeg(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes.starts_with(&[0xFF, 0xD8]) && bytes.ends_with(&[0xFF, 0xD9])
}

/// Reads a cached picture. A file that is not a whole JPEG is deleted and
/// None answered, so it is made again; a file that is not there is None.
pub fn read(path: &Path) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    if is_whole_jpeg(&bytes) {
        Some(bytes)
    } else {
        let _ = std::fs::remove_file(path);
        None
    }
}

/// True when a whole cached picture is at `path`, without reading all of
/// it: its size and its last two bytes.
pub fn is_whole_file(path: &Path) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let mut head = [0u8; 2];
    let mut tail = [0u8; 2];
    f.read_exact(&mut head).is_ok()
        && f.seek(SeekFrom::End(-2)).is_ok()
        && f.read_exact(&mut tail).is_ok()
        && head == [0xFF, 0xD8]
        && tail == [0xFF, 0xD9]
}

/// Writes `bytes` to `path` through `<path>.<random>.tmp` in the same
/// folder, renamed over the final name. When the rename fails but the
/// final file exists, another thread got there first, and that is
/// success. The temporary file is removed in every case. Not flushed with
/// `fsync`: this is a cache, and [`read`] catches a file cut short.
pub fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let random = uuid::Uuid::new_v4().simple().to_string();
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}.tmp", &random[..12]));
    let tmp = PathBuf::from(tmp);
    let result = std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, path));
    let _ = std::fs::remove_file(&tmp);
    match result {
        Err(_) if path.is_file() => Ok(()),
        other => other,
    }
}

/// Deletes one cached picture; one already gone is not an error.
pub fn remove(path: &Path) {
    let _ = std::fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "ab34567890abcdef1234567890abcdef12345678";

    #[test]
    fn paths_follow_the_documented_layout() {
        let d = Path::new("/data");
        assert_eq!(
            path(d, KEY, Kind::Small),
            Path::new("/data/thumbs/ab").join(format!("{KEY}_s.jpg"))
        );
        assert_eq!(
            path(d, KEY, Kind::Medium),
            Path::new("/data/thumbs/ab").join(format!("{KEY}_m.jpg"))
        );
        assert_eq!(
            path(d, KEY, Kind::Full),
            Path::new("/data/full").join(format!("{KEY}.jpg"))
        );
    }

    #[test]
    fn only_forty_lower_case_hex_digits_are_a_key() {
        assert!(is_thumb_key(KEY));
        assert!(!is_thumb_key(&KEY.to_uppercase()));
        assert!(!is_thumb_key(&KEY[1..]));
        assert!(!is_thumb_key("../../../../../../../../../../etc/passwd1"));
        assert!(!is_thumb_key(""));
    }

    #[test]
    fn the_end_marker_check() {
        assert!(is_whole_jpeg(&[0xFF, 0xD8, 0xFF, 0xD9]));
        assert!(!is_whole_jpeg(&[]));
        assert!(!is_whole_jpeg(&[0xFF, 0xD8, 0x00, 0x00]));
        assert!(!is_whole_jpeg(&[0x00, 0xD8, 0xFF, 0xD9]));
    }

    #[test]
    fn write_leaves_only_the_final_file() {
        let tmp = tempfile::tempdir().unwrap();
        let p = path(tmp.path(), KEY, Kind::Small);
        write(&p, &[0xFF, 0xD8, 1, 0xFF, 0xD9]).unwrap();
        assert!(is_whole_file(&p));
        assert_eq!(read(&p).unwrap(), [0xFF, 0xD8, 1, 0xFF, 0xD9]);
        let names: Vec<_> = std::fs::read_dir(p.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, [p.file_name().unwrap()]);
        // Writing again over it is fine.
        write(&p, &[0xFF, 0xD8, 2, 0xFF, 0xD9]).unwrap();
        assert_eq!(read(&p).unwrap()[2], 2);
    }

    #[test]
    fn a_file_cut_short_is_deleted_when_read() {
        let tmp = tempfile::tempdir().unwrap();
        let p = path(tmp.path(), KEY, Kind::Medium);
        write(&p, &[0xFF, 0xD8, 1, 2, 3]).unwrap();
        assert!(!is_whole_file(&p));
        assert!(read(&p).is_none());
        assert!(!p.exists());
        let empty = path(tmp.path(), KEY, Kind::Full);
        write(&empty, &[]).unwrap();
        assert!(read(&empty).is_none());
        assert!(!empty.exists());
    }
}
