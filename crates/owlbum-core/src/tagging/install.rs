//! Model files on disk (docs/07-tagging.md, "Model files on disk"): the
//! install state of a model, the `verified.json` marker, hashing, and
//! removal.
//!
//! A model's files are `<models>/<id>/<file.path>`; a partial download is
//! `<file.path>.part`. Working out the state reads sizes and times only;
//! a file becomes trusted only after its SHA-256 matched, and the marker
//! remembers that until the file's size or time changes.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use super::catalogue::{ModelChoice, ModelFile};

/// The marker's file name inside a model's folder.
pub const MARKER: &str = "verified.json";

/// Where a model is in its life on this computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum InstallState {
    /// Nothing of it is here.
    Absent,
    /// Some files are missing, or a download was left part way.
    Partial,
    /// Every file is here at the right size, but not all are vouched for.
    Unverified,
    /// An unverified model is being hashed.
    Checking,
    /// Every file is here and matched its SHA-256. Only this can be chosen.
    Ok,
    /// A file has the wrong size, or failed its SHA-256 check.
    Damaged,
}

/// One file's entry in the marker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub bytes: u64,
    pub mtime_ms: i64,
    pub sha256: String,
}

/// `verified.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    pub revision: String,
    pub files: BTreeMap<String, Entry>,
}

/// A model's folder.
pub fn model_dir(models: &Path, model: &ModelChoice) -> PathBuf {
    models.join(model.id)
}

/// A file's place inside a model's folder.
pub fn file_path(dir: &Path, file: &ModelFile) -> PathBuf {
    dir.join(file.path)
}

/// The `.part` file of a download of `file`.
pub fn part_path(dir: &Path, file: &ModelFile) -> PathBuf {
    let mut p = file_path(dir, file).into_os_string();
    p.push(".part");
    PathBuf::from(p)
}

