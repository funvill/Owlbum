//! Saving edits into files and as copies (docs/06-editing-and-file-actions.md,
//! "Saving"): `edit_status`, `edit_save_to_file`, `edit_save_all`,
//! `edit_save_as`, the save record and its recovery on open.
//!
//! Principle 2 holds throughout: a photo file is written only here, only
//! when the user asked, and the file a save replaces always goes to the
//! Recycle Bin or Trash first. A save record in `meta` (`save:<id>`) guards
//! the moment between replacing the file and clearing its edit, so an edit
//! is never drawn a second time on a file that already holds it.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::model::{PhotoEdit, same_edit, stored_value};
use super::render::render_edit;
use super::store::{not_editable, read_stored};
use crate::album::{Album, sqlite_error};
use crate::appdata::settings::EditTarget;
use crate::error::{AppError, AppResult, ErrorCode};
use crate::files::FileFailure;
use crate::media::{MediaFormat, SaveFormat, format_by_id};
use crate::scan::{keys, walk};
use crate::text;

/// The SQL for a photo's edit as it is drawn: none while the photo has a
/// save record, so a replaced file is never drawn with its edit a second
/// time. Readers select it in place of `photos.edit` (the table must be
/// named `photos`). A macro, so readers can `concat!` it into their SQL.
macro_rules! drawn_edit {
    () => {
        "CASE WHEN EXISTS (SELECT 1 FROM meta WHERE meta.key = 'save:' || photos.id) THEN NULL ELSE photos.edit END"
    };
}
pub(crate) use drawn_edit;

/// [`drawn_edit!`] as a constant.
pub const DRAWN_EDIT: &str = drawn_edit!();

/// How long a render left in `renders/` is kept.
pub const RENDER_KEPT: Duration = Duration::from_secs(60 * 60);

/// The busy retries of a move: attempts and the wait between them.
pub const MOVE_TRIES: u32 = 3;
pub const MOVE_WAIT: Duration = Duration::from_millis(200);

// ---------------------------------------------------------------------------
// Claims
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct Claimed {
    /// Photos being saved.
    ids: HashSet<i64>,
    /// Their paths, which the scanner and the folder watcher skip.
    paths: HashSet<String>,
}

/// The saves running in one album: which photos are busy, which paths the
/// scanner skips, and the stop flag Save all reads between photos.
#[derive(Debug, Default)]
pub struct Saves {
    claimed: Mutex<Claimed>,
    idle: Condvar,
    stop: AtomicBool,
}

impl Saves {
    fn lock(&self) -> MutexGuard<'_, Claimed> {
        self.claimed.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Marks photo `id` at `rel_path` busy; None when it already is.
    fn claim(&self, id: i64, rel_path: &str) -> Option<Claim<'_>> {
        let mut c = self.lock();
        if !c.ids.insert(id) {
            return None;
        }
        c.paths.insert(rel_path.to_owned());
        Some(Claim {
            saves: self,
            id,
            rel_path: rel_path.to_owned(),
        })
    }

    /// Whether the scanner skips `rel_path` now.
    pub fn skips(&self, rel_path: &str) -> bool {
        let c = self.lock();
        !c.paths.is_empty() && c.paths.contains(rel_path)
    }

    /// Whether photo `id` is being saved.
    pub fn is_busy(&self, id: i64) -> bool {
        self.lock().ids.contains(&id)
    }

    /// How many photos are being saved now.
    pub fn count(&self) -> u64 {
        self.lock().ids.len() as u64
    }

    /// Asks Save all to stop after the photo it is saving (the album is
    /// closing or the program quits).
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    /// Lets saves run again after a close was called off.
    pub fn resume(&self) {
        self.stop.store(false, Ordering::SeqCst);
    }

    /// Whether [`Self::stop`] was asked.
    pub fn stopping(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }

    /// Waits until no photo is being saved, at most `limit`. True when none
    /// is.
    pub fn wait_idle(&self, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        let mut c = self.lock();
        while !c.ids.is_empty() {
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            c = self
                .idle
                .wait_timeout(c, deadline - now)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        true
    }
}

/// One photo's claim; released when dropped.
struct Claim<'a> {
    saves: &'a Saves,
    id: i64,
    rel_path: String,
}

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        let mut c = self.saves.lock();
        c.ids.remove(&self.id);
        c.paths.remove(&self.rel_path);
        if c.ids.is_empty() {
            self.saves.idle.notify_all();
        }
    }
}

// ---------------------------------------------------------------------------
// Replies
// ---------------------------------------------------------------------------

