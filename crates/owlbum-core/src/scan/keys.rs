//! `file_key` and `thumb_key` (docs/02-data.md, "Schema" and "Thumbnail
//! keys and the cache").
//!
//! Both are stored in every album, so their bytes are fixed here and pinned
//! by known-answer tests:
//!
//! - `file_key = "fk1:" + size + ":" + hex(sha1(size as 8 bytes
//!   little-endian ‖ the first min(16 KiB, size) bytes ‖ the last
//!   min(16 KiB, size) bytes))`. A file under 32 KiB has its two samples
//!   overlap; an empty file hashes the size alone.
//! - `thumb_key = hex(sha1(album_id + "|" + file_key + "|" + mtime_ms))`,
//!   the three as UTF-8 text, `mtime_ms` in decimal.
//!
//! A file that could not be opened gets `fk1:<size>:` with an empty hash,
//! which never matches anything.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use sha1::{Digest, Sha1};

/// How much of each end of a file `file_key` reads.
pub const SAMPLE: usize = 16 * 1024;

/// What reading a file's two ends gives: its size when opened, its first
/// bytes (for sniffing) and its `file_key`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sample {
    /// The size of the open file.
    pub size: u64,
    /// The first [`SAMPLE`] bytes, or the whole file when it is smaller.
    pub head: Vec<u8>,
    /// `fk1:<size>:<hash>`.
    pub file_key: String,
}

/// Reads the first and last [`SAMPLE`] bytes of `path` (at most 32 KiB in
/// all) and computes its `file_key`. The file is opened read-only and
/// closed before this returns.
pub fn sample(path: &Path) -> std::io::Result<Sample> {
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    let head_len = usize::try_from(size).map_or(SAMPLE, |s| s.min(SAMPLE));
    let mut head = vec![0; head_len];
    file.read_exact(&mut head)?;
    let tail = if size <= SAMPLE as u64 {
        head.clone()
    } else {
        file.seek(SeekFrom::Start(size - SAMPLE as u64))?;
        let mut tail = vec![0; SAMPLE];
        file.read_exact(&mut tail)?;
        tail
    };
    let file_key = file_key(size, &head, &tail);
    Ok(Sample {
        size,
        head,
        file_key,
    })
}

/// `fk1:<size>:<sha1 of size, head, tail>`.
pub fn file_key(size: u64, head: &[u8], tail: &[u8]) -> String {
    let mut h = Sha1::new();
    h.update(size.to_le_bytes());
    h.update(head);
    h.update(tail);
    format!("fk1:{size}:{}", hex(&h.finalize()))
}

/// The key of a file that could not be read: an empty hash.
pub fn unreadable_file_key(size: u64) -> String {
    format!("fk1:{size}:")
}

/// True when `key` has an empty hash, which matches nothing.
pub fn is_unreadable_key(key: &str) -> bool {
    key.ends_with(':')
}

/// `sha1(album_id | file_key | mtime_ms)` in hex. It is the same for a file
/// that was only renamed or moved.
pub fn thumb_key(album_id: &str, file_key: &str, mtime_ms: i64) -> String {
    let mut h = Sha1::new();
    h.update(format!("{album_id}|{file_key}|{mtime_ms}").as_bytes());
    hex(&h.finalize())
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(40), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn known_answers() {
        // sha1 of eight zero bytes.
        assert_eq!(
            file_key(0, &[], &[]),
            "fk1:0:05fe405753166f125559e7c9ac558654f107c7e9"
        );
        // sha1("abc"), which is not a key but pins the hex and the hash.
        let mut h = Sha1::new();
        h.update(b"abc");
        assert_eq!(
            hex(&h.finalize()),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        // sha1("album|fk1:0:|0")
        assert_eq!(
            thumb_key("album", "fk1:0:", 0),
            "d5f1b89e984d7afff1edfcb30d1d98aed015851d"
        );
    }

    #[test]
    fn a_small_file_hashes_its_size_and_itself_twice() {
        let tmp = tempfile::tempdir().unwrap();
        let p = write(tmp.path(), "a.bin", b"hello");
        let s = sample(&p).unwrap();
        assert_eq!(s.size, 5);
        assert_eq!(s.head, b"hello");
        assert_eq!(s.file_key, file_key(5, b"hello", b"hello"));
        assert_eq!(s.file_key, "fk1:5:e89f3318b822138c83ebb08ec982bdcbee735130");
    }

    #[test]
    fn a_large_file_reads_only_its_two_ends() {
        let tmp = tempfile::tempdir().unwrap();
        let mut bytes = vec![1u8; 100_000];
        bytes[50_000] = 9; // in the middle: not sampled
        let p = write(tmp.path(), "a.bin", &bytes);
        let a = sample(&p).unwrap();
        assert_eq!(a.head.len(), SAMPLE);
        bytes[50_000] = 7;
        let q = write(tmp.path(), "b.bin", &bytes);
        assert_eq!(
            sample(&q).unwrap().file_key,
            a.file_key,
            "the middle is not read"
        );
        bytes[99_999] = 7;
        let r = write(tmp.path(), "c.bin", &bytes);
        assert_ne!(sample(&r).unwrap().file_key, a.file_key, "the tail is read");
        bytes[0] = 7;
        let t = write(tmp.path(), "d.bin", &bytes);
        assert_ne!(
            sample(&t).unwrap().file_key,
            sample(&r).unwrap().file_key,
            "the head is read"
        );
    }

    #[test]
    fn the_size_is_part_of_the_key() {
        assert_ne!(file_key(1, b"x", b"x"), file_key(2, b"x", b"x"));
    }

    #[test]
    fn an_unreadable_key_has_an_empty_hash() {
        assert_eq!(unreadable_file_key(12), "fk1:12:");
        assert!(is_unreadable_key("fk1:12:"));
        assert!(!is_unreadable_key(&file_key(0, &[], &[])));
    }

    #[test]
    fn thumb_keys_follow_the_album_the_content_and_the_time() {
        let k = thumb_key("A", "fk1:1:aa", 5);
        assert_eq!(k.len(), 40);
        assert_ne!(k, thumb_key("B", "fk1:1:aa", 5));
        assert_ne!(k, thumb_key("A", "fk1:1:ab", 5));
        assert_ne!(k, thumb_key("A", "fk1:1:aa", 6));
        assert_eq!(k, thumb_key("A", "fk1:1:aa", 5));
    }

    #[test]
    fn a_missing_file_is_an_io_error() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(sample(&tmp.path().join("nope")).is_err());
    }
}
