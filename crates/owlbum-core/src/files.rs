//! File actions (docs/06-editing-and-file-actions.md, "File actions" and
//! "Dragging photos out"): which files a command acts on, moving photos
//! to the Recycle Bin or Trash, and the drag preparation cache.
//!
//! The platform work (the confirm, the bin, the clipboard, the drag) is
//! the shell's; everything here runs with a temp album in `cargo test`.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::album::{Album, sqlite_error};
use crate::edit::{PhotoEdit, render, store};
use crate::error::{AppError, AppResult, ErrorCode};
use crate::scan::memory::Overlay;
use crate::text;
use crate::thumbs::lookup::{self, Photo};
use crate::thumbs::make;

/// Copy files, copy paths and dragging act on at most this many photos.
pub const LIMIT: usize = 500;

/// Copy image scales a larger picture down to this many pixels.
pub const MAX_COPY_PIXELS: u64 = 50_000_000;

/// How long a prepared drag is kept.
pub const DRAG_KEPT: Duration = Duration::from_secs(10);

/// One photo a file command acts on.
#[derive(Debug, Clone)]
pub struct Target {
    pub photo: Photo,
    /// The file name, for messages.
    pub name: String,
    /// The photo has a stored edit, which its file does not have.
    pub edited: bool,
}

/// The photos a file command acts on, in the order the page sent them.
#[derive(Debug, Clone, Default)]
pub struct Targets {
    pub photos: Vec<Target>,
    /// Photos with a file that were left out by the limit.
    pub dropped: usize,
    /// Photos of the album whose file is gone.
    pub missing: usize,
}

impl Targets {
    /// Any target has a stored edit.
    pub fn with_edits(&self) -> bool {
        self.photos.iter().any(|t| t.edited)
    }
}

/// Resolves `ids` to the photos of the open album whose files exist,
/// each once, in the order given, at most `limit` of them. An id that is
/// not a photo the album shows is left out; when none is, the answer is
/// `photo_not_found`. `exists` checks a file (tests pass their own).
pub fn resolve(
    album: &Album,
    overlay: &Mutex<Overlay>,
    ids: &[i64],
    limit: usize,
    exists: &dyn Fn(&Path) -> bool,
) -> AppResult<Targets> {
    let mut seen = HashSet::new();
    let mut known = 0;
    let mut out = Targets::default();
    for &id in ids {
        if !seen.insert(id) {
            continue;
        }
        let Some(photo) = lookup::by_id(album, overlay, id)? else {
            continue;
        };
        known += 1;
        if !exists(&photo.path) {
            out.missing += 1;
            continue;
        }
        if out.photos.len() >= limit {
            out.dropped += 1;
            continue;
        }
        let name = photo
            .rel_path
            .rsplit('/')
            .next()
            .unwrap_or(&photo.rel_path)
            .to_owned();
        out.photos.push(Target {
            photo,
            name,
            edited: false,
        });
    }
    if known == 0 {
        let message = if ids.len() == 1 {
            text::files::NOT_FOUND_ONE
        } else {
            text::files::NOT_FOUND_MANY
        };
        return Err(AppError::new(ErrorCode::PhotoNotFound, message));
    }
    mark_edited(album, &mut out.photos);
    Ok(out)
}

/// Resolves one id: `photo_not_found` for an unknown id and `file_missing`
/// when its file is gone.
pub fn resolve_one(
    album: &Album,
    overlay: &Mutex<Overlay>,
    id: i64,
    exists: &dyn Fn(&Path) -> bool,
) -> AppResult<Target> {
    resolve(album, overlay, &[id], 1, exists)?
        .photos
        .pop()
        .ok_or_else(|| AppError::new(ErrorCode::FileMissing, text::files::FILE_MISSING))
}