fn mtime_ms(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// Reads the marker; an empty one when there is none or it cannot be read.
pub fn read_marker(dir: &Path) -> Marker {
    std::fs::read(dir.join(MARKER))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Writes the marker with a temporary file and a rename.
pub fn write_marker(dir: &Path, marker: &Marker) -> std::io::Result<()> {
    let tmp = dir.join(format!("{MARKER}.tmp"));
    let bytes = serde_json::to_vec_pretty(marker).map_err(std::io::Error::other)?;
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, dir.join(MARKER))
}

/// Records that `file`, as it now is on disk, matched its SHA-256.
pub fn vouch(dir: &Path, model: &ModelChoice, file: &ModelFile) -> std::io::Result<()> {
    let meta = std::fs::metadata(file_path(dir, file))?;
    let mut marker = read_marker(dir);
    if marker.revision != model.revision {
        marker = Marker {
            revision: model.revision.to_owned(),
            files: BTreeMap::new(),
        };
    }
    marker.files.insert(
        file.path.to_owned(),
        Entry {
            bytes: meta.len(),
            mtime_ms: mtime_ms(&meta),
            sha256: file.sha256.to_owned(),
        },
    );
    write_marker(dir, &marker)
}

/// Forgets a file's entry.
fn unvouch(dir: &Path, file: &ModelFile) {
    let mut marker = read_marker(dir);
    if marker.files.remove(file.path).is_some() {
        let _ = write_marker(dir, &marker);
    }
}

/// True when the marker vouches for `file` as it is on disk.
fn vouched(marker: &Marker, model: &ModelChoice, file: &ModelFile, meta: &std::fs::Metadata) -> bool {
    marker.revision == model.revision
        && marker.files.get(file.path).is_some_and(|e| {
            e.bytes == meta.len() && e.mtime_ms == mtime_ms(meta) && e.sha256 == file.sha256
        })
}

/// The state from sizes, times and the marker alone: never `checking`, and
/// `damaged` only for a file of the wrong size.
pub fn quick_state(dir: &Path, model: &ModelChoice) -> InstallState {
    if !dir.is_dir() {
        return InstallState::Absent;
    }
    let marker = read_marker(dir);
    let (mut present, mut missing, mut parts, mut unvouched) = (0, 0, 0, 0);
    for f in model.files {
        if part_path(dir, f).is_file() {
            parts += 1;
        }
        match std::fs::metadata(file_path(dir, f)) {
            Ok(meta) if meta.is_file() => {
                if meta.len() != f.bytes {
                    return InstallState::Damaged;
                }
                present += 1;
                if !vouched(&marker, model, f, &meta) {
                    unvouched += 1;
                }
            }
            _ => missing += 1,
        }
    }
    if present == 0 && parts == 0 {
        InstallState::Absent
    } else if missing > 0 || parts > 0 {
        InstallState::Partial
    } else if unvouched > 0 {
        InstallState::Unverified
    } else {
        InstallState::Ok
    }
}

/// The bytes of the model already on disk: whole files and `.part` files.
pub fn bytes_present(dir: &Path, model: &ModelChoice) -> u64 {
    model
        .files
        .iter()
        .map(|f| {
            let whole = std::fs::metadata(file_path(dir, f)).map_or(0, |m| m.len());
            let part = std::fs::metadata(part_path(dir, f)).map_or(0, |m| m.len());
            whole.min(f.bytes) + part.min(f.bytes)
        })
        .sum()
}

/// The SHA-256 of a file, as lower-case hex.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

/// Lower-case hex.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Hashes the files the marker does not vouch for (every present file when
/// `all`), deletes those of the wrong size or hash and forgets their
/// entries, and vouches for the rest. Answers `ok` or `damaged` when every
/// file was present, else what [`quick_state`] now says.
pub fn verify(dir: &Path, model: &ModelChoice, all: bool) -> InstallState {
    let marker = read_marker(dir);
    let mut failed = false;
    for f in model.files {
        let p = file_path(dir, f);
        let Ok(meta) = std::fs::metadata(&p) else {
            continue;
        };
        if !all && vouched(&marker, model, f, &meta) {
            continue;
        }
        let good = meta.len() == f.bytes && sha256_file(&p).is_ok_and(|h| h == f.sha256);
        if good {
            let _ = vouch(dir, model, f);
        } else {
            failed = true;
            let _ = std::fs::remove_file(&p);
            unvouch(dir, f);
            tracing::warn!(target: "tagging", "{}: {} failed its check and was deleted", model.id, f.path);
        }
    }
    match (failed, quick_state(dir, model)) {
        (true, _) => InstallState::Damaged,
        (false, s) => s,
    }
}

/// Deletes a model's folder with everything in it.
pub fn remove(dir: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::super::fake;
    use super::*;

    fn model() -> &'static ModelChoice {
        &fake::FAKE_MODELS[0]
    }

    #[test]
    fn each_state() {
        let tmp = tempfile::tempdir().unwrap();
        let models = tmp.path();
        let m = model();
        let dir = model_dir(models, m);
        assert_eq!(quick_state(&dir, m), InstallState::Absent);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(quick_state(&dir, m), InstallState::Absent);
        std::fs::write(part_path(&dir, &m.files[0]), b"fake").unwrap();
        assert_eq!(quick_state(&dir, m), InstallState::Partial);
        assert_eq!(bytes_present(&dir, m), 4);
        std::fs::remove_file(part_path(&dir, &m.files[0])).unwrap();
        fake::install(models, m);
        assert_eq!(quick_state(&dir, m), InstallState::Unverified);
        assert_eq!(bytes_present(&dir, m), m.bytes());
        assert_eq!(verify(&dir, m, false), InstallState::Ok);
        assert_eq!(quick_state(&dir, m), InstallState::Ok);
        // A file of the wrong size.
        std::fs::write(file_path(&dir, &m.files[1]), b"short").unwrap();
        assert_eq!(quick_state(&dir, m), InstallState::Damaged);
        assert_eq!(verify(&dir, m, false), InstallState::Damaged);
        // The bad file was deleted, so only it is missing now.
        assert!(!file_path(&dir, &m.files[1]).exists());
        assert_eq!(quick_state(&dir, m), InstallState::Partial);
    }

    #[test]
    fn a_file_changed_in_place_is_caught_by_a_full_verify() {
        let tmp = tempfile::tempdir().unwrap();
        let m = model();
        fake::install(tmp.path(), m);
        let dir = model_dir(tmp.path(), m);
        assert_eq!(verify(&dir, m, false), InstallState::Ok);
        // Same size, same time, other bytes: the marker still vouches.
        let p = file_path(&dir, &m.files[0]);
        let meta = std::fs::metadata(&p).unwrap();
        let mut bytes = std::fs::read(&p).unwrap();
        bytes[0] ^= 1;
        std::fs::write(&p, &bytes).unwrap();
        let f = std::fs::File::options().write(true).open(&p).unwrap();
        f.set_modified(meta.modified().unwrap()).unwrap();
        drop(f);
        assert_eq!(quick_state(&dir, m), InstallState::Ok);
        assert_eq!(verify(&dir, m, true), InstallState::Damaged);
        assert!(!p.exists());
    }

    #[test]
    fn a_marker_of_another_revision_vouches_for_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let m = model();
        fake::install(tmp.path(), m);
        let dir = model_dir(tmp.path(), m);
        verify(&dir, m, false);
        let mut marker = read_marker(&dir);
        marker.revision = "x".into();
        write_marker(&dir, &marker).unwrap();
        assert_eq!(quick_state(&dir, m), InstallState::Unverified);
        remove(&dir).unwrap();
        assert_eq!(quick_state(&dir, m), InstallState::Absent);
        remove(&dir).unwrap();
    }
}