/// `edit_status`'s answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct EditStatus {
    /// Photos with a stored edit.
    pub edited: u64,
    /// Photos waiting to be saved: `edited` with `edit_target = file`, else 0.
    pub waiting: u64,
    /// Photos being saved now.
    pub saving: u64,
}

/// `edit_save_to_file`'s and `edit_save_all`'s answer.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct SavedEdits {
    /// The photos saved.
    pub saved: Vec<i64>,
    /// The photos that failed, each with its message.
    pub failed: Vec<FileFailure>,
    /// Photos left out because they can only be saved as a copy.
    pub skipped: u32,
    /// Saved, with something to say: "A later change to <name> was not
    /// saved".
    pub warnings: Vec<String>,
    /// The album's revision after the saves.
    pub revision: u64,
}

/// `edit_save_as`'s answer.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct SavedAs {
    /// The dialog was cancelled; nothing happened.
    pub cancelled: bool,
    /// The written file's name.
    pub saved_name: Option<String>,
    /// The copy is inside the album and has its own row.
    pub in_album: bool,
    /// The copy's row, when it has one.
    pub photo_id: Option<i64>,
    /// Saved, but partly: the copy could not be read, or its typed info not
    /// copied.
    pub warning: Option<String>,
    /// The album's revision after the save.
    pub revision: u64,
}

/// `edit_save_progress`: sent as each photo of a save of several starts,
/// and once at the end.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SaveProgress {
    /// Photos finished.
    pub done: u32,
    pub total: u32,
    /// The file name being saved; null at the end.
    pub current: Option<String>,
}

impl crate::events::Event for SaveProgress {
    const NAME: &'static str = "edit_save_progress";
}

// ---------------------------------------------------------------------------
// What a save needs from outside
// ---------------------------------------------------------------------------

/// Renders an edit into a file: [`render_edit`] normally.
pub type RenderFn<'a> = &'a (
        dyn Fn(&Path, &MediaFormat, &PhotoEdit, SaveFormat, &Path) -> AppResult<(u32, u32)> + Sync
    );

/// What saving needs from the shell. Tests give stand-ins.
pub struct SaveEnv<'a> {
    /// `<app data>/renders`.
    pub renders: PathBuf,
    /// Moves one file to the Recycle Bin or Trash; a reason when it cannot.
    pub bin: &'a (dyn Fn(&Path) -> Result<(), String> + Sync),
    /// Renames a file: [`fs::rename`] normally.
    pub rename: &'a (dyn Fn(&Path, &Path) -> io::Result<()> + Sync),
    /// The wait between busy retries of a move.
    pub retry_wait: Duration,
    pub render: RenderFn<'a>,
}

fn real_rename(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to)
}

fn real_render(
    source: &Path,
    format: &MediaFormat,
    edit: &PhotoEdit,
    target: SaveFormat,
    out: &Path,
) -> AppResult<(u32, u32)> {
    render_edit(source, format, edit, target, out)
}

impl<'a> SaveEnv<'a> {
    /// The real renames and renderer, with the bin given.
    pub fn new(renders: PathBuf, bin: &'a (dyn Fn(&Path) -> Result<(), String> + Sync)) -> Self {
        Self {
            renders,
            bin,
            rename: &real_rename,
            retry_wait: MOVE_WAIT,
            render: &real_render,
        }
    }
}

// ---------------------------------------------------------------------------
// The move rules
// ---------------------------------------------------------------------------

/// The system reports the file busy, in use, or access denied.
fn is_busy(e: &io::Error) -> bool {
    if matches!(
        e.kind(),
        io::ErrorKind::PermissionDenied | io::ErrorKind::ResourceBusy
    ) {
        return true;
    }
    // Windows: sharing and lock violations. Unix: EBUSY, ETXTBSY.
    let busy: &[i32] = if cfg!(windows) {
        &[5, 32, 33]
    } else {
        &[16, 26]
    };
    e.raw_os_error().is_some_and(|c| busy.contains(&c))
}