/// Sets `edited` from `photos.edit`. A database that cannot be read (a
/// damaged view-only album) leaves every photo unedited: the flag only
/// adds a note to a toast.
fn mark_edited(album: &Album, targets: &mut [Target]) {
    if targets.is_empty() {
        return;
    }
    let ids: Vec<i64> = targets.iter().map(|t| t.photo.id).collect();
    let edited: HashSet<i64> = album
        .db()
        .read(move |conn| {
            let mut st = conn
                .prepare("SELECT 1 FROM photos WHERE id = ?1 AND edit IS NOT NULL")
                .map_err(sqlite_error(text::album::READING_DATABASE))?;
            let mut found = HashSet::new();
            for id in ids {
                if st
                    .exists([id])
                    .map_err(sqlite_error(text::album::READING_DATABASE))?
                {
                    found.insert(id);
                }
            }
            Ok(found)
        })
        .unwrap_or_default();
    for t in targets {
        t.edited = edited.contains(&t.photo.id);
    }
}

/// One file that could not be moved to the bin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FileFailure {
    pub id: i64,
    /// The file name.
    pub name: String,
    /// Why, for the user.
    pub message: String,
}

/// `file_trash_ask`'s answer: the question the page shows before moving
/// photos to the bin (docs/06, "Move to the Recycle Bin or Trash"). The
/// page passes `ticket` to `file_trash` when the user agrees; the ticket
/// stands for exactly the photos asked about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TrashQuestion {
    pub ticket: u64,
    /// How many photos would move.
    pub count: usize,
    /// `Move "<name>" to the Recycle Bin?` or `Move <n> photos to the …?`.
    pub message: String,
    /// "You can restore it from the Recycle Bin." and the like.
    pub detail: String,
    /// The accepting button: "Move to Recycle Bin" or "Move to Trash".
    pub button: String,
}

/// `file_trash`'s answer.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct Trashed {
    /// The photos moved to the bin, now marked missing.
    pub trashed: Vec<i64>,
    /// The photos whose files stayed where they were.
    pub failed: Vec<FileFailure>,
    /// The ticket was not the latest question's (already used, or another
    /// question was asked since); nothing changed.
    pub cancelled: bool,
    /// The album's revision afterwards.
    pub revision: u64,
}

/// Moves the files of `targets` to the bin one at a time with `bin`, then
/// marks the rows of the moved ones `missing = 1` in one transaction
/// (docs/06, "Move to the Recycle Bin or Trash"). A file `bin` refuses
/// stays and is reported; it is never retried another way. `view_only`
/// in a view-only album, before any file is touched.
pub fn trash(
    album: &Album,
    targets: &[Target],
    bin: &dyn Fn(&Path) -> Result<(), String>,
) -> AppResult<Trashed> {
    check_writable(album)?;
    let mut out = Trashed::default();
    for t in targets {
        match bin(&t.photo.path) {
            Ok(()) => out.trashed.push(t.photo.id),
            Err(message) => out.failed.push(FileFailure {
                id: t.photo.id,
                name: t.name.clone(),
                message,
            }),
        }
    }
    out.revision = mark_missing(album, &out.trashed)?;
    Ok(out)
}

/// `view_only` when the album cannot be changed.
pub fn check_writable(album: &Album) -> AppResult<()> {
    if album.is_view_only() {
        return Err(AppError::new(ErrorCode::ViewOnly, text::files::VIEW_ONLY));
    }
    Ok(())
}

/// Marks `ids` missing in one transaction. The rows keep their typed
/// info, edit and tags, so a file restored from the bin is found again by
/// the next scan. Answers the album's revision.
pub fn mark_missing(album: &Album, ids: &[i64]) -> AppResult<u64> {
    if ids.is_empty() {
        return Ok(album.revision());
    }
    let ids = ids.to_vec();
    album.write_photos(move |tx, touched| {
        for &id in &ids {
            tx.execute("UPDATE photos SET missing = 1 WHERE id = ?1", [id])
                .map_err(sqlite_error(text::album::WRITING_DATABASE))?;
        }
        touched.photos(ids);
        Ok(())
    })?;
    Ok(album.next_revision())
}

/// Why a file cannot go to the bin on this drive, checked before asking
/// the system: a drive with no bin must never turn a move into a
/// permanent delete (docs/06, "A drive with no bin"). On Windows only
/// local fixed drives have a Recycle Bin; network shares and removable
/// drives do not. On Linux the trash library itself refuses a mount with
/// no trash folder.
pub fn no_bin_reason(path: &Path) -> Option<String> {
    (!has_bin(path)).then(text::files::no_bin)
}