/// A rename across drives was refused.
fn crosses_drives(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::CrossesDevices
        || e.raw_os_error() == Some(if cfg!(windows) { 17 } else { 18 })
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

/// Copies `from` to `<to>.<pid>-<ms>.part`, flushes it, renames it onto
/// `to` and deletes `from`. The `.part` file is removed on failure.
fn copy_across(
    from: &Path,
    to: &Path,
    rename: &dyn Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    let mut part = to.as_os_str().to_owned();
    part.push(format!(".{}-{}.part", std::process::id(), now_ms()));
    let part = PathBuf::from(part);
    let result = (|| {
        fs::copy(from, &part)?;
        fs::OpenOptions::new().write(true).open(&part)?.sync_all()?;
        rename(&part, to)
    })();
    match result {
        Ok(()) => {
            let _ = fs::remove_file(from);
            Ok(())
        }
        Err(e) => {
            let _ = fs::remove_file(&part);
            Err(e)
        }
    }
}

/// Moves `from` onto `to` by the move rules: a rename; up to three
/// attempts when the target is busy; a copy and rename across drives.
pub fn move_into_place(from: &Path, to: &Path, env: &SaveEnv<'_>) -> io::Result<()> {
    let mut attempt = 1;
    loop {
        match (env.rename)(from, to) {
            Ok(()) => return Ok(()),
            Err(e) if crosses_drives(&e) => return copy_across(from, to, env.rename),
            Err(e) if is_busy(&e) && attempt < MOVE_TRIES => {
                attempt += 1;
                std::thread::sleep(env.retry_wait);
            }
            Err(e) => return Err(e),
        }
    }
}

// ---------------------------------------------------------------------------
// Reading what a save needs
// ---------------------------------------------------------------------------

/// One photo's row, as saving reads it.
struct Photo {
    id: i64,
    rel_path: String,
    name: String,
    format: &'static MediaFormat,
    edit: Option<PhotoEdit>,
    file_key: String,
    not_editable: Option<&'static str>,
}

fn gone() -> AppError {
    AppError::new(ErrorCode::PhotoNotFound, text::edit::SAVE_GONE)
}

fn load(album: &Album, id: i64) -> AppResult<Photo> {
    let row = album.db().read(move |conn| {
        conn.query_row(
            "SELECT rel_path, file_name, format, kind, duration_ms, edit, file_key
             FROM photos WHERE id = ?1 AND missing = 0",
            [id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<i64>>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, String>(6)?,
                ))
            },
        )
        .optional()
        .map_err(sqlite_error(text::edit::READING))
    })?;
    let (rel_path, name, format, kind, duration, edit, file_key) = row.ok_or_else(gone)?;
    let format = format_by_id(&format).ok_or_else(gone)?;
    Ok(Photo {
        id,
        rel_path,
        name,
        format,
        edit: read_stored(id, edit.as_deref()).filter(|e| stored_value(e).is_some()),
        file_key,
        not_editable: not_editable(&kind, duration),
    })
}

fn absolute(album: &Album, rel_path: &str) -> PathBuf {
    walk::absolute(&album.location().root, rel_path)
}

/// The default extension of a written format, with the dot, from the
/// format registry.
fn extension_of(format: SaveFormat) -> &'static str {
    registry_of(format).map_or("", |r| r.extensions[0])
}

/// The registry entry of a written format.
fn registry_of(format: SaveFormat) -> Option<&'static MediaFormat> {
    format_by_id(match format {
        SaveFormat::Jpeg => "jpeg",
        SaveFormat::Png => "png",
        SaveFormat::Webp => "webp",
    })
}

/// A fresh render path in `renders/`.
fn render_path(env: &SaveEnv<'_>, format: SaveFormat) -> AppResult<PathBuf> {
    fs::create_dir_all(&env.renders).map_err(|e| {
        AppError::new(
            ErrorCode::EditRenderFailed,
            text::edit::render_failed(e.to_string()),
        )
    })?;
    Ok(env.renders.join(format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        extension_of(format)
    )))
}

// ---------------------------------------------------------------------------
// The save record
// ---------------------------------------------------------------------------

/// A save record's value (docs/06, "The save record and recovery").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveRecord {
    /// The edit being saved, in its stored form.
    pub edit: serde_json::Value,
    /// The original's file key.
    pub file_key: String,
    /// The render's file name in `renders/`.
    pub render: String,
    /// When the save started, ms since the epoch.
    pub started_at: i64,
}

fn record_key(id: i64) -> String {
    format!("save:{id}")
}

fn write_record(album: &Album, id: i64, record: &SaveRecord) -> AppResult<()> {
    let value = serde_json::to_string(record).unwrap_or_default();
    let key = record_key(id);
    album.db().write(move |tx| {
        tx.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
            params![key, value],
        )
        .map_err(sqlite_error(text::album::WRITING_DATABASE))?;
        Ok(())
    })
}