#[cfg(windows)]
fn has_bin(path: &Path) -> bool {
    use std::path::{Component, Prefix};
    match path.components().next() {
        Some(Component::Prefix(p)) => match p.kind() {
            Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => drive_is_fixed(letter),
            _ => false,
        },
        _ => false,
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn drive_is_fixed(letter: u8) -> bool {
    use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
    /// `GetDriveTypeW` answers this for a local hard drive or SSD.
    const DRIVE_FIXED: u32 = 3;
    let root: Vec<u16> = format!("{}:\\", char::from(letter))
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: `root` is a NUL-terminated wide string that outlives the
    // call; nothing is kept.
    unsafe { GetDriveTypeW(root.as_ptr()) == DRIVE_FIXED }
}

#[cfg(not(windows))]
fn has_bin(_path: &Path) -> bool {
    true
}

/// What the clipboard is given by copy files: the files themselves, or
/// their paths as text where the platform cannot put files there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CopyAs {
    Files,
    Paths,
}

/// What this platform offers (`app_capabilities`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Capabilities {
    /// Photos can be dragged out to other programs.
    pub drag_out: bool,
    /// What copying several photos puts on the clipboard.
    pub copy_files: CopyAs,
}

impl Capabilities {
    /// This build's platform. Linux work is paused: copying several photos
    /// copies their paths there, and dragging out is not offered, until the
    /// Linux prototype (docs/06, "Copy several photos" and "Dragging
    /// photos out") is done.
    pub const fn current() -> Self {
        if cfg!(windows) {
            Self {
                drag_out: true,
                copy_files: CopyAs::Files,
            }
        } else {
            Self {
                drag_out: false,
                copy_files: CopyAs::Paths,
            }
        }
    }
}

/// `file_copy_image`'s answer. The picture is drawn with the photo's
/// stored edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CopiedImage {
    /// The picture was made smaller to fit the clipboard.
    pub scaled: bool,
    /// The photo is a video, so its file was copied instead.
    pub file: bool,
}

/// `file_copy_files`'s answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CopiedFiles {
    pub copied: usize,
    /// Photos left out by the limit.
    pub dropped: usize,
    /// What was put on the clipboard.
    #[serde(rename = "as")]
    pub copied_as: CopyAs,
    /// Some copied photo has a stored edit its file does not have.
    pub with_edits: bool,
}

/// `file_copy_paths`'s answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CopiedPaths {
    pub copied: usize,
    pub dropped: usize,
}

/// `file_start_drag`'s answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Dragged {
    /// How many files were offered.
    pub dragged: usize,
    /// Some dragged photo has a stored edit its file does not have.
    pub with_edits: bool,
}

/// The size of a picture scaled down to at most [`MAX_COPY_PIXELS`],
/// keeping its shape; None when it already fits.
pub fn fit_pixels(width: u32, height: u32, max: u64) -> Option<(u32, u32)> {
    let area = u64::from(width) * u64::from(height);
    if area <= max || area == 0 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let scale = (max as f64 / area as f64).sqrt();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let w = ((f64::from(width) * scale).floor() as u32).max(1);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let h = ((f64::from(height) * scale).floor() as u32).max(1);
    Some((w, h))
}

/// The paths of `targets`, one per line, for the clipboard.
pub fn path_lines(targets: &[Target]) -> String {
    targets
        .iter()
        .map(|t| t.photo.path.display().to_string())
        .collect::<Vec<_>>()
        .join(if cfg!(windows) { "\r\n" } else { "\n" })
}

/// A picture for the clipboard: upright, 8-bit sRGB RGBA.
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// It was made smaller to fit [`MAX_COPY_PIXELS`].
    pub scaled: bool,
}

/// The stored edit of photo `id`, if it has a readable one. A database
/// that cannot be read counts as no edit.
pub fn stored_edit(album: &Album, id: i64) -> Option<PhotoEdit> {
    let text: Option<String> = album
        .db()
        .read(move |conn| {
            conn.query_row("SELECT edit FROM photos WHERE id = ?1", [id], |r| r.get(0))
                .optional()
                .map(Option::flatten)
                .map_err(sqlite_error(text::album::READING_DATABASE))
        })
        .ok()
        .flatten();
    store::read_stored(id, text.as_deref())
}

/// The picture of a still photo for copy image (docs/06, "Copy image (one
/// photo)"): its full-size working pixels drawn through `edit` (the
/// photo's stored edit, if any) by the edit renderer, then scaled down to
/// at most [`MAX_COPY_PIXELS`]. `internal` with "Couldn't read that
/// photo." when it cannot be decoded.
pub fn picture(target: &Target, edit: Option<&PhotoEdit>) -> AppResult<Picture> {
    let read_failed = || AppError::new(ErrorCode::Internal, text::files::READ_FAILED);
    let format = target.photo.format.ok_or_else(read_failed)?;
    let px = format
        .handler
        .working(&target.photo.path)
        .map_err(|_| read_failed())?;
    let px = match edit {
        Some(edit) => render::render_pixels(&px, edit).1,
        None => px,
    };
    let img = image::RgbaImage::from_raw(px.width, px.height, px.rgba).ok_or_else(read_failed)?;
    Ok(
        match fit_pixels(img.width(), img.height(), MAX_COPY_PIXELS) {
            None => Picture {
                width: img.width(),
                height: img.height(),
                rgba: img.into_raw(),
                scaled: false,
            },
            Some((w, h)) => Picture {
                width: w,
                height: h,
                rgba: make::lanczos(&img, w, h).into_raw(),
                scaled: true,
            },
        },
    )
}

/// The longest side of the drag icon.
pub const DRAG_ICON_SIDE: u32 = 128;

/// The side of the plain square used when there is no thumbnail.
pub const DRAG_PLACEHOLDER_SIDE: u32 = 48;

/// The placeholder's colour, token `--drag-placeholder`
/// (docs/09-visual-design.md).
pub const DRAG_PLACEHOLDER: [u8; 4] = [0x2d, 0x2d, 0x32, 0xff];

/// The drag icon as PNG bytes: `small` (a small thumbnail's JPEG bytes)
/// scaled to [`DRAG_ICON_SIDE`] on its longest side, never enlarged, or
/// the plain placeholder square when there is none or it cannot be read.
pub fn drag_icon(small: Option<&[u8]>) -> Vec<u8> {
    let icon = small
        .and_then(|bytes| image::load_from_memory(bytes).ok())
        .map(|img| make::fit(img.to_rgba8(), DRAG_ICON_SIDE))
        .unwrap_or_else(|| {
            image::RgbaImage::from_pixel(
                DRAG_PLACEHOLDER_SIDE,
                DRAG_PLACEHOLDER_SIDE,
                image::Rgba(DRAG_PLACEHOLDER),
            )
        });
    let mut out = std::io::Cursor::new(Vec::new());
    // Writing a PNG to memory does not fail.
    let _ = icon.write_to(&mut out, image::ImageFormat::Png);
    out.into_inner()
}

/// A drag prepared on pointer-down, kept for the same ids in the same
/// order for [`DRAG_KEPT`] (docs/06, "Starting the drag").
pub struct DragCache<T> {
    held: Mutex<Option<(Vec<i64>, Instant, T)>>,
    kept: Duration,
}

impl<T: Clone> Default for DragCache<T> {
    fn default() -> Self {
        Self::with_kept(DRAG_KEPT)
    }
}

impl<T: Clone> DragCache<T> {
    /// A cache that keeps a preparation for `kept`.
    pub fn with_kept(kept: Duration) -> Self {
        Self {
            held: Mutex::new(None),
            kept,
        }
    }

    /// Keeps `value` as the preparation of `ids`, made at `now`.
    pub fn put(&self, ids: Vec<i64>, now: Instant, value: T) {
        *self.held.lock().unwrap_or_else(|p| p.into_inner()) = Some((ids, now, value));
    }

    /// The preparation of exactly `ids`, when it is at most `kept` old at
    /// `now`. It is used once.
    pub fn take(&self, ids: &[i64], now: Instant) -> Option<T> {
        let mut held = self.held.lock().unwrap_or_else(|p| p.into_inner());
        match held.as_ref() {
            Some((kept_ids, at, value))
                if kept_ids.as_slice() == ids
                    && now.saturating_duration_since(*at) <= self.kept =>
            {
                let value = value.clone();
                *held = None;
                Some(value)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