fn delete_record(album: &Album, id: i64) -> AppResult<()> {
    let key = record_key(id);
    album.db().write(move |tx| {
        tx.execute("DELETE FROM meta WHERE key = ?1", [key])
            .map_err(sqlite_error(text::album::WRITING_DATABASE))?;
        Ok(())
    })
}

/// Every save record in the album, by photo id.
pub fn records(album: &Album) -> AppResult<Vec<(i64, SaveRecord)>> {
    album.db().read(|conn| {
        let failed = sqlite_error(text::edit::READING);
        let mut st = conn
            .prepare("SELECT key, value FROM meta WHERE key LIKE 'save:%'")
            .map_err(&failed)?;
        let rows = st
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(&failed)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(&failed)?;
        Ok(rows
            .into_iter()
            .filter_map(|(k, v)| {
                let id = k.strip_prefix("save:")?.parse().ok()?;
                Some((id, serde_json::from_str(&v).ok()?))
            })
            .collect())
    })
}

/// Step 7: refreshes the row from the file now at its path and clears the
/// edit when it still equals `saved`, and deletes the save record, in one
/// transaction. `size` is the written picture's size when known; without
/// it the size is kept and the pixels pass corrects it. Answers whether
/// the edit had changed since the save began (and so was kept).
fn finish(
    album: &Album,
    id: i64,
    path: &Path,
    saved: &PhotoEdit,
    size: Option<(u32, u32)>,
) -> AppResult<bool> {
    let not_read = |e: io::Error| {
        AppError::new(
            ErrorCode::EditSavedNotRead,
            text::edit::saved_not_read(&e.to_string()),
        )
    };
    let meta = fs::metadata(path).map_err(not_read)?;
    let sample = keys::sample(path).map_err(not_read)?;
    let mtime = walk::mtime_ms(&meta);
    let thumb_key = keys::thumb_key(album.db().album_id(), &sample.file_key, mtime);
    let saved = *saved;
    let key = record_key(id);
    album.write_photos(move |tx, touched| {
        let failed = sqlite_error(text::album::WRITING_DATABASE);
        let edit: Option<String> = tx
            .query_row("SELECT edit FROM photos WHERE id = ?1", [id], |r| r.get(0))
            .optional()
            .map_err(&failed)?
            .flatten();
        let now = read_stored(id, edit.as_deref());
        let later = now.is_some_and(|e| !same_edit(&e, &saved));
        let size_i64 = i64::try_from(sample.size).unwrap_or(i64::MAX);
        tx.execute(
            "UPDATE photos SET size = ?2, mtime_ms = ?3, file_key = ?4, thumb_key = ?5,
                    analysis = NULL, scan_error = NULL, missing = 0,
                    width = coalesce(?6, width), height = coalesce(?7, height),
                    edit = CASE WHEN ?8 THEN edit ELSE NULL END
             WHERE id = ?1",
            params![
                id,
                size_i64,
                mtime,
                sample.file_key,
                thumb_key,
                size.map(|s| s.0),
                size.map(|s| s.1),
                later,
            ],
        )
        .map_err(&failed)?;
        tx.execute("DELETE FROM meta WHERE key = ?1", [&key])
            .map_err(&failed)?;
        touched.photo(id);
        Ok(later)
    })
}

// ---------------------------------------------------------------------------
// Save into the file
// ---------------------------------------------------------------------------

fn check_writable(album: &Album) -> AppResult<()> {
    if album.is_view_only() {
        return Err(AppError::new(ErrorCode::ViewOnly, text::edit::VIEW_ONLY));
    }
    Ok(())
}

/// Saves one photo's edit into its file: the whole sequence. Answers
/// whether a later change was left unsaved (step 7).
pub fn save_one(album: &Album, env: &SaveEnv<'_>, id: i64) -> AppResult<bool> {
    // 1. Check.
    check_writable(album)?;
    let photo = load(album, id)?;
    if let Some(reason) = photo.not_editable {
        return Err(AppError::new(ErrorCode::EditNotEditable, reason));
    }
    let Some(edit) = photo.edit else {
        return Err(AppError::new(
            ErrorCode::EditNone,
            text::edit::save_none(&photo.name),
        ));
    };
    if !photo.format.caps.edit_in_place {
        return Err(AppError::new(
            ErrorCode::EditCopyOnly,
            text::edit::copy_only(photo.format.label),
        ));
    }
    // 2. Claim.
    let Some(claim) = album.saves().claim(photo.id, &photo.rel_path) else {
        return Err(AppError::new(ErrorCode::EditBusy, text::edit::SAVE_BUSY));
    };
    let path = absolute(album, &photo.rel_path);

    // 3. Render.
    let target = photo.format.caps.save_as;
    let render = render_path(env, target)?;
    let size = (env.render)(&path, photo.format, &edit, target, &render).inspect_err(|_| {
        let _ = fs::remove_file(&render);
    })?;

    // 4. Record.
    let record = SaveRecord {
        edit: stored_value(&edit)
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
        file_key: photo.file_key.clone(),
        render: render
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        started_at: i64::try_from(now_ms()).unwrap_or(0),
    };
    if let Err(e) = write_record(album, id, &record) {
        let _ = fs::remove_file(&render);
        return Err(
            AppError::new(e.code, text::edit::save_not_recorded(&photo.name))
                .with_details(e.details.unwrap_or_default()),
        );
    }

    // 5. Move the original to the bin.
    if let Err(reason) = (env.bin)(&path) {
        let _ = fs::remove_file(&render);
        let _ = delete_record(album, id);
        return Err(AppError::new(
            ErrorCode::EditOriginalNotMoved,
            text::edit::original_not_moved(&reason),
        ));
    }

    // 6. Move the render into place. On failure the render and the record
    // stay, so the next open finishes the job.
    if let Err(e) = move_into_place(&render, &path, env) {
        tracing::warn!(target: "edit", "save: the render of photo {id} could not be put in place: {e}");
        return Err(AppError::new(
            ErrorCode::EditNotPlaced,
            text::edit::not_placed(&photo.name, &e.to_string()),
        ));
    }

    // 7. Finish. A failure keeps the record; the next open finishes the row.
    let later = finish(album, id, &path, &edit, Some(size))?;

    // 8. Release, and drop this photo's edits from undo.
    drop(claim);
    album.undo().drop_edits_of(&HashSet::from([id]));
    tracing::info!(target: "edit", "save: photo {id} saved into its file");
    Ok(later)
}

/// `edit_save_to_file(photo_ids)`: saves each photo, one after another.
/// `progress` hears each photo start, and the end, when there is more than
/// one.
pub fn save_to_file(
    album: &Album,
    env: &SaveEnv<'_>,
    ids: &[i64],
    progress: &dyn Fn(SaveProgress),
) -> AppResult<SavedEdits> {
    check_writable(album)?;
    let mut seen = HashSet::new();
    let ids: Vec<i64> = ids.iter().copied().filter(|id| seen.insert(*id)).collect();
    save_many(album, env, &ids, 0, progress)
}

fn save_many(
    album: &Album,
    env: &SaveEnv<'_>,
    ids: &[i64],
    skipped: u32,
    progress: &dyn Fn(SaveProgress),
) -> AppResult<SavedEdits> {
    let mut out = SavedEdits {
        skipped,
        ..SavedEdits::default()
    };
    let total = u32::try_from(ids.len()).unwrap_or(u32::MAX);
    let several = ids.len() > 1;
    for (done, &id) in ids.iter().enumerate() {
        if album.saves().stopping() {
            break;
        }
        let name = load(album, id).map(|p| p.name).unwrap_or_default();
        if several {
            progress(SaveProgress {
                done: u32::try_from(done).unwrap_or(u32::MAX),
                total,
                current: Some(name.clone()),
            });
        }
        match save_one(album, env, id) {
            Ok(later) => {
                out.saved.push(id);
                if later {
                    out.warnings.push(text::edit::later_change(&name));
                }
            }
            Err(e) => out.failed.push(FileFailure {
                id,
                name,
                message: e.message,
            }),
        }
    }
    if several {
        progress(SaveProgress {
            done: u32::try_from(out.saved.len() + out.failed.len()).unwrap_or(u32::MAX),
            total,
            current: None,
        });
    }
    out.revision = if out.saved.is_empty() {
        album.revision()
    } else {
        album.next_revision()
    };
    Ok(out)
}

/// `edit_save_all()`: every photo with an edit, in path order. Copy-only
/// photos are skipped and counted; photos already being saved are left to
/// that save.
pub fn save_all(
    album: &Album,
    env: &SaveEnv<'_>,
    progress: &dyn Fn(SaveProgress),
) -> AppResult<SavedEdits> {
    check_writable(album)?;
    let rows = album.db().read(|conn| {
        let failed = sqlite_error(text::edit::READING);
        let mut st = conn
            .prepare(
                "SELECT id, format FROM photos WHERE edit IS NOT NULL AND missing = 0
                 ORDER BY rel_path",
            )
            .map_err(&failed)?;
        st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .map_err(&failed)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(&failed)
    })?;
    let mut skipped = 0;
    let ids: Vec<i64> = rows
        .into_iter()
        .filter(|(id, format)| {
            let in_place = format_by_id(format).is_some_and(|f| f.caps.edit_in_place);
            if !in_place {
                skipped += 1;
            }
            in_place && !album.saves().is_busy(*id)
        })
        .map(|(id, _)| id)
        .collect();
    save_many(album, env, &ids, skipped, progress)
}

/// `edit_status()`: counted from the database.
pub fn status(album: &Album, target: EditTarget) -> AppResult<EditStatus> {
    let edited: i64 = album.db().read(|conn| {
        conn.query_row(
            "SELECT count(*) FROM photos WHERE edit IS NOT NULL AND missing = 0",
            [],
            |r| r.get(0),
        )
        .map_err(sqlite_error(text::edit::READING))
    })?;
    let edited = edited.unsigned_abs();
    Ok(EditStatus {
        edited,
        waiting: if target == EditTarget::File {
            edited
        } else {
            0
        },
        saving: album.saves().count(),
    })
}

// ---------------------------------------------------------------------------
// Save as…
// ---------------------------------------------------------------------------

/// What the Save dialog of `edit_save_as` shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveAsDialog {
    /// The photo's own folder.
    pub folder: PathBuf,
    /// `<name>-edited<ext>`.
    pub default_name: String,
    /// "<Format label> photo".
    pub filter: String,
    /// The written format's extensions, without the dot.
    pub extensions: Vec<String>,
}

/// The extensions a copy of `format` may carry, with the dot; the first
/// is the default.
fn copy_extensions(format: &MediaFormat) -> Vec<&'static str> {
    let target = format.caps.save_as;
    if registry_of(target).is_some_and(|r| r.id == format.id) {
        format.extensions.to_vec()
    } else {
        registry_of(target).map_or_else(|| vec![extension_of(target)], |r| r.extensions.to_vec())
    }
}

/// The dialog for Save as… of photo `id`. Errors: `photo_not_found`,
/// `edit_not_editable`, `edit_none`.
pub fn save_as_dialog(album: &Album, id: i64) -> AppResult<SaveAsDialog> {
    let photo = load(album, id)?;
    if let Some(reason) = photo.not_editable {
        return Err(AppError::new(ErrorCode::EditNotEditable, reason));
    }
    if photo.edit.is_none() {
        return Err(AppError::new(
            ErrorCode::EditNone,
            text::edit::save_none(&photo.name),
        ));
    }
    let path = absolute(album, &photo.rel_path);
    let stem = Path::new(&photo.name)
        .file_stem()
        .map_or_else(|| photo.name.clone(), |s| s.to_string_lossy().into_owned());
    let own_ext = Path::new(&photo.name)
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let exts = copy_extensions(photo.format);
    let ext = if exts.contains(&own_ext.to_lowercase().as_str()) {
        own_ext
    } else {
        exts[0].to_owned()
    };
    let target = registry_of(photo.format.caps.save_as);
    Ok(SaveAsDialog {
        folder: path.parent().map(Path::to_path_buf).unwrap_or_default(),
        default_name: format!("{stem}-edited{ext}"),
        filter: text::edit::save_as_filter(target.map_or(photo.format.label, |t| t.label)),
        extensions: exts
            .iter()
            .map(|e| e.trim_start_matches('.').to_owned())
            .collect(),
    })
}

/// `chosen` with its extension forced to fit `exts` (with the dot, the
/// first the default). Answers the path and whether it changed.
pub fn force_extension(chosen: &Path, exts: &[&str]) -> (PathBuf, bool) {
    let ext = chosen
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()));
    if ext.as_deref().is_some_and(|e| exts.contains(&e)) {
        return (chosen.to_path_buf(), false);
    }
    let mut name = chosen
        .file_stem()
        .map(|s| s.to_os_string())
        .unwrap_or_default();
    name.push(exts[0]);
    (chosen.with_file_name(name), true)
}

/// Whether two paths name the same file, ignoring case on Windows.
pub fn same_path(a: &Path, b: &Path) -> bool {
    if let (Ok(a), Ok(b)) = (fs::canonicalize(a), fs::canonicalize(b)) {
        return a == b;
    }
    let (a, b) = (a.to_string_lossy(), b.to_string_lossy());
    if cfg!(windows) {
        a.replace('/', "\\").to_lowercase() == b.replace('/', "\\").to_lowercase()
    } else {
        a == b
    }
}

/// The path of `abs` relative to the album root with `/` separators, when
/// it lies inside the album's scope (and not in `.owlbum`).
fn in_album(album: &Album, abs: &Path) -> Option<String> {
    let loc = album.location();
    let root = fs::canonicalize(&loc.root).unwrap_or_else(|_| loc.root.clone());
    let parent = fs::canonicalize(abs.parent()?).ok()?;
    let rel_dir = parent.strip_prefix(&root).ok()?;
    let mut parts: Vec<String> = rel_dir
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    if parts.first().is_some_and(|p| p == ".owlbum") {
        return None;
    }
    parts.push(abs.file_name()?.to_string_lossy().into_owned());
    let rel = parts.join("/");
    walk::is_under(&rel, &loc.scope).then_some(rel)
}

/// `edit_save_as` after the dialog: writes the edited copy of photo `id` to
/// `chosen`. The original and its edit are left as they are. A copy inside
/// the album is read by `rescan` (given its folder) and gets the
/// original's typed info, except in a view-only album.
pub fn save_as(
    album: &Album,
    env: &SaveEnv<'_>,
    id: i64,
    chosen: &Path,
    rescan: &dyn Fn(&str),
) -> AppResult<SavedAs> {
    let photo = load(album, id)?;
    if let Some(reason) = photo.not_editable {
        return Err(AppError::new(ErrorCode::EditNotEditable, reason));
    }
    let Some(edit) = photo.edit else {
        return Err(AppError::new(
            ErrorCode::EditNone,
            text::edit::save_none(&photo.name),
        ));
    };
    let exts = copy_extensions(photo.format);
    let (target, forced) = force_extension(chosen, &exts);
    if forced && target.exists() {
        return Err(AppError::new(
            ErrorCode::SaveAsExists,
            text::edit::SAVE_AS_EXISTS,
        ));
    }
    let original = absolute(album, &photo.rel_path);
    if same_path(&target, &original) {
        return Err(AppError::new(
            ErrorCode::SaveAsSameFile,
            text::edit::SAVE_AS_SAME,
        ));
    }
    let copy_failed = |reason: String| {
        AppError::new(
            ErrorCode::EditRenderFailed,
            text::edit::copy_failed(&reason),
        )
    };
    let format = photo.format.caps.save_as;
    let render = render_path(env, format)?;
    if let Err(e) = (env.render)(&original, photo.format, &edit, format, &render) {
        let _ = fs::remove_file(&render);
        return Err(copy_failed(e.message));
    }
    // An existing file the user agreed to replace goes to the bin first,
    // never overwritten.
    if target.exists()
        && let Err(reason) = (env.bin)(&target)
    {
        let _ = fs::remove_file(&render);
        return Err(AppError::new(
            ErrorCode::EditOriginalNotMoved,
            text::edit::replaced_not_moved(&reason),
        ));
    }
    if let Err(e) = move_into_place(&render, &target, env) {
        let _ = fs::remove_file(&render);
        return Err(copy_failed(e.to_string()));
    }
    let saved_name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    tracing::info!(target: "edit", "save as: photo {id} written as a copy");
    let mut out = SavedAs {
        saved_name: Some(saved_name.clone()),
        revision: album.revision(),
        ..SavedAs::default()
    };
    let Some(rel) = in_album(album, &target) else {
        return Ok(out);
    };
    out.in_album = true;
    rescan(crate::scan::rows::rel_dir(&rel));
    let copy_id = album.db().read({
        let rel = rel.clone();
        move |conn| {
            conn.query_row(
                "SELECT id FROM photos WHERE rel_path = ?1 AND missing = 0 AND scan_error IS NULL",
                [rel],
                |r| r.get::<_, i64>(0),
            )
            .optional()
            .map_err(sqlite_error(text::edit::READING))
        }
    });
    let copy_id = match copy_id {
        Ok(Some(c)) => c,
        Ok(None) => {
            out.warning = Some(text::edit::copy_not_read(
                &saved_name,
                "it was not found when the folder was read",
            ));
            return Ok(out);
        }
        Err(e) => {
            out.warning = Some(text::edit::copy_not_read(&saved_name, &e.message));
            return Ok(out);
        }
    };
    out.photo_id = Some(copy_id);
    if album.is_view_only() {
        return Ok(out);
    }
    match album.write_photos(move |tx, touched| {
        copy_typed(tx, id, copy_id)?;
        touched.photo(copy_id);
        Ok(())
    }) {
        Ok(()) => out.revision = album.next_revision(),
        Err(e) => out.warning = Some(text::edit::copy_info_failed(&saved_name, &e.message)),
    }
    Ok(out)
}

/// Copies star, people, date, place, description and notes from photo
/// `from` to photo `to`. Not hidden, the score nudge, the edit or tags.
fn copy_typed(tx: &Transaction<'_>, from: i64, to: i64) -> AppResult<()> {
    let failed = sqlite_error(text::album::WRITING_DATABASE);
    tx.execute(
        "UPDATE photos SET (star, date_override, loc_lat, loc_lon, loc_name, description, notes, info_updated_at) =
            (SELECT star, date_override, loc_lat, loc_lon, loc_name, description, notes, info_updated_at
             FROM photos WHERE id = ?1)
         WHERE id = ?2",
        params![from, to],
    )
    .map_err(&failed)?;
    tx.execute(
        "INSERT OR IGNORE INTO photo_people (photo_id, person_id)
         SELECT ?2, person_id FROM photo_people WHERE photo_id = ?1",
        params![from, to],
    )
    .map_err(&failed)?;
    crate::album::search::reindex(tx, to)
}

// ---------------------------------------------------------------------------
// Recovery
// ---------------------------------------------------------------------------

/// Settles every save record of a writable album before its first scan
/// (docs/06, "The save record and recovery"), then deletes renders older
/// than an hour that no record names. Answers the messages to show.
pub fn recover(album: &Album, env: &SaveEnv<'_>) -> Vec<String> {
    if album.is_view_only() {
        return Vec::new();
    }
    let mut notices = Vec::new();
    let records = records(album).unwrap_or_else(|e| {
        tracing::warn!(target: "edit", "save records could not be read: {e}");
        Vec::new()
    });
    let mut kept = HashSet::new();
    for (id, record) in records {
        match settle(album, env, id, &record) {
            Ok(Some(notice)) => notices.push(notice),
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(target: "edit", "the save record of photo {id} could not be settled: {e}");
                kept.insert(record.render.clone());
            }
        }
    }
    clean_renders(&env.renders, &kept, RENDER_KEPT);
    notices
}

fn settle(
    album: &Album,
    env: &SaveEnv<'_>,
    id: i64,
    record: &SaveRecord,
) -> AppResult<Option<String>> {
    let row = album.db().read(move |conn| {
        conn.query_row(
            "SELECT rel_path, file_name FROM photos WHERE id = ?1",
            [id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(sqlite_error(text::edit::READING))
    })?;
    let Some((rel_path, name)) = row else {
        delete_record(album, id)?;
        return Ok(None);
    };
    let path = absolute(album, &rel_path);
    let saved = super::parse_edit(&record.edit);
    let render = env.renders.join(&record.render);
    if path.exists() {
        let key = keys::sample(&path).map(|s| s.file_key).unwrap_or_default();
        if key == record.file_key {
            // The save never replaced the file: the edit stays.
            delete_record(album, id)?;
        } else {
            // It did: finish step 7.
            let saved = saved.unwrap_or(super::IDENTITY);
            finish(album, id, &path, &saved, None)?;
            album.undo().drop_edits_of(&HashSet::from([id]));
            let _ = fs::remove_file(&render);
        }
        tracing::info!(target: "edit", "save record of photo {id} settled: the file was there");
        return Ok(None);
    }
    if render.is_file() {
        move_into_place(&render, &path, env).map_err(|e| {
            AppError::new(
                ErrorCode::EditNotPlaced,
                text::edit::not_placed(&name, &e.to_string()),
            )
        })?;
        let saved = saved.unwrap_or(super::IDENTITY);
        finish(album, id, &path, &saved, None)?;
        tracing::info!(target: "edit", "save record of photo {id} settled: the render was put in place");
        return Ok(None);
    }
    delete_record(album, id)?;
    tracing::warn!(target: "edit", "save record of photo {id}: the original is in the bin and the render is gone");
    Ok(Some(text::edit::interrupted(&name)))
}

/// Deletes files in `renders` older than `kept_for`, except those named in
/// `keep`.
pub fn clean_renders(renders: &Path, keep: &HashSet<String>, kept_for: Duration) {
    let Ok(dir) = fs::read_dir(renders) else {
        return;
    };
    let now = SystemTime::now();
    for entry in dir.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if keep.contains(&name) {
            continue;
        }
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|m| now.duration_since(m).ok())
            .is_some_and(|age| age >= kept_for);
        if old {
            let _ = fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests;
